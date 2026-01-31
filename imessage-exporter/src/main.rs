#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
mod app;
mod exporters;

pub use exporters::{exporter::Exporter, html::HTML, txt::TXT};

use app::{
    options::{Options, from_command_line},
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
        match options {
            Ok(options) => match Config::new(options) {
                Ok(mut app) => {
                    // Resolve the filtered contacts, if provided
                    app.resolve_filtered_handles();
                    // Optional: dump participants map when --debug-participants was passed
                    app.debug_print_participants();

                    if let Err(why) = app.start() {
                        eprintln!("Unable to export: {why}");
                    }
                }
                Err(why) => {
                    eprintln!("Invalid configuration: {why}");
                }
            },
            Err(why) => eprintln!("Invalid command line options: {why}"),
        }
    }
}
