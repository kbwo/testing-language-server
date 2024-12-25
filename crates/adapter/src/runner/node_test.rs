use std::process::Output;

use regex::Regex;
use testing_language_server::{
    error::LSError,
    spec::{
        DetectWorkspaceResult, DiscoverResult, FileDiagnostics, FoundFileTests, RunFileTestResult,
        TestItem,
    },
};
use xml::{reader::XmlEvent, ParserConfig};

use crate::model::Runner;

use super::util::{
    detect_workspaces_from_file_list, discover_with_treesitter, send_stdout, write_result_log,
    ResultFromXml,
};

#[derive(Eq, PartialEq, Debug)]
pub struct NodeTestRunner;

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

// characters can be like
// \n[Error [ERR_TEST_FAILURE]: assert is not defined] {\n  failureType: 'testCodeFailure',\n  cause: ReferenceError [Error]: assert is not defined\n      at TestContext.<anonymous> (/home/test-user/projects/testing-language-server/demo/node-test/index.test.js:6:3)\n  at Test.runInAsyncScope (node:async_hooks:203:9)\n      at Test.run (node:internal/test_runner/test:631:25)\n      at Test.start (node:internal/test_runner/test:542:17)\n      at startSubtest (node:internal/test_runner/harness:214:17),\n  code: 'ERR_TEST_FAILURE'\n}\n\t\t
fn get_result_from_characters(
    error_text: &str,
    target_file_paths: &[String],
) -> Result<ResultFromXml, anyhow::Error> {
    let re_path_line = Regex::new(r"\(([^:]+):(\d+):(\d+)\)").unwrap();
    for line in error_text.lines() {
        if let Some(caps) = re_path_line.captures(line) {
            let file_path = &caps[1];
            if !target_file_paths.contains(&file_path.to_string()) {
                continue;
            }
            return Ok(ResultFromXml {
                // remove prefix because it's like "\n"
                message: error_text.strip_prefix("\n").unwrap().to_string(),
                path: file_path.to_string(),
                line: caps[2].parse::<u32>().unwrap(),
                col: caps[3].parse::<u32>().unwrap(),
            });
        }
    }

    Err(anyhow::anyhow!("Failed to parse error from {}", error_text))
}

