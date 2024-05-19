use crate::runner::cargo_test::CargoTestRunner;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::DetectWorkspaceRootArgs;
use testing_language_server::spec::DiscoverArgs;
use testing_language_server::spec::RunFileTestArgs;

use crate::runner::jest::JestRunner;

#[derive(Debug, Eq, PartialEq)]
pub enum AvailableTestKind {
    CargoTest(CargoTestRunner),
    Jest(JestRunner),
}
impl Runner for AvailableTestKind {
    fn disover(&self, args: DiscoverArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.disover(args),
            AvailableTestKind::Jest(runner) => runner.disover(args),
        }
    }

    fn run_file_test(&self, args: RunFileTestArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.run_file_test(args),
            AvailableTestKind::Jest(runner) => runner.run_file_test(args),
        }
    }

    fn detect_workspaces_root(&self, args: DetectWorkspaceRootArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.detect_workspaces_root(args),
            AvailableTestKind::Jest(runner) => runner.detect_workspaces_root(args),
        }
    }
}

impl FromStr for AvailableTestKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cargo-test" => Ok(AvailableTestKind::CargoTest(CargoTestRunner)),
            "jest" => Ok(AvailableTestKind::Jest(JestRunner)),
            _ => Err(anyhow::anyhow!("Unknown test kind: {}", s)),
        }
    }
}

pub trait Runner {
    fn disover(&self, args: DiscoverArgs) -> Result<(), LSError>;
    fn run_file_test(&self, args: RunFileTestArgs) -> Result<(), LSError>;
    fn detect_workspaces_root(&self, args: DetectWorkspaceRootArgs) -> Result<(), LSError>;
}
