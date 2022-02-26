use clap::{App, Arg, ArgMatches};

// CLI Arg Names
pub const OPTION_PATH: &str = "db-path";
pub const OPTION_COPY: &str = "no-copy";
pub const OPTION_DIAGNOSTIC: &str = "diagnostics";

pub fn from_command_line() -> ArgMatches {
    let matches = App::new("iMessage Exporter")
        .version("")
        .about("")
        .arg(
            Arg::new(OPTION_PATH)
                .short('p')
                .long(OPTION_PATH)
                .help("Specify a custom path for the iMessage databse file")
                .takes_value(true)
                .value_name("path/to/chat.db"),
        )
        .arg(
            Arg::new(OPTION_COPY)
                .short('n')
                .long(OPTION_COPY)
                .help("Do not copy attachments, instead reference them in-place"),
        )
        .arg(
            Arg::new(OPTION_DIAGNOSTIC)
                .short('d')
                .long(OPTION_DIAGNOSTIC)
                .help("Print iMessage Database Diagnostics and exit"),
        )
        .get_matches();
    matches
}
