use crate::Runner;

#[derive(Eq, PartialEq, Debug)]
pub struct GoRunner;

impl Runner for GoRunner {
    fn disover(
        &self,
        args: testing_language_server::spec::DiscoverArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        todo!()
    }

    fn run_file_test(
        &self,
        args: testing_language_server::spec::RunFileTestArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        todo!()
    }

    fn detect_workspaces_root(
        &self,
        args: testing_language_server::spec::DetectWorkspaceRootArgs,
    ) -> Result<(), testing_language_server::error::LSError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostics() {}

    #[test]
    fn test_run_file_test() {}

    #[test]
    fn test_detect_workspace_root() {}
}
