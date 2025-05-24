use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range}; // Removed Location
use testing_language_server::{
    error::LSError,
    spec::{
        DetectWorkspaceArgs, DetectWorkspaceResult, DiscoverArgs, DiscoverResult,
        FileDiagnostics, FoundFileTests, RunFileTestArgs, RunFileTestResult, TestItem, // TestItem is used by FoundFileTests
    },
};
use crate::model::Runner;
use super::util::{
    detect_workspaces_from_file_list, discover_with_treesitter, send_stdout, MAX_CHAR_LENGTH,
    LOG_LOCATION,
};
use serde::Deserialize;
// serde_json is used by from_str, but it's brought in via `use serde_json;` which is fine.
// The specific warning might have been about a more specific import if present.
use std::{
    collections::HashMap,
    fs,
    io::Write, // Used by writeln!
    path::PathBuf,
    process::Command, // Used
};
use tempfile::NamedTempFile; // Used

// Structs for RSpec JSON output
#[derive(Deserialize, Debug)]
pub struct RspecReport {
    // version: Option<String>, // Removed
    // summary_line: Option<String>, // Removed
    examples: Vec<RspecExample>,
}

#[derive(Deserialize, Debug)]
pub struct RspecExample {
    // id: String, // Removed
    // description: String, // Removed
    // full_description: String, // Removed
    status: String,
    file_path: String,
    line_number: u32,
    // run_time: Option<f64>, // Removed
    // pending_message: Option<String>, // Removed
    exception: Option<RspecException>,
}

#[derive(Deserialize, Debug)]
pub struct RspecException {
    // class: String, // Removed
    message: String,
    // backtrace: Vec<String>, // Removed
}

