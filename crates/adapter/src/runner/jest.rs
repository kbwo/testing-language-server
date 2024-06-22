use lsp_types::Diagnostic;
use lsp_types::Position;
use lsp_types::Range;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use tempfile::tempdir;
use testing_language_server::error::LSError;

use testing_language_server::spec::DetectWorkspaceResult;
use testing_language_server::spec::DiscoverResult;
use testing_language_server::spec::DiscoverResultItem;
use testing_language_server::spec::RunFileTestResult;
use testing_language_server::spec::RunFileTestResultItem;
use testing_language_server::spec::TestItem;
use tree_sitter::Point;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::model::Runner;

// If the character value is greater than the line length it defaults back to the line length.
const MAX_CHAR_LENGTH: u32 = 10000;

fn clean_ansi(input: &str) -> String {
    let re = Regex::new(r"\x1B\[([0-9]{1,2}(;[0-9]{1,2})*)?[m|K]").unwrap();
    re.replace_all(input, "").to_string()
}

fn parse_diagnostics(
    test_result: &str,
    file_paths: Vec<String>,
) -> Result<RunFileTestResult, LSError> {
    let mut result_map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    let json: Value = serde_json::from_str(test_result)?;
    let test_results = json["testResults"].as_array().unwrap();
    for test_result in test_results {
        let file_path = test_result["name"].as_str().unwrap();
        if !file_paths.iter().any(|path| path.contains(file_path)) {
            continue;
        }
        let assertion_results = test_result["assertionResults"].as_array().unwrap();
        'assertion: for assertion_result in assertion_results {
            let status = assertion_result["status"].as_str().unwrap();
            if status != "failed" {
                continue 'assertion;
            }
            let location = assertion_result["location"].as_object().unwrap();
            let failure_messages = assertion_result["failureMessages"].as_array().unwrap();
            let line = location["line"].as_u64().unwrap() - 1;
            let column = location["column"].as_u64().unwrap() - 1;
            failure_messages.iter().for_each(|message| {
                let message = clean_ansi(message.as_str().unwrap());
                let diagnostic = Diagnostic {
                    range: lsp_types::Range {
                        start: lsp_types::Position {
                            line: line as u32,
                            character: column as u32,
                        },
                        end: lsp_types::Position {
                            line: line as u32,
                            character: MAX_CHAR_LENGTH,
                        },
                    },
                    message,
                    ..Diagnostic::default()
                };
                result_map
                    .entry(file_path.to_string())
                    .or_default()
                    .push(diagnostic);
            })
        }
    }
    Ok(result_map
        .into_iter()
        .map(|(path, diagnostics)| RunFileTestResultItem { path, diagnostics })
        .collect())
}

fn detect_workspace_from_file(file_path: PathBuf) -> Option<String> {
    let parent = file_path.parent();
    if let Some(parent) = parent {
        if parent.join("package.json").exists() {
            return Some(parent.to_string_lossy().to_string());
        } else {
            detect_workspace_from_file(parent.to_path_buf())
        }
    } else {
        None
    }
}