fn get_result_from_xml(
    output: &str,
    target_file_paths: &[String],
) -> Result<Vec<ResultFromXml>, anyhow::Error> {
    use xml::common::Position;

    let mut reader = ParserConfig::default()
        .ignore_root_level_whitespace(false)
        .create_reader(output.as_bytes());

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
                    if let Ok(result_from_xml) =
                        get_result_from_characters(&data, target_file_paths)
                    {
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
    #[tracing::instrument(skip(self))]
    fn discover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = DiscoverResult { data: vec![] };
        for file_path in file_paths {
            discover_results.data.push(FoundFileTests {
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
        let output = std::process::Command::new("node")
            .current_dir(&workspace_root)
            .args(["--test", "--test-reporter", "junit"])
            .args(args.extra)
            .args(&file_paths)
            .output()
            .unwrap();
        write_result_log("node-test.xml", &output)?;
        let Output { stdout, stderr, .. } = output;
        if stdout.is_empty() && !stderr.is_empty() {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let stdout = String::from_utf8(stdout).unwrap();
        let result_from_xml = get_result_from_xml(&stdout, &file_paths)?;
        let result_item: Vec<FileDiagnostics> = result_from_xml
            .into_iter()
            .map(|result_from_xml| {
                let result_item: FileDiagnostics = result_from_xml.into();
                result_item
            })
            .collect();
        let result = RunFileTestResult {
            data: result_item,
            messages: vec![],
        };
        send_stdout(&result)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn detect_workspaces(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let detect_result: DetectWorkspaceResult =
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
        let mut xml_path = std::env::current_dir().unwrap();
        xml_path.push("../../demo/node-test/output.xml");
        let content = std::fs::read_to_string(&xml_path).unwrap();
        let target_file_path =
            "/home/test-user/projects/testing-language-server/demo/node-test/index.test.js";
        let result = get_result_from_xml(&content, &[target_file_path.to_string()]).unwrap();
        assert_eq!(result.len(), 9);

        let paths = result
            .iter()
            .map(|result_from_xml| result_from_xml.path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            assert_eq!(target_file_path, path.as_str());
        }

        let lines = result
            .iter()
            .map(|result_from_xml| result_from_xml.line)
            .collect::<Vec<_>>();
        assert_eq!(lines, [13, 25, 32, 47, 87, 101, 145, 156, 172]);

        let cols = result
            .iter()
            .map(|result_from_xml| result_from_xml.col)
            .collect::<Vec<_>>();
        assert_eq!(cols, [10, 10, 14, 10, 9, 9, 9, 11, 3]);
    }

    #[test]
    fn test_discover() {
        let file_path = "../../demo/node-test/index.test.js";
        let test_items = discover(file_path).unwrap();
        assert_eq!(test_items.len(), 26);
        assert_eq!(
            test_items,
            [
                TestItem {
                    id: "synchronous passing test".to_string(),
                    name: "synchronous passing test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 5,
                            character: 0
                        },
                        end: Position {
                            line: 5,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 8,
                            character: 0
                        },
                        end: Position {
                            line: 8,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "synchronous failing test".to_string(),
                    name: "synchronous failing test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 10,
                            character: 0
                        },
                        end: Position {
                            line: 10,
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
                    id: "asynchronous passing test".to_string(),
                    name: "asynchronous passing test".to_string(),
                    path: file_path.to_string(),
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
                            line: 19,
                            character: 0
                        },
                        end: Position {
                            line: 19,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "asynchronous failing test".to_string(),
                    name: "asynchronous failing test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 21,
                            character: 0
                        },
                        end: Position {
                            line: 21,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 25,
                            character: 0
                        },
                        end: Position {
                            line: 25,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "failing test using Promises".to_string(),
                    name: "failing test using Promises".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 27,
                            character: 0
                        },
                        end: Position {
                            line: 27,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 34,
                            character: 0
                        },
                        end: Position {
                            line: 34,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "callback passing test".to_string(),
                    name: "callback passing test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 36,
                            character: 0
                        },
                        end: Position {
                            line: 36,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 40,
                            character: 0
                        },
                        end: Position {
                            line: 40,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "callback failing test".to_string(),
                    name: "callback failing test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 42,
                            character: 0
                        },
                        end: Position {
                            line: 42,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 48,
                            character: 0
                        },
                        end: Position {
                            line: 48,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "top level test".to_string(),
                    name: "top level test".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 51,
                            character: 0
                        },
                        end: Position {
                            line: 51,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 59,
                            character: 0
                        },
                        end: Position {
                            line: 59,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip option".to_string(),
                    name: "skip option".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 63,
                            character: 0
                        },
                        end: Position {
                            line: 63,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 65,
                            character: 0
                        },
                        end: Position {
                            line: 65,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip option with message".to_string(),
                    name: "skip option with message".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 68,
                            character: 0
                        },
                        end: Position {
                            line: 68,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 70,
                            character: 0
                        },
                        end: Position {
                            line: 70,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip() method".to_string(),
                    name: "skip() method".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 72,
                            character: 0
                        },
                        end: Position {
                            line: 72,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 75,
                            character: 0
                        },
                        end: Position {
                            line: 75,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "skip() method with message".to_string(),
                    name: "skip() method with message".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 77,
                            character: 0
                        },
                        end: Position {
                            line: 77,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 80,
                            character: 0
                        },
                        end: Position {
                            line: 80,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo option".to_string(),
                    name: "todo option".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 84,
                            character: 0
                        },
                        end: Position {
                            line: 84,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 87,
                            character: 0
                        },
                        end: Position {
                            line: 87,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo option with message".to_string(),
                    name: "todo option with message".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 90,
                            character: 0
                        },
                        end: Position {
                            line: 90,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 92,
                            character: 0
                        },
                        end: Position {
                            line: 92,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo() method".to_string(),
                    name: "todo() method".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 94,
                            character: 0
                        },
                        end: Position {
                            line: 94,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 96,
                            character: 0
                        },
                        end: Position {
                            line: 96,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "todo() method with message".to_string(),
                    name: "todo() method with message".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 98,
                            character: 0
                        },
                        end: Position {
                            line: 98,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 101,
                            character: 0
                        },
                        end: Position {
                            line: 101,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "A thing::should work".to_string(),
                    name: "A thing::should work".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 105,
                            character: 2
                        },
                        end: Position {
                            line: 105,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 107,
                            character: 0
                        },
                        end: Position {
                            line: 107,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "A thing::should be ok".to_string(),
                    name: "A thing::should be ok".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 109,
                            character: 2
                        },
                        end: Position {
                            line: 109,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 111,
                            character: 0
                        },
                        end: Position {
                            line: 111,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "A thing::a nested thing::should work".to_string(),
                    name: "A thing::a nested thing::should work".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 114,
                            character: 4
                        },
                        end: Position {
                            line: 114,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 116,
                            character: 0
                        },
                        end: Position {
                            line: 116,
                            character: 6
                        }
                    }
                },
                TestItem {
                    id: "only: this test is run".to_string(),
                    name: "only: this test is run".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 123,
                            character: 0
                        },
                        end: Position {
                            line: 123,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 139,
                            character: 0
                        },
                        end: Position {
                            line: 139,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "only: this test is not run".to_string(),
                    name: "only: this test is not run".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 142,
                            character: 0
                        },
                        end: Position {
                            line: 142,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 145,
                            character: 0
                        },
                        end: Position {
                            line: 145,
                            character: 2
                        }
                    }
                },
                TestItem {
                    id: "A suite::this test is run A ".to_string(),
                    name: "A suite::this test is run A ".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 149,
                            character: 2
                        },
                        end: Position {
                            line: 149,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 151,
                            character: 0
                        },
                        end: Position {
                            line: 151,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "A suite::this test is not run B".to_string(),
                    name: "A suite::this test is not run B".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 153,
                            character: 2
                        },
                        end: Position {
                            line: 153,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 156,
                            character: 0
                        },
                        end: Position {
                            line: 156,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "this test is run C".to_string(),
                    name: "this test is run C".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 161,
                            character: 2
                        },
                        end: Position {
                            line: 161,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 163,
                            character: 0
                        },
                        end: Position {
                            line: 163,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "this test is run D".to_string(),
                    name: "this test is run D".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 165,
                            character: 2
                        },
                        end: Position {
                            line: 165,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 167,
                            character: 0
                        },
                        end: Position {
                            line: 167,
                            character: 4
                        }
                    }
                },
                TestItem {
                    id: "import from external file. this must be fail".to_string(),
                    name: "import from external file. this must be fail".to_string(),
                    path: file_path.to_string(),
                    start_position: Range {
                        start: Position {
                            line: 170,
                            character: 0
                        },
                        end: Position {
                            line: 170,
                            character: 10000
                        }
                    },
                    end_position: Range {
                        start: Position {
                            line: 172,
                            character: 0
                        },
                        end: Position {
                            line: 172,
                            character: 2
                        }
                    }
                }
            ]
        );
    }
}
