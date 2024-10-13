use crate::runner::util::resolve_path;
use crate::runner::util::send_stdout;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use lsp_types::Position;
use lsp_types::Range;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Output;
use std::str::FromStr;
use testing_language_server::error::LSError;

use testing_language_server::spec::DetectWorkspaceResult;
use testing_language_server::spec::DiscoverResult;
use testing_language_server::spec::DiscoverResultItem;
use testing_language_server::spec::RunFileTestResult;
use testing_language_server::spec::RunFileTestResultItem;
use testing_language_server::spec::TestItem;

use crate::model::Runner;

use super::util::clean_ansi;
use super::util::detect_workspaces_from_file_list;
use super::util::discover_with_treesitter;
use super::util::write_result_log;
use super::util::MAX_CHAR_LENGTH;

fn get_position_from_output(line: &str) -> Option<(String, u32, u32)> {
    let re = Regex::new(r"=> (?P<file>.*):(?P<line>\d+):(?P<column>\d+)").unwrap();

    if let Some(captures) = re.captures(line) {
        let file = captures.name("file").unwrap().as_str().to_string();
        let line = captures.name("line").unwrap().as_str().parse().unwrap();
        let column = captures.name("column").unwrap().as_str().parse().unwrap();

        Some((file, line, column))
    } else {
        None
    }
}

fn parse_diagnostics(
    contents: &str,
    workspace_root: PathBuf,
    file_paths: &[String],
) -> Result<RunFileTestResult, LSError> {
    let contents = clean_ansi(&contents.replace("\r\n", "\n"));
    let lines = contents.lines();
    let mut result_map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    let mut file_name: Option<String> = None;
    let mut lnum: Option<u32> = None;
    let mut message = String::new();
    let mut error_exists = false;
    for line in lines {
        if line.contains("ERRORS") {
            error_exists = true;
        } else if !error_exists {
            continue;
        }
        if let Some(position) = get_position_from_output(line) {
            if file_name.is_some() {
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: lnum.unwrap(),
                            character: 1,
                        },
                        end: Position {
                            line: lnum.unwrap(),
                            character: MAX_CHAR_LENGTH,
                        },
                    },
                    message: message.clone(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    ..Diagnostic::default()
                };
                let file_path = resolve_path(&workspace_root, file_name.as_ref().unwrap())
                    .to_str()
                    .unwrap()
                    .to_string();
                if file_paths.contains(&file_path) {
                    result_map.entry(file_path).or_default().push(diagnostic);
                }
            }
            file_name = Some(position.0);
            lnum = Some(position.1);
        } else {
            message += line;
        }
    }
    Ok(result_map
        .into_iter()
        .map(|(path, diagnostics)| RunFileTestResultItem { path, diagnostics })
        .collect())
}

fn detect_workspaces(file_paths: Vec<String>) -> DetectWorkspaceResult {
    detect_workspaces_from_file_list(&file_paths, &["deno.json".to_string()])
}

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    // from https://github.com/MarkEmmons/neotest-deno/blob/7136b9342aeecb675c7c16a0bde327d7fcb00a1c/lua/neotest-deno/init.lua#L93
    // license: https://github.com/MarkEmmons/neotest-deno/blob/main/LICENSE
    let query = r#"
