use crate::error::LSError;
use crate::spec::AdapterConfiguration;
use crate::spec::AdapterId;
use crate::spec::DetectWorkspaceRootResult;
use crate::spec::DiscoverResult;
use crate::spec::RunFileTestResult;
use crate::spec::RunFileTestResultItem;
use crate::spec::WorkspaceAnalysis;
use crate::util::send_stdout;
use glob::glob;
use glob::Pattern;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticOptions;
use lsp_types::DiagnosticServerCapabilities;
use lsp_types::DiagnosticSeverity;
use lsp_types::InitializeParams;
use lsp_types::InitializeResult;
use lsp_types::NumberOrString;
use lsp_types::Position;
use lsp_types::ProgressParams;
use lsp_types::ProgressParamsValue;
use lsp_types::PublishDiagnosticsParams;
use lsp_types::Range;
use lsp_types::ServerCapabilities;
use lsp_types::TextDocumentSyncCapability;
use lsp_types::TextDocumentSyncKind;
use lsp_types::Url;
use lsp_types::WorkDoneProgress;
use lsp_types::WorkDoneProgressBegin;
use lsp_types::WorkDoneProgressCreateParams;
use lsp_types::WorkDoneProgressEnd;
use lsp_types::WorkDoneProgressOptions;
use serde::de::Error;
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::io::BufRead;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InitializedOptions {
    adapter_command: HashMap<AdapterId, Vec<AdapterConfiguration>>,
    project_dir: Option<PathBuf>,
}

pub struct TestingLS {
    pub initialize_params: InitializeParams,
    pub options: InitializedOptions,
    pub workspace_root_cache: Vec<WorkspaceAnalysis>,
}

impl Default for TestingLS {
    fn default() -> Self {
        Self::new()
    }
}

impl TestingLS {
    pub fn new() -> Self {
        Self {
            initialize_params: Default::default(),
            options: Default::default(),
            workspace_root_cache: Vec::new(),
        }
    }

