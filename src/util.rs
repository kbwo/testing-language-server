use crate::error::LSError;
use chrono::NaiveDate;
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::io::stdout;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

pub fn send_stdout<T>(message: &T) -> Result<(), LSError>
where
    T: ?Sized + Serialize + std::fmt::Debug,
{
    tracing::info!("send stdout: {:#?}", message);
    let msg = serde_json::to_string(message)?;
    let mut stdout = stdout().lock();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
    stdout.flush()?;
    Ok(())
}

pub fn format_uri(uri: &str) -> String {
    uri.replace("file://", "")
}

pub fn resolve_path(base_dir: &Path, relative_path: &str) -> PathBuf {
    let absolute = if Path::new(relative_path).is_absolute() {
        PathBuf::from(relative_path)
    } else {
        base_dir.join(relative_path)
    };

    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::Normal(_) | std::path::Component::RootDir => {
                components.push(component);
            }
            _ => {}
        }
    }

    PathBuf::from_iter(components)
}

pub fn clean_old_logs(
    log_dir: &str,
    retention_days: i64,
    glob_pattern: &str,
    prefix: &str,
) -> Result<(), LSError> {
    let today = Utc::now().date_naive();
    let retention_threshold = today - chrono::Duration::days(retention_days);

    let walker = globwalk::GlobWalkerBuilder::from_patterns(log_dir, &[glob_pattern])
        .build()
        .unwrap();

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            if let Some(date_str) = file_name.strip_prefix(prefix) {
                if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    if file_date < retention_threshold {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_resolve_path() {
        let base_dir = PathBuf::from("/Users/test/projects");

        // relative path
        assert_eq!(
            resolve_path(&base_dir, "github.com/hoge/fuga"),
            PathBuf::from("/Users/test/projects/github.com/hoge/fuga")
        );

        // current directory
        assert_eq!(
            resolve_path(&base_dir, "./github.com/hoge/fuga"),
            PathBuf::from("/Users/test/projects/github.com/hoge/fuga")
        );

        // parent directory
        assert_eq!(
            resolve_path(&base_dir, "../other/project"),
            PathBuf::from("/Users/test/other/project")
        );

        // multiple ..
        assert_eq!(
            resolve_path(&base_dir, "foo/bar/../../../baz"),
            PathBuf::from("/Users/test/baz")
        );

        // absolute path
        assert_eq!(
            resolve_path(&base_dir, "/absolute/path"),
            PathBuf::from("/absolute/path")
        );

        // empty relative path
        assert_eq!(
            resolve_path(&base_dir, ""),
            PathBuf::from("/Users/test/projects")
        );

        // ending /
        assert_eq!(
            resolve_path(&base_dir, "github.com/hoge/fuga/"),
            PathBuf::from("/Users/test/projects/github.com/hoge/fuga")
        );

        // complex path
        assert_eq!(
            resolve_path(&base_dir, "./foo/../bar/./baz/../qux/"),
            PathBuf::from("/Users/test/projects/bar/qux")
        );
    }

    #[test]
    fn test_clean_old_logs() {
        let home_dir = dirs::home_dir().unwrap();
        let log_dir = home_dir.join(".config/testing_language_server/logs");

        // Create test log files
        let old_file = log_dir.join("prefix.log.2023-01-01");
        File::create(&old_file).unwrap();
        let recent_file = log_dir.join("prefix.log.2099-12-31");
        File::create(&recent_file).unwrap();
        let non_log_file = log_dir.join("not_a_log.txt");
        File::create(&non_log_file).unwrap();

        // Run the clean_old_logs function
        clean_old_logs(log_dir.to_str().unwrap(), 30, "prefix.log.*", "prefix.log.").unwrap();

        // Check results
        assert!(!old_file.exists(), "Old log file should be deleted");
        assert!(
            recent_file.exists(),
            "Recent log file should not be deleted"
        );
        assert!(non_log_file.exists(), "Non-log file should not be deleted");
    }
}
