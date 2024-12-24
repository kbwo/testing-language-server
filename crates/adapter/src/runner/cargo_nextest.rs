use crate::runner::util::send_stdout;
use std::path::PathBuf;
use std::process::Output;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::DetectWorkspaceResult;
use testing_language_server::spec::RunFileTestResult;

use testing_language_server::spec::DiscoverResult;
use testing_language_server::spec::FoundFileTests;
use testing_language_server::spec::TestItem;

use crate::model::Runner;

use super::util::detect_workspaces_from_file_list;
use super::util::discover_rust_tests;
use super::util::parse_cargo_diagnostics;
use super::util::write_result_log;

fn detect_workspaces(file_paths: &[String]) -> DetectWorkspaceResult {
    detect_workspaces_from_file_list(file_paths, &["Cargo.toml".to_string()])
}

#[derive(Eq, PartialEq, Hash, Debug)]
pub struct CargoNextestRunner;

impl Runner for CargoNextestRunner {
    #[tracing::instrument(skip(self))]
    fn discover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = DiscoverResult { data: vec![] };

        for file_path in file_paths {
            let tests = discover_rust_tests(&file_path)?;
            discover_results.data.push(FoundFileTests {
                tests,
                path: file_path,
            });
        }
        send_stdout(&discover_results)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let discovered_tests: Vec<TestItem> = file_paths
            .iter()
            .map(|path| discover_rust_tests(path))
            .filter_map(Result::ok)
            .flatten()
            .collect::<Vec<_>>();
        let test_ids = discovered_tests
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<String>>();
        let workspace_root = args.workspace;
        let test_result = std::process::Command::new("cargo")
            .current_dir(&workspace_root)
            .arg("nextest")
            .arg("run")
            .arg("--workspace")
            .arg("--no-fail-fast")
            .args(args.extra)
            .arg("--")
            .args(&test_ids)
            .output()
            .unwrap();
        let output = test_result;
        write_result_log("cargo_nextest.log", &output)?;
        let Output {
            stdout,
            stderr,
            status,
        } = output;
        let unexpected_status_code = status.code().map(|code| code != 100);
        if stdout.is_empty() && !stderr.is_empty() && unexpected_status_code.unwrap_or(false) {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let test_result = String::from_utf8(stderr)?;
        let diagnostics: RunFileTestResult = parse_cargo_diagnostics(
            &test_result,
            PathBuf::from_str(&workspace_root).unwrap(),
            &file_paths,
            &discovered_tests,
        );
        send_stdout(&diagnostics)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn detect_workspaces(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let detect_result = detect_workspaces(&file_paths);
        send_stdout(&detect_result)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
    use testing_language_server::spec::{FileDiagnostics, TestItem};

    use crate::runner::util::MAX_CHAR_LENGTH;

    use super::*;

    #[test]
    fn parse_test_results() {
        let fixture = r#"
running 1 test
test rocks::dependency::tests::parse_dependency ... FAILED
failures:
    Finished test [unoptimized + debuginfo] target(s) in 0.12s
    Starting 1 test across 2 binaries (17 skipped)
        FAIL [   0.004s] rocks-lib rocks::dependency::tests::parse_dependency
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 17 filtered out; finis
hed in 0.00s
--- STDERR:              rocks-lib rocks::dependency::tests::parse_dependency ---
thread 'rocks::dependency::tests::parse_dependency' panicked at rocks-lib/src/rocks/dependency.rs:86:64:
called `Result::unwrap()` on an `Err` value: unexpected end of input while parsing min or version number
Location:
    rocks-lib/src/rocks/dependency.rs:62:22
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

            "#;
        let file_paths =
            vec!["/home/example/projects/rocks-lib/src/rocks/dependency.rs".to_string()];
        let test_items: Vec<TestItem> = vec![TestItem {
            id: "rocks::dependency::tests::parse_dependency".to_string(),
            name: "rocks::dependency::tests::parse_dependency".to_string(),
            start_position: Range {
                start: Position {
                    line: 85,
                    character: 63,
                },
                end: Position {
                    line: 85,
                    character: MAX_CHAR_LENGTH,
                },
            },
            end_position: Range {
                start: Position {
                    line: 85,
                    character: 63,
                },
                end: Position {
                    line: 85,
                    character: MAX_CHAR_LENGTH,
                },
            },
        }];
        let diagnostics: RunFileTestResult = parse_cargo_diagnostics(
            fixture,
            PathBuf::from_str("/home/example/projects").unwrap(),
            &file_paths,
            &test_items,
        );
        let message = r#"called `Result::unwrap()` on an `Err` value: unexpected end of input while parsing min or version number
Location:
    rocks-lib/src/rocks/dependency.rs:62:22
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
"#;

        assert_eq!(
            diagnostics,
            RunFileTestResult {
                data: vec![FileDiagnostics {
                    path: file_paths.first().unwrap().to_owned(),
                    diagnostics: vec![Diagnostic {
                        range: Range {
                            start: Position {
                                line: 85,
                                character: 63
                            },
                            end: Position {
                                line: 85,
                                character: MAX_CHAR_LENGTH
                            }
                        },
                        message: message.to_string(),
                        severity: Some(DiagnosticSeverity::ERROR),
                        ..Diagnostic::default()
                    }]
                }],
                messages: vec!()
            }
        )
    }

    #[test]
    fn test_discover() {
        let file_path = "../../demo/rust/src/lib.rs";
        discover_rust_tests(file_path).unwrap();
    }

    #[test]
    fn test_detect_workspaces() {
        let current_dir = std::env::current_dir().unwrap();
        let librs = current_dir.join("src/lib.rs");
        let mainrs = current_dir.join("src/main.rs");
        let absolute_path_of_demo = current_dir.join("../../demo/rust");
        let demo_librs = absolute_path_of_demo.join("src/lib.rs");
        let file_paths: Vec<String> = [librs, mainrs, demo_librs]
            .iter()
            .map(|file_path| file_path.to_str().unwrap().to_string())
            .collect();

        let workspaces = detect_workspaces(&file_paths);
        assert_eq!(workspaces.data.len(), 2);
        assert!(workspaces
            .data
            .contains_key(absolute_path_of_demo.to_str().unwrap()));
        assert!(workspaces.data.contains_key(current_dir.to_str().unwrap()));
    }
}
