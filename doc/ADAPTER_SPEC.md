# Adapter Specifications

This document outlines the command specifications.

# Commands

These commands must be implemented by the adapter.

- **discover**: Initiates the discovery process.
- **run-file-test**: Executes tests on specified files.
- **detect-workspace**: Identifies the workspace based on provided parameters.

## discover

### Arguments
- `file_paths`: A list of file paths to be processed.

### Stdout
Returns a JSON array of discovered items. Each item is a JSON object containing:
- `path`: String representing the file path.
- `tests`: Array of test items, where each test item is a JSON object including:
  - `id`: String identifier for the test.
  - `name`: String name of the test.
  - `start_position`: [Range](https://docs.rs/lsp-types/latest/lsp_types/struct.Range.html) indicating the start position of the test in the file.
  - `end_position`: [Range](https://docs.rs/lsp-types/latest/lsp_types/struct.Range.html) indicating the end position of the test in the file.

## run-file-test

### Arguments
- `file_paths`: A list of file paths to be tested.
- `workspace`: The workspace identifier where the tests will be executed.

### Stdout
Returns a JSON array of test results. Each result is a JSON object containing:
- `path`: String representing the file path.
- `diagnostics`: Array of [Diagnostic](https://docs.rs/lsp-types/latest/lsp_types/struct.Diagnostic.html) objects.

## detect-workspace

### Arguments
- `file_paths`: A list of file paths to identify the workspace.

### Stdout
Returns a JSON object where:
- Keys are strings representing workspace file paths.
- Values are arrays of strings representing file paths associated with each workspace.

# Note: All stdout must be valid JSON and should be parseable by standard JSON parsers.

