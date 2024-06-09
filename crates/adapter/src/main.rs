use crate::model::AvailableTestKind;
use crate::model::Runner;
use clap::Parser;
use std::str::FromStr;
use testing_language_server::spec::AdapterCommands;
use testing_language_server::spec::DetectWorkspaceRootArgs;
use testing_language_server::spec::DiscoverArgs;
use testing_language_server::spec::RunFileTestArgs;
pub mod model;
pub mod runner;

fn pick_test_from_extra(
    extra: &mut [String],
) -> Result<(Vec<String>, AvailableTestKind), anyhow::Error> {
    // extraから--test-kind=のものを取り出し、元の配列から`--test-kind=`のものは除外する
    let mut extra = extra.to_vec();
    let index = extra
        .iter()
        .position(|arg| arg.starts_with("--test-kind="))
        .unwrap();
    let test_kind = extra.remove(index);

    let language = test_kind.replace("--test-kind=", "");
    Ok((extra, AvailableTestKind::from_str(&language)?))
}

fn main() {
    let args = AdapterCommands::parse();
    match args {
        AdapterCommands::Discover(mut args) => {
            let (extra, test_kind) = pick_test_from_extra(&mut args.extra).unwrap();
            test_kind.disover(DiscoverArgs { extra, ..args }).unwrap();
        }
        AdapterCommands::RunFileTest(mut args) => {
            let (extra, test_kind) = pick_test_from_extra(&mut args.extra).unwrap();
            test_kind
                .run_file_test(RunFileTestArgs { extra, ..args })
                .unwrap();
        }
        AdapterCommands::DetectWorkspaceRoot(mut args) => {
            let (extra, test_kind) = pick_test_from_extra(&mut args.extra).unwrap();
            test_kind
                .detect_workspaces_root(DetectWorkspaceRootArgs { extra, ..args })
                .unwrap();
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
        let mut extra = vec![];
        pick_test_from_extra(&mut extra).unwrap_err();
        let mut extra = vec!["--foo=bar".to_string()];
        pick_test_from_extra(&mut extra).unwrap_err();
    }

    #[test]
    // If `--test-kind=<value>` is present, then return Ok(value)
    fn test_kind_detection() {
        let mut extra = vec!["--test-kind=cargo-test".to_string()];
        let (_, language) = pick_test_from_extra(&mut extra).unwrap();
        assert_eq!(language, AvailableTestKind::CargoTest(CargoTestRunner));
    }

    #[test]
    // If multiple `--test-kind=<value>` are present, then return first one
    fn error_multiple_test_kind_detection() {
        let mut extra = vec![
            "--test-kind=cargo-test".to_string(),
            "--test-kind=jest".to_string(),
            "--test-kind=foo".to_string(),
        ];
        let (_, test_kind) = pick_test_from_extra(&mut extra).unwrap();
        assert_eq!(test_kind, AvailableTestKind::Jest(JestRunner));
    }
}