;; Deno.test
(call_expression
	function: (member_expression) @func_name (#match? @func_name "^Deno.test$")
	arguments: [
		(arguments ((string) @test.name . (arrow_function)))
		(arguments . (function_expression name: (identifier) @test.name))
		(arguments . (object(pair
			key: (property_identifier) @key (#match? @key "^name$")
			value: (string) @test.name
		)))
		(arguments ((string) @test.name . (object) . (arrow_function)))
		(arguments (object) . (function_expression name: (identifier) @test.name))
	]
) @test.definition

;; BDD describe - nested
(call_expression
	function: (identifier) @func_name (#match? @func_name "^describe$")
	arguments: [
		(arguments ((string) @namespace.name . (arrow_function)))
		(arguments ((string) @namespace.name . (function_expression)))
	]
) @namespace.definition

;; BDD describe - flat
(variable_declarator
	name: (identifier) @namespace.id
	value: (call_expression
		function: (identifier) @func_name (#match? @func_name "^describe")
		arguments: [
			(arguments ((string) @namespace.name))
			(arguments (object (pair
				key: (property_identifier) @key (#match? @key "^name$")
				value: (string) @namespace.name
			)))
		]
	)
) @namespace.definition

;; BDD it
(call_expression
	function: (identifier) @func_name (#match? @func_name "^it$")
	arguments: [
		(arguments ((string) @test.name . (arrow_function)))
		(arguments ((string) @test.name . (function_expression)))
	]
) @test.definition
        "#;
    discover_with_treesitter(file_path, &tree_sitter_javascript::language(), query)
}

#[derive(Eq, PartialEq, Debug)]
pub struct DenoRunner;

impl Runner for DenoRunner {
    #[tracing::instrument(skip(self))]
    fn discover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = vec![];
        for file_path in file_paths {
            discover_results.push(DiscoverResultItem {
                tests: discover(&file_path)?,
                path: file_path,
            })
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
        let workspace = args.workspace;
        let output = std::process::Command::new("deno")
            .current_dir(&workspace)
            .args(["test", "--no-prompt"])
            .args(&file_paths)
            .output()
            .unwrap();
        write_result_log("deno.log", &output)?;
        let Output { stdout, stderr, .. } = output;
        if stdout.is_empty() {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let test_result = String::from_utf8(stdout)?;
        let diagnostics: RunFileTestResult = parse_diagnostics(
            &test_result,
            PathBuf::from_str(&workspace).unwrap(),
            &file_paths,
        )?;
        send_stdout(&diagnostics)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn detect_workspaces(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let detect_result = detect_workspaces(file_paths);
        send_stdout(&detect_result)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::env::current_dir;

    use super::*;

    #[test]
    fn test_parse_diagnostics() {
        let test_result = std::env::current_dir()
            .unwrap()
            .join("../../demo/deno/output.txt");
        let test_result = std::fs::read_to_string(test_result).unwrap();
        let workspace = PathBuf::from_str("/home/demo/test/dneo/").unwrap();
        let target_file_path = "/home/demo/test/dneo/main_test.ts";
        let diagnostics =
            parse_diagnostics(&test_result, workspace, &[target_file_path.to_string()]).unwrap();
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_detect_workspace() {
        let current_dir = std::env::current_dir().unwrap();
        let absolute_path_of_demo = current_dir.join("../../demo/deno");
        let test_file = absolute_path_of_demo.join("main.test.ts");
        let file_paths: Vec<String> = [test_file]
            .iter()
            .map(|file_path| file_path.to_str().unwrap().to_string())
            .collect();
        let detect_result = detect_workspaces(file_paths);
        assert_eq!(detect_result.len(), 1);
        detect_result.iter().for_each(|(workspace, _)| {
            assert_eq!(workspace, absolute_path_of_demo.to_str().unwrap());
        });
    }

    #[test]
    fn test_discover() {
        let file_path = current_dir().unwrap().join("../../demo/deno/main_test.ts");
        let file_path = file_path.to_str().unwrap();
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 3);
        assert_eq!(
            test_items,
            vec![
                TestItem {
                    id: String::from("addTest"),
                    name: String::from("addTest"),
                    start_position: Range {
                        start: Position {
                            line: 7,
                            character: 0
                        },
                        end: Position {
                            line: 7,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 9,
                            character: 0
                        },
                        end: Position {
                            line: 9,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: String::from("fail1"),
                    name: String::from("fail1"),
                    start_position: Range {
                        start: Position {
                            line: 11,
                            character: 0
                        },
                        end: Position {
                            line: 11,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 13,
                            character: 0
                        },
                        end: Position {
                            line: 13,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: String::from("fail2"),
                    name: String::from("fail2"),
                    start_position: Range {
                        start: Position {
                            line: 15,
                            character: 0
                        },
                        end: Position {
                            line: 15,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 17,
                            character: 0
                        },
                        end: Position {
                            line: 17,
                            character: 2
                        }
                    }
                }
            ]
        )
    }
}
