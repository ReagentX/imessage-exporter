#![forbid(unsafe_code)]

use std::process::ExitCode;

use imessage_exporter::{
    Config,
    app::{
        call_logs,
        options::{Options, from_command_line},
    },
};

fn main() -> ExitCode {
    // Get args from command line
    let args = from_command_line();
    // Create application options
    let options = Options::from_args(&args);

    // Create app state and start
    match options {
        Ok(options) => {
            if options.export_call_logs {
                match call_logs::export_csv_from_options(&options) {
                    Ok((path, result)) => {
                        println!(
                            "Exported {} call log{} from {} to {}",
                            result.entries.len(),
                            if result.entries.len() == 1 { "" } else { "s" },
                            result.source,
                            path.display()
                        );
                        return ExitCode::SUCCESS;
                    }
                    Err(why) => {
                        eprintln!("Unable to export: {why}");
                        return ExitCode::FAILURE;
                    }
                }
            }

            match Config::new(options) {
                Ok(mut app) => {
                    // Resolve the filtered contacts, if provided
                    app.resolve_filtered_handles();

                    if let Err(why) = app.start() {
                        eprintln!("Unable to export: {why}");
                        return ExitCode::FAILURE;
                    }
                    ExitCode::SUCCESS
                }
                Err(why) => {
                    eprintln!("Invalid configuration: {why}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(why) => {
            eprintln!("Invalid command line options: {why}");
            ExitCode::FAILURE
        }
    }
}
