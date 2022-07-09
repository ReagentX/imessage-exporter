#![forbid(unsafe_code)]
#![doc = include_str!("../../docs/binary/README.md")]
mod app;
mod exporters;

pub use exporters::{exporter::Exporter, txt::TXT};

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
    match Config::new(options) {
        Some(app) => app.start(),
        None => {}
    }
}
