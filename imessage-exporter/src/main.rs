#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
mod app;
mod exporters;

pub use exporters::{exporter::Exporter, html::HTML, txt::TXT};

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
    if let Err(why) = &options {
        eprintln!("{why}");
    } else {
        match Config::new(options.unwrap()) {
            Ok(app) => {
                if let Err(why) = app.start() {
                    eprintln!("Unable to start: {why}");
                }
            }
            Err(why) => {
                eprintln!("Unable to launch: {why}");
            }
        }
    }
}