const RSPEC_QUERY: &str = r#"
(block
  (call
    method: (identifier) @func_name (#any-of? @func_name "describe" "context")
    arguments: (arguments (string (string_content) @namespace.name))
  )
) @namespace.definition

(block
  (call
    method: (identifier) @func_name (#any-of? @func_name "it" "specify" "example")
    arguments: (arguments (string (string_content) @test.name))
  )
) @test.definition
"#; // This IS used in discover_with_treesitter call.

#[derive(Debug, Default, Eq, PartialEq)]
pub struct RspecRunner;

impl Runner for RspecRunner {
    fn detect_workspaces(
        &self,
        args: DetectWorkspaceArgs, 
    ) -> Result<(), LSError> {
        let indicator_files = vec!["Gemfile".to_string(), ".rspec".to_string()];
        let result: DetectWorkspaceResult =
            detect_workspaces_from_file_list(&args.file_paths, &indicator_files);
        send_stdout(&result)?;
        Ok(())
    }

    fn discover(&self, args: DiscoverArgs) -> Result<(), LSError> {
        let mut found_file_tests_data: Vec<FoundFileTests> = Vec::new();
        for file_path_str in args.file_paths { 
            match discover_with_treesitter(&file_path_str, &tree_sitter_ruby::language(), RSPEC_QUERY) {
                Ok(test_items_for_file) => {
                    found_file_tests_data.push(FoundFileTests {
                        path: file_path_str.clone(), 
                        tests: test_items_for_file,
                    });
                }
                Err(e) => {
                    eprintln!("Error discovering tests in file {}: {:?}", file_path_str, e);
                }
            }
        }
        let result = DiscoverResult {
            data: found_file_tests_data,
        };
        send_stdout(&result)?;
        Ok(())
    }

    fn run_file_test(&self, args: RunFileTestArgs) -> Result<(), LSError> {
        let temp_file = NamedTempFile::new().map_err(|e| LSError::Adapter(e.to_string()))?;
        let temp_file_path_obj = temp_file.path().to_path_buf(); 
        let temp_file_path_str = temp_file_path_obj.to_str().ok_or_else(|| {
            LSError::Adapter("Failed to get temp file path as string".to_string())
        })?;
        
        let mut cmd = Command::new("rspec");
        cmd.arg("-f").arg("json").arg("--out").arg(temp_file_path_str);
        cmd.args(&args.file_paths); 
        cmd.current_dir(&args.workspace); 

        let log_dir = LOG_LOCATION.join("rspec");
        fs::create_dir_all(&log_dir).map_err(|e| LSError::Adapter(format!("Failed to create log dir: {}", e)))?;
        let log_file_path = log_dir.join("rspec_command.log");
        
        if let Ok(mut log_file) = fs::File::create(&log_file_path) {
            writeln!(log_file, "Executing command: {:?}", cmd)
                .map_err(|e| LSError::Adapter(format!("Failed to write to log file: {}", e)))?;
        } else {
             return Err(LSError::Adapter(format!("Failed to create log file: {:?}", log_file_path)));
        }

        let output = cmd.output().map_err(|e| LSError::Adapter(format!("Failed to execute rspec command: {}", e)))?;

        if let Ok(mut log_file) = fs::OpenOptions::new().append(true).open(&log_file_path) {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                writeln!(log_file, "Rspec command failed. Stderr:\n{}", stderr)
                    .map_err(|e| LSError::Adapter(format!("Failed to write to log file: {}", e)))?;
            } else {
                 writeln!(log_file, "Rspec command successful.")
                    .map_err(|e| LSError::Adapter(format!("Failed to write to log file: {}", e)))?;
            }
        }

        let json_output = fs::read_to_string(&temp_file_path_obj) 
            .map_err(|e| LSError::Adapter(format!("Failed to read rspec output from temp file: {:?}, error: {}", temp_file_path_obj, e)))?;
        
        if let Ok(mut log_file) = fs::OpenOptions::new().append(true).open(&log_file_path) {
            writeln!(log_file, "Rspec JSON output:\n{}", json_output)
                .map_err(|e| LSError::Adapter(format!("Failed to write to log file: {}", e)))?;
        }

        if json_output.trim().is_empty() && !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LSError::Adapter(format!(
                "RSpec execution failed and produced no JSON output. Stderr: {}",
                stderr
            )));
        }

        let report: RspecReport = serde_json::from_str(&json_output)
            .map_err(|e| LSError::Adapter(format!("Failed to parse rspec JSON output: {}. Output: {}", e, json_output)))?;

        let result_diagnostics = self.parse_diagnostics(report, &args.file_paths, &args.workspace)?;
        send_stdout(&result_diagnostics)?;

        Ok(())
    }
}

impl RspecRunner {
    pub fn parse_diagnostics(
        &self,
        report: RspecReport,
        _file_paths: &[String], 
        workspace_root: &str,
    ) -> Result<RunFileTestResult, LSError> {
        let mut diagnostics_map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
        for example in report.examples {
            if example.status == "failed" {
                let file_path_abs = PathBuf::from(workspace_root).join(&example.file_path.trim_start_matches("./"));
                let file_path_str = file_path_abs.to_str().unwrap_or_default().to_string();

                if file_path_str.is_empty() {
                    eprintln!("Warning: Could not determine absolute path for example: {:?}", example);
                    continue;
                }

                let message = example
                    .exception
                    .as_ref()
                    .map_or_else(|| "Unknown error".to_string(), |ex| ex.message.clone());

                let line_number = if example.line_number > 0 { example.line_number - 1 } else { 0 };

                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: line_number,
                            character: 0, 
                        },
                        end: Position {
                            line: line_number,
                            character: MAX_CHAR_LENGTH, 
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message,
                    ..Default::default()
                };
                diagnostics_map.entry(file_path_str).or_default().push(diagnostic);
            }
        }
        let file_diagnostics = diagnostics_map
            .into_iter()
            .map(|(path, diagnostics)| FileDiagnostics { path, diagnostics })
            .collect();
        Ok(RunFileTestResult {
            data: file_diagnostics,
            messages: vec![], 
        })
    }
}
