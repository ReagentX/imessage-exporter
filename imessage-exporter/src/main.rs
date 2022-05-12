#![forbid(unsafe_code)]
mod app;
mod exporter;

pub use exporter::{exporter::Exporter, txt::TXT};

use app::{
    options::{from_command_line, Options},
    runtime::Config,
};

fn main() {
    // Get args from command line
    let args = from_command_line();
    // Create application options
    let options = Options::from_args(&args);

    // Create app state and start
    let app = Config::new(options).unwrap();
    app.start()
}
