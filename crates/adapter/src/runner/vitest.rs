use std::{
    collections::HashMap,
    fs::{self},
};

use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde_json::Value;
use testing_language_server::{
    error::LSError,
    spec::{
        DiscoverResult, DiscoverResultItem, RunFileTestResult, RunFileTestResultItem, TestItem,
    },
};

use crate::model::Runner;

use super::util::{
    clean_ansi, detect_workspaces_from_file_list, discover_with_treesitter, send_stdout,
    LOG_LOCATION, MAX_CHAR_LENGTH,
};

#[derive(Eq, PartialEq, Hash, Debug)]
pub struct VitestRunner;

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    // from https://github.com/marilari88/neotest-vitest/blob/353364aa05b94b09409cbef21b79c97c5564e2ce/lua/neotest-vitest/init.lua#L101
    let query = r#"
    ; -- Namespaces --
    ; Matches: `describe('context')`
    ((call_expression
      function: (identifier) @func_name (#eq? @func_name "describe")
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe.only('context')`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#any-of? @func_name "describe")
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe.each(['data'])('context')`
    ((call_expression
      function: (call_expression
        function: (member_expression
          object: (identifier) @func_name (#any-of? @func_name "describe")
        )
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition

    ; -- Tests --
    ; Matches: `test('test') / it('test')`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "it" "test")
      arguments: (arguments (string (string_fragment) @test.name) (arrow_function))
    )) @test.definition
    ; Matches: `test.only('test') / it.only('test')`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#any-of? @func_name "test" "it")
      )
      arguments: (arguments (string (string_fragment) @test.name) (arrow_function))
    )) @test.definition
    ; Matches: `test.each(['data'])('test') / it.each(['data'])('test')`
    ((call_expression
      function: (call_expression
        function: (member_expression
          object: (identifier) @func_name (#any-of? @func_name "it" "test")
        )
      )
      arguments: (arguments (string (string_fragment) @test.name) (arrow_function))
    )) @test.definition
"#;
    discover_with_treesitter(file_path, &tree_sitter_javascript::language(), query)
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
            failure_messages.iter().for_each(|message| {
                let message = clean_ansi(message.as_str().unwrap());
                let diagnostic = Diagnostic {
                    range: lsp_types::Range {
                        start: lsp_types::Position {
                            line: line as u32,
                            // Line and column number is slightly incorrect.
                            // ref:
                            // Bug in json reporter line number? · vitest-dev/vitest · Discussion #5350
                            // https://github.com/vitest-dev/vitest/discussions/5350
                            // Currently, The row numbers are from the parse result, the column numbers are 0 and MAX_CHAR_LENGTH is hard-coded.
                            character: 0,
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

impl Runner for VitestRunner {
    #[tracing::instrument(skip(self))]
    fn discover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = vec![];

        for file_path in file_paths {
            let tests = discover(&file_path)?;
            discover_results.push(DiscoverResultItem {
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
        let workspace_root = args.workspace;
        let log_path = LOG_LOCATION.join("vitest.json");
        let log_path = log_path.to_str().unwrap();
        std::process::Command::new("vitest")
            .current_dir(&workspace_root)
            .args([
                "--watch=false",
                "--reporter=json",
                "--outputFile=",
                log_path,
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
        send_stdout(&detect_workspaces_from_file_list(
            &args.file_paths,
            &[
                "package.json".to_string(),
                "vitest.config.ts".to_string(),
                "vitest.config.js".to_string(),
                "vite.config.ts".to_string(),
                "vite.config.js".to_string(),
                "vitest.config.mts".to_string(),
                "vitest.config.mjs".to_string(),
                "vite.config.mts".to_string(),
                "vite.config.mjs".to_string(),
            ],
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

    #[test]
    fn test_discover() {
        let file_path = "../../demo/vitest/basic.test.ts";
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 2);
        assert_eq!(
            test_items,
            [
                TestItem {
                    id: "describe text::pass".to_string(),
                    name: "describe text::pass".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 4,
                            character: 2
                        },
                        end: Position {
                            line: 4,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 6,
                            character: 0
                        },
                        end: Position {
                            line: 6,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "describe text::fail".to_string(),
                    name: "describe text::fail".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 8,
                            character: 2
                        },
                        end: Position {
                            line: 8,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 10,
                            character: 0
                        },
                        end: Position {
                            line: 10,
                            character: 4
                        }
                    }
                }
            ]
        )
    }
}
