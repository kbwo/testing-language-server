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
    DetectWorkspaceRoot(DetectWorkspaceRootArgs),
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
pub struct DetectWorkspaceRootArgs {
    #[arg(short, long)]
    pub file_paths: Vec<String>,
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub(crate) type Extension = String;
pub(crate) type FilePath = String;
pub(crate) type AdapterCommandPath = String;
pub(crate) type WorkspaceRootFilePath = String;

#[derive(Debug, Deserialize, Clone)]
pub struct AdapterConfiguration {
    pub path: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub envs: HashMap<String, String>,
}

pub type DetectWorkspaceRootResult = HashMap<WorkspaceRootFilePath, Vec<FilePath>>;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
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
