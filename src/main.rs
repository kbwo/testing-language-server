use testing_language_server::log::Log;
use testing_language_server::server::TestingLS;

fn main() {
    let mut server = TestingLS::new();
    let _guard = Log::init().expect("Failed to initialize logger");
    if let Err(ls_error) = server.main_loop() {
        tracing::error!("Error: {:?}", ls_error);
    }
}
