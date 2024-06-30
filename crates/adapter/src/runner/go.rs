use crate::model::Runner;
use anyhow::anyhow;
use lsp_types::Diagnostic;
use lsp_types::Position;
use lsp_types::Range;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Output;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::DiscoverResult;
use testing_language_server::spec::DiscoverResultItem;
use testing_language_server::spec::RunFileTestResult;
use testing_language_server::spec::RunFileTestResultItem;
use testing_language_server::spec::TestItem;
use tree_sitter::Point;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use super::util::detect_workspaces_from_file_paths;

// If the character value is greater than the line length it defaults back to the line length.
const MAX_CHAR_LENGTH: u32 = 10000;

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Action {
    Start,
    Run,
    Output,
    Fail,
    Pass,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TestResultLine {
    time: String,
    action: Action,
    package: String,
    test: Option<String>,
    output: Option<String>,
}

fn get_position_from_output(output: &str) -> Option<(String, u32)> {
    let pattern = r"^\s{4}(.*_test\.go):(\d+):";
    let re = Regex::new(pattern).unwrap();
    if let Some(captures) = re.captures(output) {
        if let (Some(file_name), Some(lnum)) = (captures.get(1), captures.get(2)) {
            return Some((
                file_name.as_str().to_string(),
                lnum.as_str().parse::<u32>().unwrap() - 1,
            ));
        }
    }
    None
}
fn get_log_from_output(output: &str) -> String {
    output.replace("        ", "")
}

fn parse_diagnostics(
    contents: &str,
    workspace_root: PathBuf,
    file_paths: &[String],
) -> Result<RunFileTestResult, LSError> {
    let contents = contents.replace("\r\n", "\n");
    let lines = contents.lines();
    let mut result_map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    let mut file_name: Option<String> = None;
    let mut lnum: Option<u32> = None;
    let mut message = String::new();
    let mut last_action: Option<Action> = None;
    for line in lines {
        let value: TestResultLine = serde_json::from_str(line).map_err(|e| anyhow!("{:?}", e))?;
        match value.action {
            Action::Run => {
                file_name = None;
                message = String::new();
            }
            Action::Output => {
                let output = &value.output.unwrap();
                if let Some((detected_file_name, detected_lnum)) = get_position_from_output(output)
                {
                    file_name = Some(detected_file_name);
                    lnum = Some(detected_lnum);
                    message = String::new();
                } else {
                    message += &get_log_from_output(output);
                }
            }
            _ => {}
        }
        let current_action = value.action;
        let is_action_changed = last_action.as_ref() != Some(&current_action);
        if is_action_changed {
            last_action = Some(current_action);
        } else {
            continue;
        }

        if let (Some(detected_fn), Some(detected_lnum)) = (&file_name, lnum) {
            let diagnostic = Diagnostic {
                range: Range {
                    start: Position {
                        line: detected_lnum,
                        character: 1,
                    },
                    end: Position {
                        line: detected_lnum,
                        character: MAX_CHAR_LENGTH,
                    },
                },
                message: message.clone(),
                ..Diagnostic::default()
            };
            let file_path = workspace_root
                .join(detected_fn)
                .to_str()
                .unwrap()
                .to_owned();
            if file_paths.contains(&file_path) {
                result_map.entry(file_path).or_default().push(diagnostic);
            }
            file_name = None;
            lnum = None;
        }
    }

    Ok(result_map
        .into_iter()
        .map(|(path, diagnostics)| RunFileTestResultItem { path, diagnostics })
        .collect())
}

fn discover(file_path: &str) -> Result<Vec<TestItem>, LSError> {
    let mut parser = tree_sitter::Parser::new();
    let mut test_items: Vec<TestItem> = vec![];
    parser
        .set_language(&tree_sitter_go::language())
        .expect("Error loading Rust grammar");
    let source_code = std::fs::read_to_string(file_path)?;
    let tree = parser.parse(&source_code, None).unwrap();
    let query_string = r#"
    ;;query
    ((function_declaration
      name: (identifier) @test.name)
      (#match? @test.name "^(Test|Example)"))
      @test.definition

    (method_declaration
      name: (field_identifier) @test.name
      (#match? @test.name "^(Test|Example)")) @test.definition

    (call_expression
      function: (selector_expression
        field: (field_identifier) @test.method)
        (#match? @test.method "^Run$")
      arguments: (argument_list . (interpreted_string_literal) @test.name))
      @test.definition
;; query for list table tests
    (block
      (short_var_declaration
        left: (expression_list
          (identifier) @test.cases)
        right: (expression_list
          (composite_literal
            (literal_value
              (literal_element
                (literal_value
                  (keyed_element
                    (literal_element
                      (identifier) @test.field.name)
                    (literal_element
                      (interpreted_string_literal) @test.name)))) @test.definition))))
      (for_statement
        (range_clause
          left: (expression_list
            (identifier) @test.case)
          right: (identifier) @test.cases1
            (#eq? @test.cases @test.cases1))
        body: (block
         (expression_statement
          (call_expression
            function: (selector_expression
              field: (field_identifier) @test.method)
              (#match? @test.method "^Run$")
            arguments: (argument_list
              (selector_expression
                operand: (identifier) @test.case1
                (#eq? @test.case @test.case1)
                field: (field_identifier) @test.field.name1
                (#eq? @test.field.name @test.field.name1))))))))

;; query for map table tests
	(block
      (short_var_declaration
        left: (expression_list
          (identifier) @test.cases)
        right: (expression_list
          (composite_literal
            (literal_value
              (keyed_element
            	(literal_element
                  (interpreted_string_literal)  @test.name)
                (literal_element
                  (literal_value)  @test.definition))))))
	  (for_statement
       (range_clause
          left: (expression_list
            ((identifier) @test.key.name)
            ((identifier) @test.case))
          right: (identifier) @test.cases1
            (#eq? @test.cases @test.cases1))
	      body: (block
           (expression_statement
            (call_expression
              function: (selector_expression
                field: (field_identifier) @test.method)
                (#match? @test.method "^Run$")
                arguments: (argument_list
                ((identifier) @test.key.name1
                (#eq? @test.key.name @test.key.name1))))))))
"#;
    let query =
        Query::new(&tree_sitter_go::language(), query_string).expect("Error creating query");

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

#[derive(Eq, PartialEq, Hash, Debug)]
pub struct GoTestRunner;
impl Runner for GoTestRunner {
    fn disover(
        &self,
        args: testing_language_server::spec::DiscoverArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        let file_paths = args.file_paths;
        let mut discover_results: DiscoverResult = vec![];

        for file_path in file_paths {
            let tests = discover(&file_path)?;
            discover_results.push(DiscoverResultItem {
                tests,
                path: file_path,
            });
        }
        serde_json::to_writer(std::io::stdout(), &discover_results)?;
        Ok(())
    }

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        let file_paths = args.file_paths;
        let default_args = ["-v", "-json", "", "-count=1", "-timeout=60s"];
        let workspace = args.workspace;
        let test_result = std::process::Command::new("go")
            .current_dir(&workspace)
            .arg("test")
            .args(default_args)
            .args(args.extra)
            .output()
            .unwrap();
        let Output { stdout, stderr, .. } = test_result;
        if stdout.is_empty() && !stderr.is_empty() {
            return Err(LSError::Adapter(String::from_utf8(stderr).unwrap()));
        }
        let test_result = String::from_utf8(stdout)?;
        let diagnostics: RunFileTestResult = parse_diagnostics(
            &test_result,
            PathBuf::from_str(&workspace).unwrap(),
            &file_paths,
        )?;
        serde_json::to_writer(std::io::stdout(), &diagnostics)?;
        Ok(())
    }

    fn detect_workspaces(
        &self,
        args: testing_language_server::spec::DetectWorkspaceArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        serde_json::to_writer(
            std::io::stdout(),
            &detect_workspaces_from_file_paths(&args.file_paths, &["go.mod".to_string()]),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::runner::go::discover;
    use std::str::FromStr;
    use std::{fs::read_to_string, path::PathBuf};

    use crate::runner::go::parse_diagnostics;

    #[test]
    fn test_parse_diagnostics() {
        let current_dir = std::env::current_dir().unwrap();
        let test_file_path = current_dir.join("tests/go-test.txt");
        let contents = read_to_string(test_file_path).unwrap();
        let workspace = PathBuf::from_str("/home/demo/test/go/src/test").unwrap();
        let target_file_path = "/home/demo/test/go/src/test/cases_test.go";
        let result =
            parse_diagnostics(&contents, workspace, &[target_file_path.to_string()]).unwrap();
        let result = result.first().unwrap();
        assert_eq!(result.path, target_file_path);
        let diagnostic = result.diagnostics.first().unwrap();
        assert_eq!(diagnostic.range.start.line, 30);
        assert_eq!(diagnostic.range.start.character, 1);
        assert_eq!(diagnostic.range.end.line, 30);
        assert_eq!(diagnostic.message, "\tError Trace:\tcases_test.go:31\n\tError:      \tNot equal: \n\t    \texpected: 7\n\t    \tactual  : -1\n\tTest:       \tTestSubtract/test_two\n--- FAIL: TestSubtract (0.00s)\n    --- FAIL: TestSubtract/test_one (0.00s)\n");
    }

    #[test]
    fn test_discover() {
        let file_path = "../../test_proj/go/cases_test.go";
        let test_items = discover(file_path).unwrap();
        assert!(!test_items.is_empty());
    }
}
