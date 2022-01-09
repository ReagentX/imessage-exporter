use clap::{App, Arg, ArgMatches};

pub fn from_command_line() -> ArgMatches {
    let matches = App::new("iMessage Exporter")
        .version("")
        .about("")
        .arg(
            Arg::new("db-path")
                .short('d')
                .long("db-path")
                .help("Specify a custom path for the iMessage databse file")
                .takes_value(true)
                .value_name("path/to/chat.db"),
        )
        .arg(
            Arg::new("no-copy")
                .short('n')
                .long("no-copy")
                .help("Do not copy attachments, instead reference them in-place"),
        )
        .get_matches();
    matches
}
