use clap::Parser;
use lsp_types::Diagnostic;
use lsp_types::Range;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Parser, Debug)]
pub enum AdapterCommands {
    Discover(DiscoverArgs),
    RunFileTest(RunFileTestArgs),
    DetectWorkspace(DetectWorkspaceArgs),
}

#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct DiscoverArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,
    #[arg(last = true)]
    pub extra: Vec<String>,
}

#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct RunFileTestArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,

    #[arg(short, long)]
    pub workspace_root: String,

    #[arg(last = true)]
    pub extra: Vec<String>,
}

#[derive(clap::Args, Debug)]
#[command(version, about, long_about = None)]
pub struct DetectWorkspaceArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub(crate) type AdapterId = String;
pub(crate) type FilePath = String;
pub(crate) type WorkspaceFilePath = String;

#[derive(Debug, Serialize, Clone)]
pub struct WorkspaceAnalysis {
    pub adapter_config: AdapterConfiguration,
    pub workspace_roots: DetectWorkspaceResult,
}

impl WorkspaceAnalysis {
    pub fn new(
        adapter_config: AdapterConfiguration,
        workspace_roots: DetectWorkspaceResult,
    ) -> Self {
        Self {
            adapter_config,
            workspace_roots,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AdapterConfiguration {
    pub path: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub envs: HashMap<String, String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

pub type DetectWorkspaceResult = HashMap<WorkspaceFilePath, Vec<FilePath>>;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct RunFileTestResultItem {
    pub path: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub type RunFileTestResult = Vec<RunFileTestResultItem>;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct TestItem {
    pub id: String,
    pub name: String,
    pub start_position: Range,
    pub end_position: Range,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct DiscoverResultItem {
    pub path: String,
    pub tests: Vec<TestItem>,
}

pub type DiscoverResult = Vec<DiscoverResultItem>;
