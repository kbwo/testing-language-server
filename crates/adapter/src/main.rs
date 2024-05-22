use crate::model::AvailableTestKind;
use crate::model::Runner;
use clap::Parser;
use std::str::FromStr;
use testing_language_server::spec::AdapterCommands;
pub mod model;
pub mod runner;

fn detect_test_from_extra(extra: &[String]) -> Result<AvailableTestKind, anyhow::Error> {
    let test_kind = extra
        .iter()
        .find(|arg| arg.starts_with("--test-kind="))
        .ok_or_else(|| anyhow::anyhow!("test kind not found"))?;
    let language = test_kind.replace("--test-kind=", "");
    AvailableTestKind::from_str(&language)
}

fn main() {
    let args = AdapterCommands::parse();
    match args {
        AdapterCommands::Discover(args) => {
            let extra = args.extra.clone();
            let test_kind = detect_test_from_extra(&extra).unwrap();
            test_kind.disover(args).unwrap();
        }
        AdapterCommands::RunFileTest(args) => {
            let extra = args.extra.clone();
            let test_kind = detect_test_from_extra(&extra).unwrap();
            test_kind.run_file_test(args).unwrap();
        }
        AdapterCommands::DetectWorkspaceRoot(args) => {
            let extra = args.extra.clone();
            let test_kind = detect_test_from_extra(&extra).unwrap();
            test_kind.detect_workspaces_root(args).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::cargo_test::CargoTestRunner;
    use crate::runner::jest::JestRunner;

    #[test]
    // If `--test-kind=<value>` is not present, then return Err
    fn error_test_kind_detection() {
        let extra = vec![];
        detect_test_from_extra(&extra).unwrap_err();
        let extra = vec!["--foo=bar".to_string()];
        detect_test_from_extra(&extra).unwrap_err();
    }

    #[test]
    // If `--test-kind=<value>` is present, then return Ok(value)
    fn test_kind_detection() {
        let extra = vec!["--test-kind=cargo-test".to_string()];
        let language = detect_test_from_extra(&extra).unwrap();
        assert_eq!(language, AvailableTestKind::CargoTest(CargoTestRunner));
    }

    #[test]
    // If multiple `--test-kind=<value>` are present, then return first one
    fn error_multiple_test_kind_detection() {
        let extra = vec![
            "--test-kind=cargo-test".to_string(),
            "--test-kind=jest".to_string(),
            "--test-kind=foo".to_string(),
        ];
        let test_kind = detect_test_from_extra(&extra).unwrap();
        assert_eq!(test_kind, AvailableTestKind::Jest(JestRunner));
    }
}
