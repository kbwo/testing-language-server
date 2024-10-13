use crate::runner::util::send_stdout;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
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
use super::util::LOG_LOCATION;
use super::util::MAX_CHAR_LENGTH;

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
                    severity: Some(DiagnosticSeverity::ERROR),
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

fn detect_workspaces(file_paths: Vec<String>) -> DetectWorkspaceResult {
    detect_workspaces_from_file_list(&file_paths, &["package.json".to_string()])
}

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    // from https://github.com/nvim-neotest/neotest-jest/blob/514fd4eae7da15fd409133086bb8e029b65ac43f/lua/neotest-jest/init.lua#L162
    // license: https://github.com/nvim-neotest/neotest-jest/blob/514fd4eae7da15fd409133086bb8e029b65ac43f/LICENSE.md
    let query = r#"
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
    discover_with_treesitter(file_path, &tree_sitter_javascript::language(), query)
}

#[derive(Eq, PartialEq, Debug)]
pub struct JestRunner;

impl Runner for JestRunner {
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
        let workspace_root = args.workspace;
        let log_path = LOG_LOCATION.join("jest.json");
        std::process::Command::new("jest")
            .current_dir(&workspace_root)
            .args([
                "--testLocationInResults",
                "--forceExit",
                "--no-coverage",
                "--verbose",
                "--json",
                "--outputFile",
                log_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let test_result = fs::read_to_string(log_path)?;
        let diagnostics: RunFileTestResult = parse_diagnostics(&test_result, file_paths)?;
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
    use lsp_types::{Position, Range};

    use super::*;

    #[test]
    fn test_parse_diagnostics() {
        let test_result = std::env::current_dir()
            .unwrap()
            .join("../../demo/jest/output.json");
        let test_result = std::fs::read_to_string(test_result).unwrap();
        let diagnostics = parse_diagnostics(
            &test_result,
            vec![
                "/absolute_path/demo/jest/index.spec.js".to_string(),
                "/absolute_path/demo/jest/another.spec.js".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_detect_workspace() {
        let current_dir = std::env::current_dir().unwrap();
        let absolute_path_of_demo = current_dir.join("../../demo/jest");
        let demo_indexjs = absolute_path_of_demo.join("index.spec.js");
        let file_paths: Vec<String> = [demo_indexjs]
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
        let file_path = "../../demo/jest/index.spec.js";
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 1);
        assert_eq!(
            test_items,
            vec![TestItem {
                id: String::from("index::fail"),
                name: String::from("index::fail"),
                start_position: Range {
                    start: Position {
                        line: 1,
                        character: 2
                    },
                    end: Position {
                        line: 1,
                        character: MAX_CHAR_LENGTH
                    }
                },
                end_position: Range {
                    start: Position {
                        line: 3,
                        character: 0
                    },
                    end: Position {
                        line: 3,
                        character: 4
                    }
                }
            }]
        )
    }
}
