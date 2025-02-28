use crate::error::LSError;
use crate::spec::*;
use crate::util::resolve_path;
use crate::util::send_stdout;
use glob::Pattern;
use lsp_types::*;
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::env::current_dir;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use testing_language_server::spec::DiscoverResult;

const TOML_FILE_NAME: &str = ".testingls.toml";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InitializedOptions {
    adapter_command: HashMap<AdapterId, AdapterConfiguration>,
    enable_workspace_diagnostics: Option<bool>,
}

pub struct TestingLS {
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
    pub options: InitializedOptions,
    pub workspaces_cache: Vec<WorkspaceAnalysis>,
}

impl Default for TestingLS {
    fn default() -> Self {
        Self::new()
    }
}

/// The status of workspace diagnostics
/// - Skipped: Skip workspace diagnostics (when `enable_workspace_diagnostics` is false)
/// - Done: Finish workspace diagnostics (when `enable_workspace_diagnostics` is true)
#[derive(Debug, PartialEq, Eq)]
pub enum WorkspaceDiagnosticsStatus {
    Skipped,
    Done,
}

impl TestingLS {
    pub fn new() -> Self {
        Self {
            workspace_folders: None,
            options: Default::default(),
            workspaces_cache: Vec::new(),
        }
    }

    fn project_dir(&self) -> Result<PathBuf, LSError> {
        let cwd = current_dir();
        if let Ok(cwd) = cwd {
            Ok(cwd)
        } else {
            let default_project_dir = self
                .workspace_folders
                .as_ref()
                .ok_or(LSError::Any(anyhow::anyhow!("No workspace folders found")))?;
            let default_workspace_uri = &default_project_dir[0].uri;
            Ok(default_workspace_uri.to_file_path().unwrap())
        }
    }