fn detect_workspaces(file_paths: Vec<String>) -> Result<DetectWorkspaceResult, LSError> {
    let mut result_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut file_paths: Vec<String> = file_paths
        .into_iter()
        .filter(|path| !path.contains("node_modules/"))
        .collect();
    file_paths.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for file_path in file_paths {
        let existing_workspace = result_map
            .iter()
            .find(|(workspace_root, _)| file_path.contains(workspace_root.as_str()));
        if let Some((workspace_root, _)) = existing_workspace {
            result_map
                .entry(workspace_root.to_string())
                .or_default()
                .push(file_path);
        } else {
            let workspace = detect_workspace_from_file(PathBuf::from_str(&file_path).unwrap());
            if let Some(workspace) = workspace {
                result_map.entry(workspace).or_default().push(file_path);
            }
        }
    }
    Ok(result_map)
}

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    let mut parser = tree_sitter::Parser::new();
    let mut test_items: Vec<TestItem> = vec![];
    parser
        .set_language(&tree_sitter_javascript::language())
        .expect("Error loading JavaScript grammar");
    let source_code = std::fs::read_to_string(file_path)?;
    let tree = parser.parse(&source_code, None).unwrap();
    let query_string = r#"
    ; -- Namespaces --
    ; Matches: `describe('context', () => {})`
    ((call_expression
      function: (identifier) @func_name (#eq? @func_name "describe")
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe('context', function() {})`
    ((call_expression
      function: (identifier) @func_name (#eq? @func_name "describe")
      arguments: (arguments (string (string_fragment) @namespace.name) (function_expression))
    )) @namespace.definition
    ; Matches: `describe.only('context', () => {})`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#any-of? @func_name "describe")
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe.only('context', function() {})`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#any-of? @func_name "describe")
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (function_expression))
    )) @namespace.definition
    ; Matches: `describe.each(['data'])('context', () => {})`
    ((call_expression
      function: (call_expression
        function: (member_expression
          object: (identifier) @func_name (#any-of? @func_name "describe")
        )
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe.each(['data'])('context', function() {})`
    ((call_expression
      function: (call_expression
        function: (member_expression
          object: (identifier) @func_name (#any-of? @func_name "describe")
        )
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (function_expression))
    )) @namespace.definition

    ; -- Tests --
    ; Matches: `test('test') / it('test')`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "it" "test")
      arguments: (arguments (string (string_fragment) @test.name) [(arrow_function) (function_expression)])
    )) @test.definition
    ; Matches: `test.only('test') / it.only('test')`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#any-of? @func_name "test" "it")
      )
      arguments: (arguments (string (string_fragment) @test.name) [(arrow_function) (function_expression)])
    )) @test.definition
    ; Matches: `test.each(['data'])('test') / it.each(['data'])('test')`
    ((call_expression
      function: (call_expression
        function: (member_expression
          object: (identifier) @func_name (#any-of? @func_name "it" "test")
          property: (property_identifier) @each_property (#eq? @each_property "each")
        )
      )
      arguments: (arguments (string (string_fragment) @test.name) [(arrow_function) (function_expression)])
    )) @test.definition
        "#;
    let query = Query::new(&tree_sitter_javascript::language(), query_string)
        .expect("Error creating query");
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(tree.root_node().byte_range());
    let source = source_code.as_bytes();
    let matches = cursor.matches(&query, tree.root_node(), source);
    for m in matches {
        let mut namespace_name = "";
        let mut test_start_position = Point::default();
        let mut test_end_position = Point::default();
        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let value = capture.node.utf8_text(source)?;
            let start_position = capture.node.start_position();
            let end_position = capture.node.end_position();
            match capture_name {
                "namespace.name" => {
                    namespace_name = value;
                }
                "test.definition" => {
                    test_start_position = start_position;
                    test_end_position = end_position;
                }
                "test.name" => {
                    let test_name = value;
                    let test_item = TestItem {
                        id: format!("{}:{}", namespace_name, test_name),
                        name: test_name.to_string(),
                        start_position: Range {
                            start: Position {
                                line: test_start_position.row as u32,
                                character: test_start_position.column as u32,
                            },
                            end: Position {
                                line: test_start_position.row as u32,
                                character: MAX_CHAR_LENGTH,
                            },
                        },
                        end_position: Range {
                            start: Position {
                                line: test_end_position.row as u32,
                                character: 0,
                            },
                            end: Position {
                                line: test_end_position.row as u32,
                                character: test_end_position.column as u32,
                            },
                        },
                    };
                    test_items.push(test_item);
                    test_start_position = Point::default();
                    test_end_position = Point::default();
                }
                _ => {}
            }
        }
    }

    Ok(test_items)
}

#[derive(Eq, PartialEq, Debug)]
pub struct JestRunner;

impl Runner for JestRunner {
    fn disover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = vec![];
        for file_path in file_paths {
            discover_results.push(DiscoverResultItem {
                tests: discover(&file_path)?,
                path: file_path,
            })
        }
        serde_json::to_writer(std::io::stdout(), &discover_results)?;
        Ok(())
    }

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let workspace_root = args.workspace_root;
        let tempdir = tempdir().unwrap();
        let tempdir_path = tempdir.path();
        let tempfile_path = tempdir_path.join("jest.json");
        std::process::Command::new("jest")
            .current_dir(&workspace_root)
            .args([
                "--testLocationInResults",
                "--forceExit",
                "--no-coverage",
                "--verbose",
                "--json",
                "--outputFile",
                tempfile_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let test_result = fs::read_to_string(tempfile_path)?;
        let diagnostics: RunFileTestResult = parse_diagnostics(&test_result, file_paths)?;
        serde_json::to_writer(std::io::stdout(), &diagnostics)?;
        Ok(())
    }

    fn detect_workspaces_root(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let detect_result = detect_workspaces(file_paths)?;
        serde_json::to_writer(std::io::stdout(), &detect_result)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

    #[test]
    fn test_parse_diagnostics() {
        let test_result = std::env::current_dir()
            .unwrap()
            .join("../../test_proj/jest/output.json");
        let test_result = std::fs::read_to_string(test_result).unwrap();
        let diagnostics = parse_diagnostics(
            &test_result,
            vec![
                "/absolute_path/test_proj/jest/index.spec.js".to_string(),
                "/absolute_path/test_proj/jest/another.spec.js".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_detect_workspace() {
        let current_dir = std::env::current_dir().unwrap();
        let absolute_path_of_test_proj = current_dir.join("../../test_proj/jest");
        let test_proj_indexjs = absolute_path_of_test_proj.join("index.spec.js");
        let file_paths: Vec<String> = [test_proj_indexjs]
            .iter()
            .map(|file_path| file_path.to_str().unwrap().to_string())
            .collect();
        let detect_result = detect_workspaces(file_paths).unwrap();
        assert_eq!(detect_result.len(), 1);
        detect_result.iter().for_each(|(workspace, _)| {
            assert_eq!(workspace, absolute_path_of_test_proj.to_str().unwrap());
        });
    }

    #[test]
    fn test_discover() {
        let file_path = "../../test_proj/jest/index.spec.js";
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 1);
        assert_eq!(
            test_items,
            vec![TestItem {
                id: String::from(":fail"),
                name: String::from("fail"),
                start_position: Range {
                    start: Position {
                        line: 2,
                        character: 2
                    },
                    end: Position {
                        line: 2,
                        character: MAX_CHAR_LENGTH
                    }
                },
                end_position: Range {
                    start: Position {
                        line: 4,
                        character: 0
                    },
                    end: Position {
                        line: 4,
                        character: 4
                    }
                }
            }]
        )
    }
}
