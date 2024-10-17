use std::{fs::File, io::BufReader, process::Output};

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use regex::Regex;
use testing_language_server::{
    error::LSError,
    spec::{
        DiscoverResult, DiscoverResultItem, RunFileTestResult, RunFileTestResultItem, TestItem,
    },
};
use xml::{reader::XmlEvent, ParserConfig};

use crate::model::Runner;

use super::util::{
    detect_workspaces_from_file_list, discover_with_treesitter, send_stdout, write_result_log,
    MAX_CHAR_LENGTH,
};

#[derive(Eq, PartialEq, Debug)]
pub struct NodeTestRunner;

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    // from https://github.com/nvim-neotest/neotest-jest/blob/514fd4eae7da15fd409133086bb8e029b65ac43f/lua/neotest-jest/init.lua#L162
    // license: https://github.com/nvim-neotest/neotest-jest/blob/514fd4eae7da15fd409133086bb8e029b65ac43f/LICENSE.md
    let query = r#"
    ; -- Namespaces --
    ; Matches: `describe("A thing", () => {})`
    ((call_expression
      function: (identifier) @func_name (#eq? @func_name "describe")
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe("A thing", function() {})`
    ((call_expression
      function: (identifier) @func_name (#eq? @func_name "describe")
      arguments: (arguments (string (string_fragment) @namespace.name) (function_expression))
    )) @namespace.definition
    ; Matches: `describe.only("A thing", () => {})`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#eq? @func_name "describe")
        property: (property_identifier) @only_property (#eq? @only_property "only")
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (arrow_function))
    )) @namespace.definition
    ; Matches: `describe.only("A thing", function() {})`
    ((call_expression
      function: (member_expression
        object: (identifier) @func_name (#eq? @func_name "describe")
        property: (property_identifier) @only_property (#eq? @only_property "only")
      )
      arguments: (arguments (string (string_fragment) @namespace.name) (function_expression))
    )) @namespace.definition

    ; -- Tests --
    ; Matches: `test("test name", (t) => {})` or `it("test name", (t) => {})`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "test" "it")
      arguments: (arguments (string (string_fragment) @test.name) [(arrow_function) (function_expression)])
    )) @test.definition
    ; Matches: `test("test name", { skip: true }, (t) => {})`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "test" "it")
      arguments: (arguments
        (string (string_fragment) @test.name)
        (object)
        [(arrow_function) (function_expression)]
      )
    )) @test.definition
    ; Matches: `test("test name", async (t) => {})`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "test" "it")
      arguments: (arguments
        (string (string_fragment) @test.name)
        (arrow_function (identifier) @async (#eq? @async "async"))
      )
    )) @test.definition
    ; Matches: `test("test name", (t, done) => {})`
    ((call_expression
      function: (identifier) @func_name (#any-of? @func_name "test" "it")
      arguments: (arguments
        (string (string_fragment) @test.name)
        [(arrow_function (formal_parameters (identifier) (identifier))) (function_expression)]
      )
    )) @test.definition

    "#;
    discover_with_treesitter(file_path, &tree_sitter_javascript::language(), query)
}

#[derive(Debug)]
struct ResultFromXml {
    pub message: String,
    pub path: String,
    pub line: u32,
    pub col: u32,
}

impl Into<RunFileTestResultItem> for ResultFromXml {
    fn into(self) -> RunFileTestResultItem {
        RunFileTestResultItem {
            path: self.path,
            diagnostics: vec![Diagnostic {
                message: self.message,
                range: Range {
                    start: Position {
                        line: self.line - 1,
                        character: 0,
                    },
                    end: Position {
                        line: self.line - 1,
                        character: MAX_CHAR_LENGTH,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                ..Default::default()
            }],
        }
    }
}
// characters can be like
// \n[Error [ERR_TEST_FAILURE]: assert is not defined] {\n  failureType: 'testCodeFailure',\n  cause: ReferenceError [Error]: assert is not defined\n      at TestContext.<anonymous> (/home/kbwo/go/projects/github.com/kbwo/testing-language-server/demo/node-test/index.test.js:6:3)\n  at Test.runInAsyncScope (node:async_hooks:203:9)\n      at Test.run (node:internal/test_runner/test:631:25)\n      at Test.start (node:internal/test_runner/test:542:17)\n      at startSubtest (node:internal/test_runner/harness:214:17),\n  code: 'ERR_TEST_FAILURE'\n}\n\t\t
fn get_result_from_characters(error_text: &str) -> Result<ResultFromXml, anyhow::Error> {
    let re_path_line = Regex::new(r"\(([^:]+):(\d+):(\d+)\)").unwrap();

    // Extract and print the file path and line number
    if let Some(caps) = re_path_line.captures(error_text) {
        let path = &caps[1];
        let line = *&caps[2].parse::<u32>().unwrap();
        let col = *&caps[3].parse::<u32>().unwrap();

        return Ok(ResultFromXml {
            message: error_text.to_string(),
            path: path.to_string(),
            line,
            col,
        });
    }

    Err(anyhow::anyhow!("Failed to parse error from {}", error_text))
}

fn get_result_from_xml(path: &str) -> Result<Vec<ResultFromXml>, anyhow::Error> {
    use xml::common::Position;

    let file = File::open(path).unwrap();
    let mut reader = ParserConfig::default()
        .ignore_root_level_whitespace(false)
        .create_reader(BufReader::new(file));

    let local_name = "failure";

    let mut in_failure = false;
    let mut result: Vec<ResultFromXml> = Vec::new();
    loop {
        match reader.next() {
            Ok(e) => match e {
                XmlEvent::StartElement { name, .. } => {
                    if name.local_name.starts_with(local_name) {
                        in_failure = true;
                    }
                }
                XmlEvent::EndElement { .. } => {
                    in_failure = false;
                }
                XmlEvent::Characters(data) => {
                    if let Ok(result_from_xml) = get_result_from_characters(&data) {
                        if in_failure {
                            result.push(result_from_xml);
                        }
                    }
                }
                XmlEvent::EndDocument => break,
                _ => {}
            },
            Err(e) => {
                tracing::error!("Error at {}: {e}", reader.position());
                break;
            }
        }
    }

    Ok(result)
}

impl Runner for NodeTestRunner {
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

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let workspace_root = args.workspace;
        let output = std::process::Command::new("node")
            .current_dir(&workspace_root)
            .args(["--test", "--test-reporter", "junit"])
            .args(args.extra)
            .args(file_paths)
            .output()
            .unwrap();
        write_result_log("node-test.xml", &output)?;
        let Output { stdout, stderr, .. } = output;
        if stdout.is_empty() && !stderr.is_empty() {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let stdout = String::from_utf8(stdout).unwrap();
        let result_from_xml = get_result_from_xml(&stdout)?;
        let diagnostics: RunFileTestResult = result_from_xml
            .into_iter()
            .map(|result_from_xml| {
                let result_item: RunFileTestResultItem = result_from_xml.into();
                result_item
            })
            .collect();
        send_stdout(&diagnostics)?;
        Ok(())
    }

    fn detect_workspaces(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let detect_result =
            detect_workspaces_from_file_list(&file_paths, &["package.json".to_string()]);
        send_stdout(&detect_result)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

    #[test]
    fn parse_xml() {
        let mut path = std::env::current_dir().unwrap();
        path.push("../../demo/node-test/output.xml");
        let result = get_result_from_xml(path.to_str().unwrap()).unwrap();
        assert_eq!(result.len(), 15);

        let paths = result
            .iter()
            .map(|result_from_xml| result_from_xml.path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            assert_eq!(
                path,
                "/home/test-user/projects/testing-language-server/demo/node-test/index.test.js"
            );
        }

        let lines = result
            .iter()
            .map(|result_from_xml| result_from_xml.line)
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            [6, 11, 17, 23, 30, 45, 51, 55, 85, 99, 105, 109, 114, 143, 154]
        );

        let cols = result
            .iter()
            .map(|result_from_xml| result_from_xml.col)
            .collect::<Vec<_>>();
        assert_eq!(cols, [3, 3, 3, 3, 14, 10, 3, 3, 9, 9, 5, 5, 7, 9, 11]);
    }

    #[test]
    fn test_discover() {
        let file_path = "../../demo/node-test/index.test.js";
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 23);
        assert_eq!(
            test_items,
            vec![
                TestItem {
                    id: "synchronous passing test".to_string(),
                    name: "synchronous passing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 3,
                            character: 0
                        },
                        end: Position {
                            line: 3,
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
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "synchronous failing test".to_string(),
                    name: "synchronous failing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 8,
                            character: 0
                        },
                        end: Position {
                            line: 8,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 11,
                            character: 0
                        },
                        end: Position {
                            line: 11,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "asynchronous passing test".to_string(),
                    name: "asynchronous passing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 13,
                            character: 0
                        },
                        end: Position {
                            line: 13,
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
                },
                TestItem {
                    id: "asynchronous failing test".to_string(),
                    name: "asynchronous failing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 19,
                            character: 0
                        },
                        end: Position {
                            line: 19,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 23,
                            character: 0
                        },
                        end: Position {
                            line: 23,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "failing test using Promises".to_string(),
                    name: "failing test using Promises".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 25,
                            character: 0
                        },
                        end: Position {
                            line: 25,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 32,
                            character: 0
                        },
                        end: Position {
                            line: 32,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "callback passing test".to_string(),
                    name: "callback passing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 34,
                            character: 0
                        },
                        end: Position {
                            line: 34,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 38,
                            character: 0
                        },
                        end: Position {
                            line: 38,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "callback failing test".to_string(),
                    name: "callback failing test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 40,
                            character: 0
                        },
                        end: Position {
                            line: 40,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 46,
                            character: 0
                        },
                        end: Position {
                            line: 46,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "top level test".to_string(),
                    name: "top level test".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 49,
                            character: 0
                        },
                        end: Position {
                            line: 49,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 57,
                            character: 0
                        },
                        end: Position {
                            line: 57,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip option".to_string(),
                    name: "skip option".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 61,
                            character: 0
                        },
                        end: Position {
                            line: 61,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 63,
                            character: 0
                        },
                        end: Position {
                            line: 63,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip option with message".to_string(),
                    name: "skip option with message".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 66,
                            character: 0
                        },
                        end: Position {
                            line: 66,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 68,
                            character: 0
                        },
                        end: Position {
                            line: 68,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip() method".to_string(),
                    name: "skip() method".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 70,
                            character: 0
                        },
                        end: Position {
                            line: 70,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 73,
                            character: 0
                        },
                        end: Position {
                            line: 73,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip() method with message".to_string(),
                    name: "skip() method with message".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 75,
                            character: 0
                        },
                        end: Position {
                            line: 75,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 78,
                            character: 0
                        },
                        end: Position {
                            line: 78,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo option".to_string(),
                    name: "todo option".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 82,
                            character: 0
                        },
                        end: Position {
                            line: 82,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 85,
                            character: 0
                        },
                        end: Position {
                            line: 85,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo option with message".to_string(),
                    name: "todo option with message".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 88,
                            character: 0
                        },
                        end: Position {
                            line: 88,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 90,
                            character: 0
                        },
                        end: Position {
                            line: 90,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo() method".to_string(),
                    name: "todo() method".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 92,
                            character: 0
                        },
                        end: Position {
                            line: 92,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 94,
                            character: 0
                        },
                        end: Position {
                            line: 94,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo() method with message".to_string(),
                    name: "todo() method with message".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 96,
                            character: 0
                        },
                        end: Position {
                            line: 96,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 99,
                            character: 0
                        },
                        end: Position {
                            line: 99,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "A thing::should work".to_string(),
                    name: "A thing::should work".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 103,
                            character: 2
                        },
                        end: Position {
                            line: 103,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 105,
                            character: 0
                        },
                        end: Position {
                            line: 105,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "A thing::should be ok".to_string(),
                    name: "A thing::should be ok".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 107,
                            character: 2
                        },
                        end: Position {
                            line: 107,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 109,
                            character: 0
                        },
                        end: Position {
                            line: 109,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "a nested thing::should work".to_string(),
                    name: "a nested thing::should work".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 112,
                            character: 4
                        },
                        end: Position {
                            line: 112,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 114,
                            character: 0
                        },
                        end: Position {
                            line: 114,
                            character: 6
                        }
                    }
                },
                TestItem {
                    id: "a nested thing::this test is run".to_string(),
                    name: "a nested thing::this test is run".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 121,
                            character: 0
                        },
                        end: Position {
                            line: 121,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 137,
                            character: 0
                        },
                        end: Position {
                            line: 137,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "a nested thing::this test is not run".to_string(),
                    name: "a nested thing::this test is not run".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 140,
                            character: 0
                        },
                        end: Position {
                            line: 140,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 143,
                            character: 0
                        },
                        end: Position {
                            line: 143,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "a suite::this test is run".to_string(),
                    name: "a suite::this test is run".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 147,
                            character: 2
                        },
                        end: Position {
                            line: 147,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 149,
                            character: 0
                        },
                        end: Position {
                            line: 149,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "a suite::this test is not run".to_string(),
                    name: "a suite::this test is not run".to_string(),
                    start_position: Range {
                        start: Position {
                            line: 151,
                            character: 2
                        },
                        end: Position {
                            line: 151,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 154,
                            character: 0
                        },
                        end: Position {
                            line: 154,
                            character: 4
                        }
                    }
                }
            ]
        );
    }
}
