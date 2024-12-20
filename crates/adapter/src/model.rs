use crate::runner::cargo_nextest::CargoNextestRunner;
use crate::runner::cargo_test::CargoTestRunner;
use crate::runner::deno::DenoRunner;
use crate::runner::go::GoTestRunner;
use crate::runner::node_test::NodeTestRunner;
use crate::runner::phpunit::PhpunitRunner;
use crate::runner::vitest::VitestRunner;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::DetectWorkspaceArgs;
use testing_language_server::spec::DiscoverArgs;
use testing_language_server::spec::RunFileTestArgs;

use crate::runner::jest::JestRunner;

#[derive(Debug, Eq, PartialEq)]
pub enum AvailableTestKind {
    CargoTest(CargoTestRunner),
    CargoNextest(CargoNextestRunner),
    Jest(JestRunner),
    Vitest(VitestRunner),
    Deno(DenoRunner),
    GoTest(GoTestRunner),
    Phpunit(PhpunitRunner),
    NodeTest(NodeTestRunner),
}
impl Runner for AvailableTestKind {
    fn discover(&self, args: DiscoverArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.discover(args),
            AvailableTestKind::CargoNextest(runner) => runner.discover(args),
            AvailableTestKind::Jest(runner) => runner.discover(args),
            AvailableTestKind::Deno(runner) => runner.discover(args),
            AvailableTestKind::GoTest(runner) => runner.discover(args),
            AvailableTestKind::Vitest(runner) => runner.discover(args),
            AvailableTestKind::Phpunit(runner) => runner.discover(args),
            AvailableTestKind::NodeTest(runner) => runner.discover(args),
        }
    }

    fn run_file_test(&self, args: RunFileTestArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.run_file_test(args),
            AvailableTestKind::CargoNextest(runner) => runner.run_file_test(args),
            AvailableTestKind::Jest(runner) => runner.run_file_test(args),
            AvailableTestKind::Deno(runner) => runner.run_file_test(args),
            AvailableTestKind::GoTest(runner) => runner.run_file_test(args),
            AvailableTestKind::Vitest(runner) => runner.run_file_test(args),
            AvailableTestKind::Phpunit(runner) => runner.run_file_test(args),
            AvailableTestKind::NodeTest(runner) => runner.run_file_test(args),
        }
    }

    fn detect_workspaces(&self, args: DetectWorkspaceArgs) -> Result<(), LSError> {
        match self {
            AvailableTestKind::CargoTest(runner) => runner.detect_workspaces(args),
            AvailableTestKind::CargoNextest(runner) => runner.detect_workspaces(args),
            AvailableTestKind::Jest(runner) => runner.detect_workspaces(args),
            AvailableTestKind::Deno(runner) => runner.detect_workspaces(args),
            AvailableTestKind::GoTest(runner) => runner.detect_workspaces(args),
            AvailableTestKind::Vitest(runner) => runner.detect_workspaces(args),
            AvailableTestKind::Phpunit(runner) => runner.detect_workspaces(args),
            AvailableTestKind::NodeTest(runner) => runner.detect_workspaces(args),
        }
    }
}

impl FromStr for AvailableTestKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cargo-test" => Ok(AvailableTestKind::CargoTest(CargoTestRunner)),
            "cargo-nextest" => Ok(AvailableTestKind::CargoNextest(CargoNextestRunner)),
            "jest" => Ok(AvailableTestKind::Jest(JestRunner)),
            "go-test" => Ok(AvailableTestKind::GoTest(GoTestRunner)),
            "vitest" => Ok(AvailableTestKind::Vitest(VitestRunner)),
            "deno" => Ok(AvailableTestKind::Deno(DenoRunner)),
            "phpunit" => Ok(AvailableTestKind::Phpunit(PhpunitRunner)),
            "node-test" => Ok(AvailableTestKind::NodeTest(NodeTestRunner)),
            _ => Err(anyhow::anyhow!("Unknown test kind: {}", s)),
        }
    }
}

pub trait Runner {
    fn discover(&self, args: DiscoverArgs) -> Result<(), LSError>;
    fn run_file_test(&self, args: RunFileTestArgs) -> Result<(), LSError>;
    fn detect_workspaces(&self, args: DetectWorkspaceArgs) -> Result<(), LSError>;
}
