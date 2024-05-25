use lsp_types::Diagnostic;
use regex::Regex;
use serde_json::Value;
use std::path::PathBuf;
use testing_language_server::spec::RunFileTestResult;
use testing_language_server::spec::RunFileTestResultItem;

use crate::Runner;

#[derive(Eq, PartialEq, Debug)]
pub struct GoRunner;

const MAX_CHAR_LENGTH: u32 = 10000;

struct Position {
    pub filename: String,
    pub line: u32,
}

fn re_testfile() -> Regex {
    Regex::new("^\\s\\s\\s\\s(.*_test.go):(\\d+):\\s").unwrap()
}

fn re_testlog() -> Regex {
    Regex::new("^\\s\\s\\s\\s\\s\\s\\s\\s").unwrap()
}

fn format_output(output: &str) -> String {
    let output = re_testlog().replace_all(output, "");
    let output = re_testfile().replace_all(&output, "").to_string();
    output
}

fn get_position_from_output(output: &str) -> Option<Position> {
    re_testfile()
        .captures_iter(output)
        .next()
        .map(|c| Position {
            filename: c[1].to_string(),
            line: c[2].parse().unwrap(),
        })
}

fn parse_diagnostics(
    contents: &str,
    workspace_root: PathBuf,
    file_paths: &[String],
) -> RunFileTestResult {
    let items: Vec<RunFileTestResultItem> = vec![];

    let position_and_name: Option<Position> = None;
    let mut contents = "".to_string();
    let mut diagnostics = vec![];
    for line in contents.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        let action = value.get("Action").unwrap();
        let package = value.get("Package").unwrap();
        let output = value.get("Output");
        if let Some(output) = output {
            let output = output.as_str().unwrap();
            match (position_and_name, get_position_from_output(output)) {
                (None, Some(position)) => {}
                (Some(old_position), Some(position)) => {
                    let diagnostic = Diagnostic {
                        range: lsp_types::Range {
                            start: lsp_types::Position {
                                line: position.line - 1,
                                character: 0,
                            },
                            end: lsp_types::Position {
                                line: position.line - 1,
                                character: MAX_CHAR_LENGTH,
                            },
                        },
                        message: format_output(output),
                        ..Diagnostic::default()
                    };
                    let old_filename = old_position.filename;
                    let new_filename = position.filename;
                    if old_filename != new_filename {
                        let file_path = file_paths
                            .iter()
                            .find(|path| path.ends_with(&old_filename))
                            .unwrap();
                        items.push(RunFileTestResultItem {
                            path: file_path.to_owned(),
                            diagnostics,
                        });
                        // diagnostics = diagnostic;
                    } else {
                        diagnostics.push(diagnostic)
                    }
                    // items.push(RunFileTestResultItem {
                    //     path: todo!(),
                    //     diagnostics: Diagnostic {
                    //         range: todo!(),
                    //         message: contents.to_string(),
                    //         ..Diagnostic::default()
                    //     },
                    // });
                }
                _ => {
                    contents += output;
                }
            }
            todo!()
        } else {
        }
    }
    items
}

fn parse_gotest_output(lines: &[String]) -> Vec<RunFileTestResultItem> {
    for line in lines {
        if line != "" && line.starts_with('{') {
            todo!()
        }
    }
    todo!()
}

impl Runner for GoRunner {
    fn disover(
        &self,
        args: testing_language_server::spec::DiscoverArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        todo!()
    }

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        let file_paths = args.file_paths;
        let workspace_root = args.workspace_root;
        let test_result = std::process::Command::new("go")
            .current_dir(&workspace_root)
            .arg("test")
            .arg("-v")
            .arg("--jsson")
            .output()
            .unwrap();
        let test_result = String::from_utf8(test_result.stdout)?;
        // let diagnostics: RunFileTestResult = marshal_gotest_output(
        //     &test_result,
        //     PathBuf::from_str(&workspace_root).unwrap(),
        //     &file_paths,
        // );
        // serde_json::to_writer(std::io::stdout(), &diagnostics)?;
        todo!()
    }

    fn detect_workspaces_root(
        &self,
        args: testing_language_server::spec::DetectWorkspaceRootArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostics() {}

    #[test]
    fn test_run_file_test() {}

    #[test]
    fn test_detect_workspace_root() {}

    #[test]
    fn test_get_position() {
        let output_line = "    cases_test.go:31: \n";
        let result = get_position_from_output(output_line).unwrap();

        assert_eq!(result.0, "cases_test.go");
        assert_eq!(result.1, 31);
    }

    #[test]
    fn test_get_position_none() {
        let output_line = "        \tError Trace:\tcases_test.go:31\n";
        assert!(get_position_from_output(output_line).is_none())
    }
}
