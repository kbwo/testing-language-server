use clap::Parser;
use lsp_types::Diagnostic;
use lsp_types::Range;
use lsp_types::ShowMessageParams;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Parser, Debug)]
pub enum AdapterCommands {
    Discover(DiscoverArgs),
    RunFileTest(RunFileTestArgs),
    DetectWorkspace(DetectWorkspaceArgs),
}

/// Arguments for `<adapter command> discover` command
#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct DiscoverArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,
    #[arg(last = true)]
    pub extra: Vec<String>,
}

/// Arguments for `<adapter command> run-file-test` command
#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct RunFileTestArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,

    #[arg(short, long)]
    pub workspace: String,

    #[arg(last = true)]
    pub extra: Vec<String>,
}

/// Arguments for `<adapter command> detect-workspace` command
#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct DetectWorkspaceArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub type AdapterId = String;
pub type FilePath = String;
pub type WorkspaceFilePath = String;

#[derive(Debug, Serialize, Clone)]
pub struct WorkspaceAnalysis {
    pub adapter_config: AdapterConfiguration,
    pub workspaces: DetectWorkspaceResult,
}

impl WorkspaceAnalysis {
    pub fn new(adapter_config: AdapterConfiguration, workspaces: DetectWorkspaceResult) -> Self {
        Self {
            adapter_config,
            workspaces,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize, Default)]
pub struct AdapterConfiguration {
    pub path: String,
    #[serde(default)]
    pub extra_arg: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub workspace_dir: Option<String>,
}

/// Result of `<adapter command> detect-workspace`
#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct DetectWorkspaceResult {
    pub data: HashMap<WorkspaceFilePath, Vec<FilePath>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct FileDiagnostics {
    pub path: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of `<adapter command> run-file-test`
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct RunFileTestResult {
    pub data: Vec<FileDiagnostics>,
    #[serde(default)]
    pub messages: Vec<ShowMessageParams>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct TestItem {
    pub id: String,
    pub name: String,
    /// Although FoundFileTests also has a `path` field, we keep the `path` field in TestItem
    /// because sometimes we need to determine where a TestItem is located on its own
    /// Example: In Rust tests, determining which file contains a test from IDs like relative::path::tests::id
    /// TODO: Remove FoundFileTests.path once we confirm it's no longer needed
    pub path: String,
    pub start_position: Range,
    pub end_position: Range,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct FoundFileTests {
    pub path: String,
    pub tests: Vec<TestItem>,
}

/// Result of `<adapter command> discover`
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct DiscoverResult {
    pub data: Vec<FoundFileTests>,
}
