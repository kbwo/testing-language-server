use crate::model::AvailableTestKind;
use crate::model::Runner;
use anyhow::anyhow;
use clap::Parser;
use log::Log;
use std::io;
use std::io::Write;
use std::str::FromStr;
use testing_language_server::error::LSError;
use testing_language_server::spec::AdapterCommands;
use testing_language_server::spec::DetectWorkspaceArgs;
use testing_language_server::spec::DiscoverArgs;
use testing_language_server::spec::RunFileTestArgs;
pub mod log;
pub mod model;
pub mod runner;

fn pick_test_from_extra(
    extra: &mut [String],
) -> Result<(Vec<String>, AvailableTestKind), anyhow::Error> {
    let mut extra = extra.to_vec();
    let index = extra
        .iter()
        .position(|arg| arg.starts_with("--test-kind="))
        .ok_or(anyhow!("test-kind is not found"))?;
    let test_kind = extra.remove(index);

    let language = test_kind.replace("--test-kind=", "");
    Ok((extra, AvailableTestKind::from_str(&language)?))
}

fn handle(commands: AdapterCommands) -> Result<(), LSError> {
    match commands {
        AdapterCommands::Discover(mut commands) => {
            let (extra, test_kind) = pick_test_from_extra(&mut commands.extra).unwrap();
            test_kind.discover(DiscoverArgs { extra, ..commands })?;
            Ok(())
        }
        AdapterCommands::RunFileTest(mut commands) => {
            let (extra, test_kind) = pick_test_from_extra(&mut commands.extra)?;
            test_kind.run_file_test(RunFileTestArgs { extra, ..commands })?;
            Ok(())
        }
        AdapterCommands::DetectWorkspace(mut commands) => {
            let (extra, test_kind) = pick_test_from_extra(&mut commands.extra)?;
            test_kind.detect_workspaces(DetectWorkspaceArgs { extra, ..commands })?;
            Ok(())
        }
    }
}

fn main() {
    let _guard = Log::init().expect("Failed to initialize logger");
    let args = AdapterCommands::parse();
    tracing::info!("adapter args={:#?}", args);
    if let Err(error) = handle(args) {
        io::stderr()
            .write_all(format!("{:#?}", error).as_bytes())
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::cargo_test::CargoTestRunner;

    #[test]
    fn error_test_kind_detection() {
        let mut extra = vec![];
        pick_test_from_extra(&mut extra).unwrap_err();
        let mut extra = vec!["--foo=bar".to_string()];
        pick_test_from_extra(&mut extra).unwrap_err();
    }

    #[test]
    fn single_test_kind_detection() {
        let mut extra = vec!["--test-kind=cargo-test".to_string()];
        let (_, language) = pick_test_from_extra(&mut extra).unwrap();
        assert_eq!(language, AvailableTestKind::CargoTest(CargoTestRunner));
    }

    #[test]
    fn multiple_test_kind_results_first_kind() {
        let mut extra = vec![
            "--test-kind=cargo-test".to_string(),
            "--test-kind=jest".to_string(),
            "--test-kind=foo".to_string(),
        ];
        let (_, test_kind) = pick_test_from_extra(&mut extra).unwrap();
        assert_eq!(test_kind, AvailableTestKind::CargoTest(CargoTestRunner));
    }
}
