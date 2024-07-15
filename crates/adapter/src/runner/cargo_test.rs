use crate::runner::util::send_stdout;
use lsp_types::DiagnosticSeverity;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Output;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::DetectWorkspaceResult;
use testing_language_server::spec::RunFileTestResult;
use testing_language_server::spec::TestItem;
use tree_sitter::Point;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use lsp_types::{Diagnostic, Position, Range};
use regex::Regex;
use testing_language_server::spec::DiscoverResult;
use testing_language_server::spec::DiscoverResultItem;
use testing_language_server::spec::RunFileTestResultItem;

use crate::model::Runner;

use super::util::detect_workspaces_from_file_paths;

// If the character value is greater than the line length it defaults back to the line length.
const MAX_CHAR_LENGTH: u32 = 10000;

fn parse_diagnostics(
    contents: &str,
    workspace_root: PathBuf,
    file_paths: &[String],
) -> RunFileTestResult {
    let contents = contents.replace("\r\n", "\n");
    let lines = contents.lines();
    let mut result_map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for (i, line) in lines.clone().enumerate() {
        let re = Regex::new(r"thread '([^']+)' panicked at ([^:]+):(\d+):(\d+):").unwrap();
        if let Some(m) = re.captures(line) {
            let mut message = String::new();
            let file = m.get(2).unwrap().as_str().to_string();
            if let Some(file_path) = file_paths
                .iter()
                .find(|path| path.contains(workspace_root.join(&file).to_str().unwrap()))
            {
                let lnum = m.get(3).unwrap().as_str().parse::<u32>().unwrap() - 1;
                let col = m.get(4).unwrap().as_str().parse::<u32>().unwrap() - 1;
                let mut next_i = i + 1;
                while next_i < lines.clone().count()
                    && !lines.clone().nth(next_i).unwrap().is_empty()
                {
                    message = format!("{}{}\n", message, lines.clone().nth(next_i).unwrap());
                    next_i += 1;
                }
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: lnum,
                            character: col,
                        },
                        end: Position {
                            line: lnum,
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
            } else {
                continue;
            }
        }
    }

    result_map
        .into_iter()
        .map(|(path, diagnostics)| RunFileTestResultItem { path, diagnostics })
        .collect()
}

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    let mut parser = tree_sitter::Parser::new();
    let mut test_items: Vec<TestItem> = vec![];
    parser
        .set_language(&tree_sitter_rust::language())
        .expect("Error loading Rust grammar");
    let source_code = std::fs::read_to_string(file_path)?;
    let tree = parser.parse(&source_code, None).unwrap();
    // from https://github.com/rouge8/neotest-rust/blob/0418811e1e3499b2501593f2e131d02f5e6823d4/lua/neotest-rust/init.lua#L167
    // license: https://github.com/rouge8/neotest-rust/blob/0418811e1e3499b2501593f2e131d02f5e6823d4/LICENSE
    let query_string = r#"
        (
  (attribute_item
    [
      (attribute
        (identifier) @macro_name
      )
      (attribute
        [
	  (identifier) @macro_name
	  (scoped_identifier
	    name: (identifier) @macro_name
          )
        ]
      )
    ]
  )
  [
  (attribute_item
    (attribute
      (identifier)
    )
  )
  (line_comment)
  ]*
  .
  (function_item
    name: (identifier) @test.name
  ) @test.definition
  (#any-of? @macro_name "test" "rstest" "case")

)
(mod_item name: (identifier) @namespace.name)? @namespace.definition
"#;
    let query =
        Query::new(&tree_sitter_rust::language(), query_string).expect("Error creating query");

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

fn detect_workspaces(file_paths: &[String]) -> DetectWorkspaceResult {
    detect_workspaces_from_file_paths(file_paths, &["Cargo.toml".to_string()])
}

#[derive(Eq, PartialEq, Hash, Debug)]
pub struct CargoTestRunner;

impl Runner for CargoTestRunner {
    fn disover(&self, args: testing_language_server::spec::DiscoverArgs) -> Result<(), LSError> {
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

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), LSError> {
        let file_paths = args.file_paths;
        let tests = file_paths
            .iter()
            .map(|path| {
                discover(path).map(|test_items| {
                    test_items
                        .into_iter()
                        .map(|item| item.id)
                        .collect::<Vec<String>>()
                })
            })
            .filter_map(Result::ok)
            .flatten()
            .collect::<Vec<_>>();
        let workspace_root = args.workspace;
        let test_result = std::process::Command::new("cargo")
            .current_dir(&workspace_root)
            .arg("test")
            .args(args.extra)
            .arg("--")
            .args(tests)
            .output()
            .unwrap();
        let Output { stdout, stderr, .. } = test_result;
        if stdout.is_empty() && !stderr.is_empty() {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let test_result = String::from_utf8(stdout)?;
        let diagnostics: RunFileTestResult = parse_diagnostics(
            &test_result,
            PathBuf::from_str(&workspace_root).unwrap(),
            &file_paths,
        );
        send_stdout(&diagnostics)?;
        Ok(())
    }

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
        let diagnostics: RunFileTestResult = parse_diagnostics(
            fixture,
            PathBuf::from_str("/home/example/projects").unwrap(),
            &file_paths,
        );
        let message = r#"called `Result::unwrap()` on an `Err` value: unexpected end of input while parsing min or version number
Location:
    rocks-lib/src/rocks/dependency.rs:62:22
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
"#;

        assert_eq!(
            diagnostics,
            vec![RunFileTestResultItem {
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
                    ..Diagnostic::default()
                }]
            }]
        )
    }

    #[test]
    fn test_discover() {
        let file_path = "../../test_proj/rust/src/lib.rs";
        discover(file_path).unwrap();
    }

    #[test]
    fn test_detect_workspaces() {
        let current_dir = std::env::current_dir().unwrap();
        let librs = current_dir.join("src/lib.rs");
        let mainrs = current_dir.join("src/main.rs");
        let absolute_path_of_test_proj = current_dir.join("../../test_proj/rust");
        let test_proj_librs = absolute_path_of_test_proj.join("src/lib.rs");
        let file_paths: Vec<String> = [librs, mainrs, test_proj_librs]
            .iter()
            .map(|file_path| file_path.to_str().unwrap().to_string())
            .collect();

        let workspaces = detect_workspaces(&file_paths);
        assert_eq!(workspaces.len(), 2);
        assert!(workspaces.contains_key(absolute_path_of_test_proj.to_str().unwrap()));
        assert!(workspaces.contains_key(current_dir.to_str().unwrap()));
    }
}