    pub fn main_loop(&mut self) -> Result<(), LSError> {
        loop {
            let mut size = 0;
            'read_header: loop {
                let mut buffer = String::new();
                let stdin = io::stdin();
                let mut handle = stdin.lock(); // We get `StdinLock` here.
                handle.read_line(&mut buffer)?;

                if buffer.is_empty() {
                    tracing::warn!("buffer is empty")
                }

                // The end of header section
                if buffer == "\r\n" {
                    break 'read_header;
                }

                let splitted: Vec<&str> = buffer.split(' ').collect();

                if splitted.len() != 2 {
                    tracing::warn!("unexpected");
                }

                let header_name = splitted[0].to_lowercase();
                let header_value = splitted[1].trim();

                match header_name.as_ref() {
                    "content-length" => {}
                    "content-type:" => {}
                    _ => {}
                }

                size = header_value.parse::<usize>().unwrap();
            }

            let stdin = io::stdin();
            let mut handle = stdin.lock();
            let mut buf = vec![0u8; size];
            handle.read_exact(&mut buf).unwrap();
            let message = String::from_utf8(buf).unwrap();

            let value: Value = serde_json::from_str(&message)?;
            let method = &value["method"].as_str();
            let params = &value["params"];

            if let Some(method) = method {
                match *method {
                    "initialize" => {
                        self.initialize_params = InitializeParams::deserialize(params)?;
                        self.options = (self.handle_initialization_options(
                            self.initialize_params.initialization_options.as_ref(),
                        ))?;
                        let id = value["id"].as_i64().unwrap();
                        self.initialize(id)?;
                    }
                    "workspace/diagnostic" => {
                        self.check_workspace()?;
                    }
                    "textDocument/diagnostic" | "textDocument/didSave" => {
                        let uri = params["textDocument"]["uri"]
                            .as_str()
                            .ok_or(serde_json::Error::custom("`textDocument.uri` is not set"))?;
                        self.check_file(uri, false)?;
                    }
                    "$/runFileTest" => {
                        let uri = params["uri"]
                            .as_str()
                            .ok_or(serde_json::Error::custom("`uri` is not set"))?;
                        self.check_file(uri, false)?;
                    }
                    "$/discoverFileTest" => {
                        let id = value["id"].as_i64().unwrap();
                        let uri = params["uri"]
                            .as_str()
                            .ok_or(serde_json::Error::custom("`uri` is not set"))?;
                        let result = self.discover_file(uri)?;
                        send_stdout(&json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": result,
                        }))?;
                    }
                    _ => {}
                }
            }
        }
    }

    fn adapter_commands(&self) -> HashMap<AdapterId, Vec<AdapterConfiguration>> {
        self.options.adapter_command.clone()
    }

    fn project_files(
        base_dir: &Path,
        include_pattern: &[String],
        exclude_pattern: &[String],
    ) -> Vec<String> {
        let mut result: Vec<String> = vec![];
        let base_dir = base_dir.to_string_lossy().to_string();

        let exclude_pattern = exclude_pattern
            .iter()
            .filter_map(|exclude_pattern| {
                Pattern::new(&format!("!{base_dir}{exclude_pattern}")).ok()
            })
            .collect::<Vec<Pattern>>();
        for include_pattern in include_pattern {
            let matched = glob(format!("{base_dir}{include_pattern}").as_str());
            if let Ok(entries) = matched {
                for path in entries.flatten() {
                    let should_exclude = exclude_pattern
                        .iter()
                        .any(|exclude_pattern| exclude_pattern.matches(path.to_str().unwrap()));
                    if !should_exclude {
                        result.push(path.display().to_string());
                    }
                }
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
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            ..ServerCapabilities::default()
        }
    }

    pub fn handle_initialization_options(
        &self,
        options: Option<&Value>,
    ) -> Result<InitializedOptions, LSError> {
        if let Some(options) = options {
            Ok(serde_json::from_value(options.clone())?)
        } else {
            Err(LSError::Any(anyhow::anyhow!(
                "Invalid initialization options"
            )))
        }
    }

    pub fn initialize(&self, id: i64) -> Result<(), LSError> {
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

    pub fn refresh_workspace_root_cache(&mut self) -> Result<(), LSError> {
        let adapter_commands = self.adapter_commands();
        let default_workspace_root = self
            .initialize_params
            .clone()
            .workspace_folders
            .ok_or(LSError::Any(anyhow::anyhow!("No workspace folders found")))?;
        let default_workspace_uri = default_workspace_root[0].uri.clone();
        let project_dir = self
            .options
            .project_dir
            .clone()
            .unwrap_or(default_workspace_uri.to_file_path().unwrap());
        self.workspace_root_cache = vec![];
        // Nested and multiple loops, but each count is small
        for adapter_commands in adapter_commands.values() {
            for adapter in adapter_commands {
                let &AdapterConfiguration {
                    path,
                    extra_args,
                    envs,
                    include_patterns,
                    exclude_patterns,
                } = &adapter;
                let file_paths =
                    Self::project_files(&project_dir, include_patterns, exclude_patterns);
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
                    .arg("detect-workspace-root")
                    .args(args_file_path)
                    .arg("--")
                    .args(extra_args)
                    .envs(envs)
                    .output()
                    .map_err(|err| LSError::Adapter(err.to_string()))?;
                let adapter_result = String::from_utf8(output.stdout)
                    .map_err(|err| LSError::Adapter(err.to_string()))?;
                let workspace_root: DetectWorkspaceRootResult =
                    serde_json::from_str(&adapter_result)?;
                self.workspace_root_cache
                    .push(WorkspaceAnalysis::new(adapter.clone(), workspace_root))
            }
        }
        send_stdout(&json!({
            "jsonrpc": "2.0",
            "method": "$/detectedWorkspaceRoots",
            "params": self.workspace_root_cache,
        }))?;
        Ok(())
    }

    pub fn check_workspace(&mut self) -> Result<(), LSError> {
        self.refresh_workspace_root_cache()?;

        self.workspace_root_cache.iter().for_each(
            |WorkspaceAnalysis {
                 adapter_config: adapter,
                 workspace_roots: workspaces,
             }| {
                workspaces.iter().for_each(|(workspace_root, paths)| {
                    let _ = self.check(adapter, workspace_root, paths);
                })
            },
        );
        Ok(())
    }

    pub fn check_file(&mut self, path: &str, refresh_needed: bool) -> Result<(), LSError> {
        let path = path.replace("file://", "");
        if refresh_needed {
            self.refresh_workspace_root_cache()?;
        }
        self.workspace_root_cache.iter().for_each(
            |WorkspaceAnalysis {
                 adapter_config: adapter,
                 workspace_roots: workspaces,
             }| {
                for (workspace_root, paths) in workspaces.iter() {
                    if !paths.contains(&path.to_string()) {
                        continue;
                    }
                    let _ = self.check(adapter, workspace_root, paths);
                }
            },
        );
        Ok(())
    }

    fn get_diagnostics(
        &self,
        adapter: &AdapterConfiguration,
        workspace_root: &str,
        paths: &[String],
    ) -> Result<Vec<(String, Vec<Diagnostic>)>, LSError> {
        let mut adapter_command = Command::new(&adapter.path);
        let mut diagnostics: Vec<(String, Vec<Diagnostic>)> = vec![];
        let cwd = PathBuf::from(workspace_root);
        let adapter_command = adapter_command.current_dir(&cwd);
        let mut args: Vec<&str> = vec!["--workspace-root", cwd.to_str().unwrap()];
        paths.iter().for_each(|path| {
            args.push("--file-paths");
            args.push(path);
        });

        let output = adapter_command
            .arg("run-file-test")
            .args(args)
            .arg("--")
            .args(&adapter.extra_args)
            .envs(&adapter.envs)
            .output()
            .map_err(|err| LSError::Adapter(err.to_string()))?;
        let Output { stdout, stderr, .. } = output;
        if !stderr.is_empty() {
            let message = "Cannot run test command: \n".to_string()
                + &String::from_utf8(stderr.clone()).unwrap();
            let placeholder_diagnostic = Diagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                message,
                severity: Some(DiagnosticSeverity::WARNING),
                code_description: None,
                code: None,
                source: None,
                tags: None,
                related_information: None,
                data: None,
            };
            for path in paths {
                diagnostics.push((path.to_string(), vec![placeholder_diagnostic.clone()]));
            }
        }

        let adapter_result =
            String::from_utf8(stdout).map_err(|err| LSError::Adapter(err.to_string()))?;
        if let Ok(res) = serde_json::from_str::<RunFileTestResult>(&adapter_result) {
            for target_file in paths {
                let diagnostics_for_file: Vec<Diagnostic> = res
                    .clone()
                    .iter()
                    .filter(|RunFileTestResultItem { path, .. }| path == target_file)
                    .flat_map(|RunFileTestResultItem { diagnostics, .. }| diagnostics.clone())
                    .collect();
                let uri = Url::from_file_path(target_file.replace("file://", "")).unwrap();
                diagnostics.push((uri.to_string(), diagnostics_for_file));
            }
        }
        Ok(diagnostics)
    }

    fn check(
        &self,
        adapter: &AdapterConfiguration,
        workspace_root: &str,
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
            title: format!("Testing by adapter: {}", adapter.path),
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
        let diagnostics = self.get_diagnostics(adapter, workspace_root, paths)?;
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
    fn discover_file(&self, path: &str) -> Result<DiscoverResult, LSError> {
        let path = path.replace("file://", "");
        let target_paths = vec![path.to_string()];
        let mut result: DiscoverResult = vec![];
        for WorkspaceAnalysis {
            adapter_config: adapter,
            workspace_roots: workspaces,
        } in &self.workspace_root_cache
        {
            for (_, paths) in workspaces.iter() {
                if !paths.contains(&path.to_string()) {
                    continue;
                }
                result.extend(self.discover(adapter, &target_paths)?);
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
            .args(&adapter.extra_args)
            .envs(&adapter.envs)
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
}

#[cfg(test)]
mod tests {
    use crate::util::extension_from_url_str;
    use lsp_types::{Url, WorkspaceFolder};
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_check_file() {
        let abs_path_of_test_proj = std::env::current_dir().unwrap().join("test_proj/rust");
        let mut server = TestingLS {
            initialize_params: InitializeParams {
                workspace_folders: Some(vec![WorkspaceFolder {
                    uri: Url::from_file_path(&abs_path_of_test_proj).unwrap(),
                    name: "test_proj".to_string(),
                }]),
                ..InitializeParams::default()
            },
            options: InitializedOptions {
                adapter_command: HashMap::from([(String::from(".rs"), vec![])]),
                project_dir: None,
            },
            workspace_root_cache: Vec::new(),
        };
        let librs = abs_path_of_test_proj.join("lib.rs");
        server.check_file(librs.to_str().unwrap(), true).unwrap();
    }

    #[test]
    fn test_check_workspace() {
        let abs_path_of_test_proj = std::env::current_dir().unwrap().join("test_proj/rust");
        let abs_path_of_rust_adapter = std::env::current_dir()
            .unwrap()
            .join("target/debug/testing-ls-adapter");
        let abs_path_of_rust_adapter = abs_path_of_rust_adapter
            .into_os_string()
            .into_string()
            .unwrap();
        let adapter_conf = AdapterConfiguration {
            path: abs_path_of_rust_adapter,
            extra_args: vec!["--test-kind=cargo-test".to_string()],
            envs: HashMap::new(),
            include_patterns: vec![],
            exclude_patterns: vec![],
        };
        let mut server = TestingLS {
            initialize_params: InitializeParams {
                workspace_folders: Some(vec![WorkspaceFolder {
                    uri: Url::from_file_path(abs_path_of_test_proj.clone()).unwrap(),
                    name: "test_proj".to_string(),
                }]),
                ..InitializeParams::default()
            },
            options: InitializedOptions {
                adapter_command: HashMap::from([(String::from(".rs"), vec![adapter_conf])]),
                project_dir: None,
            },
            workspace_root_cache: Vec::new(),
        };
        server.check_workspace().unwrap();
        server
            .workspace_root_cache
            .iter()
            .for_each(|workspace_analysis| {
                let adapter_command_path = workspace_analysis.adapter_config.path.clone();
                assert!(adapter_command_path.contains("target/debug/testing-ls-adapter"));
                workspace_analysis
                    .workspace_roots
                    .iter()
                    .for_each(|(workspace_root, paths)| {
                        assert_eq!(workspace_root, abs_path_of_test_proj.to_str().unwrap());
                        paths.iter().for_each(|path| {
                            assert!(path.contains("rust/src"));
                        });
                    });
            });
    }

    #[test]
    fn project_files_are_filtered_by_extension() {
        let absolute_path_of_test_proj = std::env::current_dir().unwrap().join("test_proj");
        let files = TestingLS::project_files(
            &absolute_path_of_test_proj.clone(),
            &["/rust/src/lib.rs".to_string()],
            &["/rust/src/target/**/*".to_string()],
        );
        let librs = absolute_path_of_test_proj.join("rust/src/lib.rs");
        assert_eq!(files, vec![librs.to_str().unwrap()]);
        let files = TestingLS::project_files(
            &absolute_path_of_test_proj.clone(),
            &["**/*.js".to_string()],
            &["**/node_modules/**/*".to_string()],
        );
        files.iter().for_each(|file| {
            assert_eq!(extension_from_url_str(file).unwrap(), ".js");
        });
    }

    #[test]
    fn bubble_adapter_error() {
        let adapter_conf: AdapterConfiguration = AdapterConfiguration {
            path: std::env::current_dir()
                .unwrap()
                .join("target/debug/testing-ls-adapter")
                .to_str()
                .unwrap()
                .to_string(),
            extra_args: vec!["--invalid-arg".to_string()],
            envs: HashMap::new(),
            include_patterns: vec![],
            exclude_patterns: vec![],
        };
        let abs_path_of_test_proj = std::env::current_dir().unwrap().join("test_proj/rust");
        let files = TestingLS::project_files(
            &abs_path_of_test_proj.clone(),
            &["/**/*.rs".to_string()],
            &[],
        );

        let server = TestingLS {
            initialize_params: InitializeParams {
                workspace_folders: Some(vec![WorkspaceFolder {
                    uri: Url::from_file_path(&abs_path_of_test_proj).unwrap(),
                    name: "test_proj".to_string(),
                }]),
                ..InitializeParams::default()
            },
            options: InitializedOptions {
                adapter_command: HashMap::from([(String::from(".rs"), vec![adapter_conf.clone()])]),
                project_dir: None,
            },
            workspace_root_cache: Vec::new(),
        };
        let diagnostics = server
            .get_diagnostics(
                &adapter_conf,
                abs_path_of_test_proj.to_str().unwrap(),
                &files,
            )
            .unwrap();
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = diagnostics.first().unwrap().1.first().unwrap();
        assert_eq!(diagnostic.severity.unwrap(), DiagnosticSeverity::WARNING);
        assert!(diagnostic.message.contains("Cannot run test command:"));
    }
}
