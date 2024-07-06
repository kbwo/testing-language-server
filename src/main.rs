mod error;
mod log;
mod server;
mod spec;
mod util;

use crate::log::Log;
use crate::server::TestingLS;

fn main() {
    let mut server = TestingLS::new();
    let _guard = Log::init().expect("Failed to initialize logger");
    if let Err(ls_error) = server.main_loop() {
        tracing::error!("Error: {:?}", ls_error);
    }
}
