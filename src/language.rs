use std::collections::HashMap;
use std::str::FromStr;
use strum::{AsRefStr, Display};

use once_cell::sync::Lazy;

type Extension<'a> = &'a str;

#[derive(Display, AsRefStr, Eq, PartialEq, Hash)]
pub enum AvailableFileType {
    #[strum(serialize = "rust")]
    Rust,
    #[strum(serialize = "javascript")]
    Javascript,
    #[strum(serialize = "javascriptreact")]
    JavascriptReact,
    #[strum(serialize = "typescript")]
    Typescript,
    #[strum(serialize = "typescriptreact")]
    TypescriptReact,
}

impl FromStr for AvailableFileType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rust" => Ok(AvailableFileType::Rust),
            "javascript" => Ok(AvailableFileType::Javascript),
            "javascriptreact" => Ok(AvailableFileType::JavascriptReact),
            "typescript" => Ok(AvailableFileType::Typescript),
            "typescriptreact" => Ok(AvailableFileType::TypescriptReact),
            _ => Err(format!("Unknown file type: {}", s)),
        }
    }
}

pub static LANGUAGE_ID_MAP: Lazy<HashMap<AvailableFileType, Vec<Extension>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(AvailableFileType::Rust, vec!["rs"]);
    map.insert(AvailableFileType::Javascript, vec!["js", "jsx"]);
    map.insert(AvailableFileType::JavascriptReact, vec!["js", "jsx"]);
    map.insert(AvailableFileType::Typescript, vec!["ts", "tsx"]);
    map.insert(AvailableFileType::TypescriptReact, vec!["ts", "tsx"]);
    map
});
