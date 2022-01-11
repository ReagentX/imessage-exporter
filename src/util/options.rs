use clap::{App, Arg, ArgMatches};

use crate::tables::table::{OPTION_COPY, OPTION_PATH};

pub fn from_command_line() -> ArgMatches {
    let matches = App::new("iMessage Exporter")
        .version("")
        .about("")
        .arg(
            Arg::new(OPTION_PATH)
                .short('d')
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
        .get_matches();
    matches
}
