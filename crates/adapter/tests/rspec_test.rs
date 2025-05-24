// crates/adapter/tests/rspec_test.rs

// use std::fs; // Removed
use std::path::PathBuf; // Path removed, PathBuf kept
// use std::str::FromStr; // Removed

use lsp_types::{Position, Range, DiagnosticSeverity};
use testing_language_server::spec::{
    // DetectWorkspaceArgs, DetectWorkspaceResult, DiscoverArgs, DiscoverResult, 
    FileDiagnostics, RunFileTestResult, TestItem, // Kept these as they are used by code under test or for assertions
};
// use testing_language_server::error::LSError; // Removed
use testing_ls_adapter::runner::rspec::{RspecRunner, RspecReport};
use testing_ls_adapter::runner::util::{
    detect_workspaces_from_file_list, discover_with_treesitter, MAX_CHAR_LENGTH,
};

// Helper to get the absolute path to the demo project
fn get_demo_project_root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); 
    path.push("demo/rspec_project");
    path
}

// Helper to get the absolute path to a file within the demo project
fn get_demo_file_path(relative_file_name: &str) -> PathBuf {
    get_demo_project_root().join(relative_file_name)
}

const RSPEC_QUERY: &str = r#"
(block
  (call
    method: (identifier) @func_name (#any-of? @func_name "describe" "context")
    arguments: (arguments (string (string_content) @namespace.name))
  )
) @namespace.definition

(block
  (call
    method: (identifier) @func_name (#any-of? @func_name "it" "specify" "example")
    arguments: (arguments (string (string_content) @test.name))
  )
) @test.definition
"#; // This IS used.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rspec_workspace() {
        let demo_spec_file = get_demo_file_path("spec/example_spec.rb");
        let file_paths = vec![demo_spec_file.to_str().unwrap().to_string()];
        let indicator_files = vec!["Gemfile".to_string(), ".rspec".to_string()];

        let result = detect_workspaces_from_file_list(&file_paths, &indicator_files);
        
        let expected_workspace_path_str = get_demo_project_root().to_str().unwrap().to_string();
        
        assert_eq!(result.data.len(), 1, "Should detect one workspace. Detected: {:#?}", result.data);
        assert!(result.data.contains_key(&expected_workspace_path_str), "Detected workspace path is incorrect. Expected: {}, Got keys: {:?}", expected_workspace_path_str, result.data.keys());
        
        let files_in_workspace = result.data.get(&expected_workspace_path_str).unwrap();
        assert_eq!(files_in_workspace.len(), 1, "Workspace should contain one file");
        assert_eq!(
            files_in_workspace[0],
            demo_spec_file.to_str().unwrap().to_string(),
            "File in workspace is incorrect"
        );
    }

    #[test]
    fn test_discover_rspec_tests() {
        let spec_file_path = get_demo_file_path("spec/example_spec.rb");
        let spec_file_path_str = spec_file_path.to_str().unwrap();

        let discovered_items = discover_with_treesitter(
            spec_file_path_str,
            &tree_sitter_ruby::language(),
            RSPEC_QUERY,
        ).unwrap_or_else(|e| panic!("discover_with_treesitter failed: {:?}", e));

        let expected_items = vec![
            TestItem { 
                id: "MyString::concatenation::should concatenate two strings".to_string(),
                name: "MyString::concatenation::should concatenate two strings".to_string(),
                path: spec_file_path_str.to_string(),
                start_position: Range { start: Position { line: 2, character: 4 }, end: Position { line: 2, character: MAX_CHAR_LENGTH } },
                end_position: Range { start: Position { line: 7, character: 0 }, end: Position { line: 7, character: 5 } },
            },
            TestItem { 
                id: "Math::should add two numbers correctly".to_string(),
                name: "Math::should add two numbers correctly".to_string(),
                path: spec_file_path_str.to_string(),
                start_position: Range { start: Position { line: 10, character: 2 }, end: Position { line: 10, character: MAX_CHAR_LENGTH } },
                end_position: Range { start: Position { line: 12, character: 0 }, end: Position { line: 12, character: 5 } },
            },
            TestItem { 
                id: "Math::should fail a test".to_string(),
                name: "Math::should fail a test".to_string(),
                path: spec_file_path_str.to_string(),
                start_position: Range { start: Position { line: 14, character: 2 }, end: Position { line: 14, character: MAX_CHAR_LENGTH } },
                end_position: Range { start: Position { line: 16, character: 0 }, end: Position { line: 16, character: 5 } },
            },
            TestItem { 
                id: "Math::is a pending test".to_string(),
                name: "Math::is a pending test".to_string(),
                path: spec_file_path_str.to_string(),
                start_position: Range { start: Position { line: 18, character: 2 }, end: Position { line: 18, character: MAX_CHAR_LENGTH } },
                end_position: Range { start: Position { line: 18, character: 0 }, end: Position { line: 18, character: "it \"is a pending test\"".len() as u32 } },
            },
        ];
        
        assert_eq!(discovered_items.len(), expected_items.len(), "Number of discovered tests does not match. Actual: {:#?}", discovered_items);

        for expected in expected_items.iter() {
            assert!(
                discovered_items.contains(expected),
                "Expected test item not found: {:?}\nActual items: {:#?}",
                expected,
                discovered_items
            );
        }
    }

    #[test]
    fn test_run_rspec_tests_and_parse_diagnostics() {
        let rspec_json_output = r#"
{
  "version": "3.13.0",
  "summary_line": "4 examples, 1 failure, 1 pending",
  "examples": [
    {
      "id": "./spec/example_spec.rb[1:1:1]",
      "description": "should concatenate two strings",
      "full_description": "MyString concatenation should concatenate two strings",
      "status": "passed",
      "file_path": "./spec/example_spec.rb",
      "line_number": 3,
      "run_time": 0.0001
    },
    {
      "id": "./spec/example_spec.rb[2:1:1]",
      "description": "should add two numbers correctly",
      "full_description": "Math should add two numbers correctly",
      "status": "passed",
      "file_path": "./spec/example_spec.rb",
      "line_number": 11,
      "run_time": 0.00005
    },
    {
      "id": "./spec/example_spec.rb[2:2:1]",
      "description": "should fail a test",
      "full_description": "Math should fail a test",
      "status": "failed",
      "file_path": "./spec/example_spec.rb",
      "line_number": 15,
      "run_time": 0.0005,
      "exception": {
        "class": "RSpec::Expectations::ExpectationNotMetError",
        "message": "\nexpected: 3\n     got: 2\n\n(compared using ==)\n",
        "backtrace": [
          "/app/demo/rspec_project/spec/example_spec.rb:16:in `block (2 levels) in <top (required)>'"
        ]
      }
    },
    {
      "id": "./spec/example_spec.rb[2:3:1]",
      "description": "is a pending test",
      "full_description": "Math is a pending test",
      "status": "pending",
      "file_path": "./spec/example_spec.rb",
      "line_number": 19,
      "pending_message": "No reason given"
    }
  ]
}
        "#;

        let report: RspecReport = serde_json::from_str(rspec_json_output)
            .unwrap_or_else(|e| panic!("Failed to parse RSpec JSON: {}", e));
        
        let runner = RspecRunner::default(); 
        let workspace_root_path = get_demo_project_root();
        let workspace_root_str = workspace_root_path.to_str().unwrap().to_string();
        
        let dummy_file_paths_for_parsing = vec![get_demo_file_path("spec/example_spec.rb").to_str().unwrap().to_string()];
        
        let result: RunFileTestResult = runner.parse_diagnostics(report, &dummy_file_paths_for_parsing, &workspace_root_str)
            .unwrap_or_else(|e| panic!("parse_diagnostics failed: {:?}", e)); // result is RunFileTestResult

        assert_eq!(result.data.len(), 1, "Should be diagnostics for one file. Actual: {:#?}", result.data); // result.data is Vec<FileDiagnostics>
        
        let file_diag: &FileDiagnostics = result.data.first().unwrap(); // file_diag is &FileDiagnostics
        let expected_diag_file_path = get_demo_file_path("spec/example_spec.rb").to_str().unwrap().to_string();
        assert_eq!(file_diag.path, expected_diag_file_path, "Diagnostic file path mismatch");
        
        assert_eq!(file_diag.diagnostics.len(), 1, "Should be one diagnostic for the failing test. Actual: {:#?}", file_diag.diagnostics);
        
        let diag = file_diag.diagnostics.first().unwrap();
        assert_eq!(diag.message, "\nexpected: 3\n     got: 2\n\n(compared using ==)\n", "Diagnostic message mismatch");
        assert_eq!(diag.severity, Some(DiagnosticSeverity::ERROR), "Diagnostic severity mismatch");
        assert_eq!(diag.range.start.line, 14, "Diagnostic start line mismatch"); 
        assert_eq!(diag.range.start.character, 0, "Diagnostic start character mismatch"); 
        assert_eq!(diag.range.end.line, 14, "Diagnostic end line mismatch");
        assert_eq!(diag.range.end.character, MAX_CHAR_LENGTH, "Diagnostic end character mismatch");
    }
}
