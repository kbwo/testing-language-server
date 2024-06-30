use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

/// determine if a particular file is the root of workspace based on whether it is in the same directory
pub fn detect_workspace_from_file(file_path: PathBuf, file_names: &[String]) -> Option<String> {
    let parent = file_path.parent();
    if let Some(parent) = parent {
        if file_names
            .iter()
            .any(|file_name| parent.join(file_name).exists())
        {
            return Some(parent.to_string_lossy().to_string());
        } else {
            detect_workspace_from_file(parent.to_path_buf(), file_names)
        }
    } else {
        None
    }
}

pub fn detect_workspaces_from_file_paths(
    target_file_paths: &[String],
    file_names: &[String],
) -> HashMap<String, Vec<String>> {
    let mut result_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut file_paths = target_file_paths.to_vec();
    file_paths.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for file_path in file_paths {
        let existing_workspace = result_map
            .iter()
            .find(|(workspace_root, _)| file_path.contains(workspace_root.as_str()));
        if let Some((workspace_root, _)) = existing_workspace {
            result_map
                .entry(workspace_root.to_string())
                .or_default()
                .push(file_path.clone());
        } else {
            let workspace =
                detect_workspace_from_file(PathBuf::from_str(&file_path).unwrap(), file_names);
            if let Some(workspace) = workspace {
                result_map
                    .entry(workspace)
                    .or_default()
                    .push(file_path.clone());
            }
        }
    }
    result_map
}
