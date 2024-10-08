mod error;
mod log;
mod server;
mod spec;
mod util;

use std::io::{self, BufRead, Read};

use error::LSError;
use lsp_types::InitializeParams;
use serde::de::Error;
use serde::Deserialize;
use serde_json::{json, Value};
use util::{format_uri, send_stdout};

use crate::log::Log;
use crate::server::TestingLS;

fn extract_textdocument_uri(params: &Value) -> Result<String, serde_json::Error> {
    let uri = params["textDocument"]["uri"]
        .as_str()
        .ok_or(serde_json::Error::custom("`textDocument.uri` is not set"))?;
    Ok(format_uri(uri))
}

fn extract_uri(params: &Value) -> Result<String, serde_json::Error> {
    let uri = params["uri"]
        .as_str()
        .ok_or(serde_json::Error::custom("`uri` is not set"))?;
    Ok(format_uri(uri))
}

fn main_loop(server: &mut TestingLS) -> Result<(), LSError> {
    loop {
        let mut size = 0;
        'read_header: loop {
            let mut buffer = String::new();
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            handle.read_line(&mut buffer)?;

            if buffer.is_empty() {
                tracing::warn!("buffer is empty")
            }

            // The end of header section
            if buffer == "\r\n" {
                break 'read_header;
            }

            let split: Vec<&str> = buffer.split(' ').collect();

            if split.len() != 2 {
                tracing::warn!("unexpected");
            }

            let header_name = split[0].to_lowercase();
            let header_value = split[1].trim();

            match header_name.as_ref() {
                "content-length" => {}
                "content-type:" => {}
                _ => {}
            }

            size = header_value.parse::<usize>().unwrap();
        }

        let stdin = io::stdin();
        let mut handle = stdin.lock();
        let mut buf = vec![0u8; size];
        handle.read_exact(&mut buf).unwrap();
        let message = String::from_utf8(buf).unwrap();

        let value: Value = serde_json::from_str(&message)?;
        let method = &value["method"].as_str();
        tracing::info!("method={:#?}", method);
        let params = &value["params"];
        tracing::info!("params={:#?}", params);

        if let Some(method) = method {
            match *method {
                "initialize" => {
                    let initialize_params = InitializeParams::deserialize(params)?;
                    let id = value["id"].as_i64().unwrap();
                    server.initialize(id, initialize_params)?;
                }
                "shutdown" => {
                    let id = value["id"].as_i64().unwrap();
                    server.shutdown(id)?;
                }
                "exit" => {
                    std::process::exit(0);
                }
                "workspace/diagnostic" => {
                    server.diagnose_workspace()?;
                }
                "textDocument/diagnostic" | "textDocument/didSave" => {
                    let uri = extract_textdocument_uri(params)?;
                    server.check_file(&uri, false)?;
                }
                "textDocument/didOpen" => {
                    let uri = extract_textdocument_uri(params)?;
                    if server.refreshing_needed(&uri) {
                        server.refresh_workspaces_cache()?;
                    }
                }
                "$/runFileTest" => {
                    let uri = extract_uri(params)?;
                    server.check_file(&uri, false)?;
                }
                "$/runWorkspaceTest" => {
                    server.diagnose_workspace()?;
                }
                "$/discoverFileTest" => {
                    let id = value["id"].as_i64().unwrap();
                    let uri = extract_uri(params)?;
                    let result = server.discover_file(&uri)?;
                    send_stdout(&json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result,
                    }))?;
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let mut server = TestingLS::new();
    let _guard = Log::init().expect("Failed to initialize logger");
    if let Err(ls_error) = main_loop(&mut server) {
        tracing::error!("Error: {:?}", ls_error);
    }
}
