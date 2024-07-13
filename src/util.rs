use crate::error::LSError;
use serde::Serialize;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
