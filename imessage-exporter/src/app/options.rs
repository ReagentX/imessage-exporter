use clap::{crate_version, Arg, ArgMatches, Command};

use imessage_database::{
    tables::table::DEFAULT_OUTPUT_DIR,
    util::{
        dirs::{default_db_path, home},
        query_context::QueryContext,
    },
};

// CLI Arg Names
pub const OPTION_PATH: &str = "db-path";
pub const OPTION_COPY: &str = "no-copy";
pub const OPTION_DIAGNOSTIC: &str = "diagnostics";
pub const OPTION_EXPORT_TYPE: &str = "format";
pub const OPTION_EXPORT_PATH: &str = "export-path";
pub const OPTION_START_DATE: &str = "start-date";
pub const OPTION_END_DATE: &str = "end-date";
pub const OPTION_DISABLE_LAZY_LOADING: &str = "no-lazy";

// Other CLI Text
pub const SUPPORTED_FILE_TYPES: &str = "txt, html";
pub const ABOUT: &str = concat!(
    "The `imessage-exporter` binary exports iMessage data to\n",
    "`txt` or `html` formats. It can also run diagnostics\n",
    "to find problems with the iMessage database."
);

pub struct Options<'a> {
    /// Path to database file
    pub db_path: String,
    /// If true, do not copy files from ~/Library to the export
    pub no_copy: bool,
    /// If true, emit diagnostic information to stdout
    pub diagnostic: bool,
    /// The type of file we are exporting data to
    pub export_type: Option<&'a str>,
    /// Where the app will save exported data
    pub export_path: Option<&'a str>,
    /// Query context describing SQL query filters
    pub query_context: QueryContext,
    /// If true, do not include `loading="lazy"` in HTML exports
    pub no_lazy: bool,
    /// Whether the options created are valid or not
    pub valid: bool,
}

impl<'a> Options<'a> {
    pub fn from_args(args: &'a ArgMatches) -> Self {
        let user_path = args.value_of(OPTION_PATH);
        let no_copy = args.is_present(OPTION_COPY);
        let diagnostic = args.is_present(OPTION_DIAGNOSTIC);
        let export_type = args.value_of(OPTION_EXPORT_TYPE);
        let export_path = args.value_of(OPTION_EXPORT_PATH);
        let start_date = args.value_of(OPTION_START_DATE);
        let end_date = args.value_of(OPTION_END_DATE);
        let no_lazy = args.is_present(OPTION_DISABLE_LAZY_LOADING);

        // Validation layer
        let mut valid = true;

        // Ensure export type is allowed
        if let Some(found_type) = export_type {
            if !SUPPORTED_FILE_TYPES
                .split(',')
                .any(|allowed_type| allowed_type.trim() == found_type)
            {
                println!("{found_type} is not a valid export type! Must be one of <{SUPPORTED_FILE_TYPES}>");
                valid = false;
            }
        }

        // Ensure an export type is specified if other export options are selected
        if no_copy && export_type.is_none() {
            println!("No export type selected, required by {OPTION_COPY}");
            valid = false;
        }
        if export_path.is_some() && export_type.is_none() {
            println!("No export type selected, required by {OPTION_EXPORT_PATH}");
            valid = false;
        }

        // Ensure that if diagnostics are enabled, no other options are
        if diagnostic && no_copy {
            println!("Diagnostics are enabled; {OPTION_COPY} is disallowed");
            valid = false;
        }
        if diagnostic && export_path.is_some() {
            println!("Diagnostics are enabled; {OPTION_EXPORT_PATH} is disallowed");
            valid = false;
        }
        if diagnostic && export_type.is_some() {
            println!("Diagnostics are enabled; {OPTION_EXPORT_TYPE} is disallowed");
            valid = false;
        }

        // Build query context
        let mut query_context = QueryContext::default();
        if let Some(start) = start_date {
            if let Err(why) = query_context.set_start(start) {
                eprintln!("{why}");
                valid = false;
            }
        }
        if let Some(end) = end_date {
            if let Err(why) = query_context.set_end(end) {
                eprintln!("{why}");
                valid = false;
            }
        }

        Options {
            db_path: user_path.unwrap_or(&default_db_path()).to_string(),
            no_copy,
            diagnostic,
            export_type,
            export_path,
            query_context,
            no_lazy,
            valid,
        }
    }
}

pub fn from_command_line() -> ArgMatches {
    let matches = Command::new("iMessage Exporter")
        .version(crate_version!())
        .about(ABOUT)
        .arg_required_else_help(true)
        .arg(
            Arg::new(OPTION_DIAGNOSTIC)
            .short('d')
            .long(OPTION_DIAGNOSTIC)
            .help("Print diagnostic information and exit")
            .display_order(0),
        )
        .arg(
            Arg::new(OPTION_EXPORT_TYPE)
            .short('f')
            .long(OPTION_EXPORT_TYPE)
            .help("Specify a single file format to export messages into")
            .takes_value(true)
            .display_order(1)
            .value_name(SUPPORTED_FILE_TYPES),
        )
        .arg(
            Arg::new(OPTION_COPY)
            .short('n')
            .long(OPTION_COPY)
            .help("Do not copy attachments, instead reference them in-place")
            .display_order(2),
        )
        .arg(
            Arg::new(OPTION_PATH)
                .short('p')
                .long(OPTION_PATH)
                .help(&*format!("Specify a custom path for the iMessage database file\nIf omitted, the default directory is {}", default_db_path()))
                .takes_value(true)
                .display_order(3)
                .value_name("path/to/chat.db"),
        )
        .arg(
            Arg::new(OPTION_EXPORT_PATH)
                .short('o')
                .long(OPTION_EXPORT_PATH)
                .help(&*format!("Specify a custom directory for outputting exported data\nIf omitted, the default directory is {}/{DEFAULT_OUTPUT_DIR}", home()))
                .takes_value(true)
                .display_order(4)
                .value_name("path/to/save/files"),
        )
        .arg(
            Arg::new(OPTION_START_DATE)
                .short('s')
                .long(OPTION_START_DATE)
                .help("The start date filter. Only messages sent on or after this date will be included")
                .takes_value(true)
                .display_order(5)
                .value_name("YYYY-MM-DD"),
        )
        .arg(
            Arg::new(OPTION_END_DATE)
                .short('e')
                .long(OPTION_END_DATE)
                .help("The end date filter. Only messages sent before this date will be included")
                .takes_value(true)
                .display_order(6)
                .value_name("YYYY-MM-DD"),
        )
        .arg(
            Arg::new(OPTION_DISABLE_LAZY_LOADING)
            .short('l')
            .long(OPTION_DISABLE_LAZY_LOADING)
            .help("Do not include `loading=\"lazy\"` in HTML export `img` tags\nThis will make pages load slower but PDF generation work")
            .display_order(7),
        )
        .get_matches();
    matches
}