    pub fn initialize(
        &mut self,
        id: i64,
        initialize_params: InitializeParams,
    ) -> Result<(), LSError> {
        self.workspace_folders = initialize_params.workspace_folders;
        self.options = (self
            .handle_initialization_options(initialize_params.initialization_options.as_ref()))?;
        let result = InitializeResult {
            capabilities: self.build_capabilities(),
            ..InitializeResult::default()
        };

        send_stdout(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
        }))?;

        Ok(())
    }

    fn adapter_commands(&self) -> HashMap<AdapterId, AdapterConfiguration> {
        self.options.adapter_command.clone()
    }

    fn project_files(base_dir: &Path, include: &[String], exclude: &[String]) -> Vec<String> {
        let mut result: Vec<String> = vec![];

        let exclude_pattern = exclude
            .iter()
            .filter_map(|exclude_pattern| {
                Pattern::new(base_dir.join(exclude_pattern).to_str().unwrap()).ok()
            })
            .collect::<Vec<Pattern>>();
        let base_dir = base_dir.to_str().unwrap();
        let entries = globwalk::GlobWalkerBuilder::from_patterns(base_dir, include)
            .follow_links(true)
            .build()
            .unwrap()
            .filter_map(Result::ok);
        for path in entries {
            let should_exclude = exclude_pattern
                .iter()
                .any(|exclude_pattern| exclude_pattern.matches(path.path().to_str().unwrap()));
            if !should_exclude {
                result.push(path.path().to_str().unwrap().to_owned());
            }
        }
        result
    }

    fn build_capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: None,
                inter_file_dependencies: false,
                workspace_diagnostics: true,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::NONE)),
            ..ServerCapabilities::default()
        }
    }

    pub fn handle_initialization_options(
        &self,
        options: Option<&Value>,
    ) -> Result<InitializedOptions, LSError> {
        let project_dir = self.project_dir()?;
        let toml_path = project_dir.join(TOML_FILE_NAME);
        let toml_content = std::fs::read_to_string(toml_path);
        match toml_content {
            Ok(toml_content) => Ok(toml::from_str::<InitializedOptions>(&toml_content).unwrap()),
            Err(_) => {
                if let Some(options) = options {
                    Ok(serde_json::from_value(options.clone())?)
                } else {
                    Err(LSError::Any(anyhow::anyhow!(
                        "Invalid initialization options"
                    )))
                }
            }
        }
    }

    pub fn refresh_workspaces_cache(&mut self) -> Result<(), LSError> {
        let adapter_commands = self.adapter_commands();
        let project_dir = self.project_dir()?;
        self.workspaces_cache = vec![];
        // Nested and multiple loops, but each count is small
        for adapter in adapter_commands.into_values() {
            let AdapterConfiguration {
                path,
                extra_arg,
                env,
                include,
                exclude,
                workspace_dir,
                ..
            } = &adapter;
            let file_paths = Self::project_files(&project_dir, include, exclude);
            if file_paths.is_empty() {
                continue;
            }
            let mut adapter_command = Command::new(path);
            let mut args_file_path: Vec<&str> = vec![];
            file_paths.iter().for_each(|file_path| {
                args_file_path.push("--file-paths");
                args_file_path.push(file_path);
            });
            let output = adapter_command
                .arg("detect-workspace")
                .args(args_file_path)
                .arg("--")
                .args(extra_arg)
                .envs(env)
                .output()
                .map_err(|err| LSError::Adapter(err.to_string()))?;
            let adapter_result = String::from_utf8(output.stdout)
                .map_err(|err| LSError::Adapter(err.to_string()))?;
            let workspace: DetectWorkspaceResult = match serde_json::from_str(&adapter_result) {
                Ok(result) => result,
                Err(err) => {
                    let stderr = String::from_utf8(output.stderr);
                    tracing::error!("Failed to parse adapter result: {:?}", err);
                    tracing::error!("Error: {:?}", stderr);
                    return Err(LSError::Adapter(err.to_string()));
                }
            };
            let workspace = if let Some(workspace_dir) = workspace_dir {
                let workspace_dir = resolve_path(&project_dir, workspace_dir)
                    .to_str()
                    .unwrap()
                    .to_string();
                let target_paths = workspace
                    .data
                    .into_iter()
                    .flat_map(|kv| kv.1)
                    .collect::<Vec<_>>();
                HashMap::from([(workspace_dir, target_paths)])
            } else {
                workspace.data
            };
            self.workspaces_cache.push(WorkspaceAnalysis::new(
                adapter,
                DetectWorkspaceResult { data: workspace },
            ))
        }
        tracing::info!("workspaces_cache={:#?}", self.workspaces_cache);
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "method": "$/detectedWorkspace",
            "params": self.workspaces_cache,
        }))?;
        Ok(())
    }

    /// Diagnoses the entire workspace for diagnostics.
    /// This function will refresh the workspace cache, check if workspace diagnostics are enabled,
    /// and then iterate through all workspaces to diagnose them.
    /// It will trigger the publication of diagnostics for all files in the workspace
    /// through the Language Server Protocol.
    pub fn diagnose_workspace(&mut self) -> Result<WorkspaceDiagnosticsStatus, LSError> {
        self.refresh_workspaces_cache()?;
        if !self.options.enable_workspace_diagnostics.unwrap_or(true) {
            return Ok(WorkspaceDiagnosticsStatus::Skipped);
        }

        self.workspaces_cache.iter().for_each(
            |WorkspaceAnalysis {
                 adapter_config: adapter,
                 workspaces,
             }| {
                workspaces.data.iter().for_each(|(workspace, paths)| {
                    let _ = self.diagnose(adapter, workspace, paths);
                })
            },
        );
        Ok(WorkspaceDiagnosticsStatus::Done)
    }

    pub fn refreshing_needed(&self, path: &str) -> bool {
        let base_dir = self.project_dir();
        match base_dir {
            Ok(base_dir) => self.workspaces_cache.iter().any(|cache| {
                let include = &cache.adapter_config.include;
                let exclude = &cache.adapter_config.exclude;
                if cache
                    .workspaces
                    .data
                    .iter()
                    .any(|(_, workspace)| workspace.contains(&path.to_string()))
                {
                    return false;
                }

                Self::project_files(&base_dir, include, exclude).contains(&path.to_owned())
            }),
            Err(e) => {
                tracing::error!("Error: {:?}", e);
                false
            }
        }
    }

    /// Checks a specific file for diagnostics, optionally refreshing the workspace cache.
    /// This function will trigger the publication of diagnostics for the specified file
    /// through the Language Server Protocol.
    pub fn check_file(&mut self, path: &str, refresh_needed: bool) -> Result<(), LSError> {
        if refresh_needed || self.workspaces_cache.is_empty() {
            self.refresh_workspaces_cache()?;
        }
        self.workspaces_cache.iter().for_each(
            |WorkspaceAnalysis {
                 adapter_config: adapter,
                 workspaces,
             }| {
                for (workspace, paths) in workspaces.data.iter() {
                    if !paths.contains(&path.to_string()) {
                        continue;
                    }
                    let _ = self.diagnose(adapter, workspace, &[path.to_string()]);
                }
            },
        );
        Ok(())
    }

    fn get_diagnostics(
        &self,
        adapter: &AdapterConfiguration,
        workspace: &str,
        paths: &[String],
    ) -> Result<Vec<(String, Vec<Diagnostic>)>, LSError> {
        let mut adapter_command = Command::new(&adapter.path);
        let mut diagnostics: Vec<(String, Vec<Diagnostic>)> = vec![];
        let cwd = PathBuf::from(workspace);
        let adapter_command = adapter_command.current_dir(&cwd);
        let mut args: Vec<&str> = vec!["--workspace", cwd.to_str().unwrap()];
        paths.iter().for_each(|path| {
            args.push("--file-paths");
            args.push(path);
        });

        let output = adapter_command
            .arg("run-file-test")
            .args(args)
            .arg("--")
            .args(&adapter.extra_arg)
            .envs(&adapter.env)
            .output()
            .map_err(|err| LSError::Adapter(err.to_string()))?;
        let Output { stdout, stderr, .. } = output;
        if !stderr.is_empty() {
            let message = "Error occurred when running test via adapter.\nCheck adapter log or run tests manually".to_string();
            let message_type = MessageType::ERROR;
            let params: ShowMessageParams = ShowMessageParams {
                typ: message_type,
                message,
            };
            send_stdout(&json!({
                "jsonrpc": "2.0",
                "method": "window/showMessage",
                "params": params,
            }))
            .unwrap();
        }

        let adapter_result =
            String::from_utf8(stdout).map_err(|err| LSError::Adapter(err.to_string()))?;
        match serde_json::from_str::<RunFileTestResult>(&adapter_result) {
            Ok(res) => {
                for target_file in paths {
                    let diagnostics_for_file: Vec<Diagnostic> = res
                        .data
                        .clone()
                        .into_iter()
                        .filter(|FileDiagnostics { path, .. }| path == target_file)
                        .flat_map(|FileDiagnostics { diagnostics, .. }| diagnostics)
                        .collect();
                    let uri = Url::from_file_path(target_file.replace("file://", "")).unwrap();
                    diagnostics.push((uri.to_string(), diagnostics_for_file));
                }
            }
            Err(err) => {
                tracing::error!("Failed to parse adapter result: {:?}", err);
            }
        }
        Ok(diagnostics)
    }

    fn diagnose(
        &self,
        adapter: &AdapterConfiguration,
        workspace: &str,
        paths: &[String],
    ) -> Result<(), LSError> {
        let token = NumberOrString::String("testing-ls/start_testing".to_string());
        let progress_token = WorkDoneProgressCreateParams {
            token: token.clone(),
        };
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "window/workDoneProgress/create",
            "params": progress_token,
        }))
        .unwrap();
        let progress_begin = WorkDoneProgressBegin {
            title: "Testing".to_string(),
            cancellable: Some(false),
            message: Some(format!("testing {} files ...", paths.len())),
            percentage: Some(0),
        };
        let params = ProgressParams {
            token: token.clone(),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(progress_begin)),
        };
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": params,
        }))
        .unwrap();
        let diagnostics = self.get_diagnostics(adapter, workspace, paths)?;
        for (path, diagnostics) in diagnostics {
            self.send_diagnostics(
                Url::from_file_path(path.replace("file://", "")).unwrap(),
                diagnostics,
            )?;
        }
        let progress_end = WorkDoneProgressEnd {
            message: Some(format!("tested {} files", paths.len())),
        };
        let params = ProgressParams {
            token: token.clone(),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(progress_end)),
        };
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": params,
        }))
        .unwrap();
        Ok(())
    }

    #[allow(clippy::for_kv_map)]
    pub fn discover_file(&self, path: &str) -> Result<DiscoverResult, LSError> {
        let target_paths = vec![path.to_string()];
        let mut result: DiscoverResult = DiscoverResult { data: vec![] };
        for WorkspaceAnalysis {
            adapter_config: adapter,
            workspaces,
        } in &self.workspaces_cache
        {
            for (_, paths) in workspaces.data.iter() {
                if !paths.contains(&path.to_string()) {
                    continue;
                }
                result
                    .data
                    .extend(self.discover(adapter, &target_paths)?.data);
            }
        }
        Ok(result)
    }

    fn discover(
        &self,
        adapter: &AdapterConfiguration,
        paths: &[String],
    ) -> Result<DiscoverResult, LSError> {
        let mut adapter_command = Command::new(&adapter.path);
        let mut args: Vec<&str> = vec![];
        paths.iter().for_each(|path| {
            args.push("--file-paths");
            args.push(path);
        });
        let output = adapter_command
            .arg("discover")
            .args(args)
            .arg("--")
            .args(&adapter.extra_arg)
            .envs(&adapter.env)
            .output()
            .map_err(|err| LSError::Adapter(err.to_string()))?;

        let adapter_result =
            String::from_utf8(output.stdout).map_err(|err| LSError::Adapter(err.to_string()))?;
        Ok(serde_json::from_str(&adapter_result)?)
    }

    pub fn send_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) -> Result<(), LSError> {
        let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": params,
        }))?;
        Ok(())
    }

    pub fn shutdown(&self, id: i64) -> Result<(), LSError> {
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": null
        }))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Url, WorkspaceFolder};
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_check_file() {
        let abs_path_of_demo = std::env::current_dir().unwrap().join("demo/rust");
        let mut server = TestingLS {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(&abs_path_of_demo).unwrap(),
                name: "demo".to_string(),
            }]),
            options: InitializedOptions {
                adapter_command: HashMap::new(),
                enable_workspace_diagnostics: Some(true),
            },
            workspaces_cache: Vec::new(),
        };
        let librs = abs_path_of_demo.join("lib.rs");
        server.check_file(librs.to_str().unwrap(), true).unwrap();
    }

    #[test]
    fn test_check_workspace() {
        let abs_path_of_demo = std::env::current_dir().unwrap().join("demo/rust");
        let abs_path_of_rust_adapter = std::env::current_dir()
            .unwrap()
            .join("target/debug/testing-ls-adapter");
        let abs_path_of_rust_adapter = abs_path_of_rust_adapter
            .into_os_string()
            .into_string()
            .unwrap();
        let adapter_conf = AdapterConfiguration {
            path: abs_path_of_rust_adapter,
            extra_arg: vec!["--test-kind=cargo-test".to_string()],
            ..Default::default()
        };
        let mut server = TestingLS {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(&abs_path_of_demo).unwrap(),
                name: "demo".to_string(),
            }]),
            options: InitializedOptions {
                adapter_command: HashMap::from([(String::from(".rs"), adapter_conf)]),
                enable_workspace_diagnostics: Some(true),
            },
            workspaces_cache: Vec::new(),
        };
        server.diagnose_workspace().unwrap();
        server
            .workspaces_cache
            .iter()
            .for_each(|workspace_analysis| {
                let adapter_command_path = workspace_analysis.adapter_config.path.clone();
                assert!(adapter_command_path.contains("target/debug/testing-ls-adapter"));
                workspace_analysis
                    .workspaces
                    .data
                    .iter()
                    .for_each(|(workspace, paths)| {
                        assert_eq!(workspace, abs_path_of_demo.to_str().unwrap());
                        paths.iter().for_each(|path| {
                            assert!(path.contains("rust/src"));
                        });
                    });
            });
    }

    #[test]
    fn project_files_are_filtered_by_extension() {
        let absolute_path_of_demo = std::env::current_dir().unwrap().join("demo");
        let files = TestingLS::project_files(
            &absolute_path_of_demo.clone(),
            &["/rust/src/lib.rs".to_string()],
            &["/rust/target/**/*".to_string()],
        );
        let librs = absolute_path_of_demo.join("rust/src/lib.rs");
        assert_eq!(files, vec![librs.to_str().unwrap()]);
        let files = TestingLS::project_files(
            &absolute_path_of_demo.clone(),
            &["jest/*.spec.js".to_string()],
            &["jest/another.spec.js".to_string()],
        );
        let test_file = absolute_path_of_demo.join("jest/index.spec.js");
        assert_eq!(files, vec![test_file.to_str().unwrap()]);
    }

    #[test]
    fn skip_workspace_diagnostics() {
        let mut server = TestingLS {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(current_dir().unwrap()).unwrap(),
                name: "demo".to_string(),
            }]),
            options: InitializedOptions {
                adapter_command: HashMap::new(),
                enable_workspace_diagnostics: Some(false),
            },
            workspaces_cache: Vec::new(),
        };
        let status = server.diagnose_workspace().unwrap();
        assert_eq!(status, WorkspaceDiagnosticsStatus::Skipped);
    }
}
