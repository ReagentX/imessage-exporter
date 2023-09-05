use std::path::PathBuf;

use clap::{crate_version, Arg, ArgMatches, Command};

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{
        dirs::{default_db_path, home},
        platform::Platform,
        query_context::QueryContext,
    },
};

use crate::app::{attachment_manager::AttachmentManager, error::RuntimeError};

/// Default export directory name
pub const DEFAULT_OUTPUT_DIR: &str = "imessage_export";

// CLI Arg Names
pub const OPTION_DB_PATH: &str = "db-path";
pub const OPTION_ATTACHMENT_MANAGER: &str = "copy-method";
pub const OPTION_DIAGNOSTIC: &str = "diagnostics";
pub const OPTION_EXPORT_TYPE: &str = "format";
pub const OPTION_EXPORT_PATH: &str = "export-path";
pub const OPTION_START_DATE: &str = "start-date";
pub const OPTION_END_DATE: &str = "end-date";
pub const OPTION_DISABLE_LAZY_LOADING: &str = "no-lazy";
pub const OPTION_CUSTOM_NAME: &str = "custom-name";
pub const OPTION_PLATFORM: &str = "platform";

// Other CLI Text
pub const SUPPORTED_FILE_TYPES: &str = "txt, html";
pub const SUPPORTED_PLATFORMS: &str = "macOS, iOS";
pub const SUPPORTED_ATTACHMENT_MANAGER_MODES: &str = "compatible, efficient, disabled";
pub const ABOUT: &str = concat!(
    "The `imessage-exporter` binary exports iMessage data to\n",
    "`txt` or `html` formats. It can also run diagnostics\n",
    "to find problems with the iMessage database."
);

pub struct Options<'a> {
    /// Path to database file
    pub db_path: PathBuf,
    /// The attachment manager type used to copy files
    pub attachment_manager: AttachmentManager,
    /// If true, emit diagnostic information to stdout
    pub diagnostic: bool,
    /// The type of file we are exporting data to
    pub export_type: Option<&'a str>,
    /// Where the app will save exported data
    pub export_path: PathBuf,
    /// Query context describing SQL query filters
    pub query_context: QueryContext,
    /// If true, do not include `loading="lazy"` in HTML exports
    pub no_lazy: bool,
    /// Custom name for database owner in output
    pub custom_name: Option<&'a str>,
    /// The database source's platform
    pub platform: Platform,
}

impl<'a> Options<'a> {
    pub fn from_args(args: &'a ArgMatches) -> Result<Self, RuntimeError> {
        let user_path = args.value_of(OPTION_DB_PATH);
        let attachment_manager_type = args.value_of(OPTION_ATTACHMENT_MANAGER);
        let diagnostic = args.is_present(OPTION_DIAGNOSTIC);
        let export_type = args.value_of(OPTION_EXPORT_TYPE);
        let export_path = args.value_of(OPTION_EXPORT_PATH);
        let start_date = args.value_of(OPTION_START_DATE);
        let end_date = args.value_of(OPTION_END_DATE);
        let no_lazy = args.is_present(OPTION_DISABLE_LAZY_LOADING);
        let custom_name = args.value_of(OPTION_CUSTOM_NAME);
        let platform_type = args.value_of(OPTION_PLATFORM);

        // Ensure export type is allowed
        if let Some(found_type) = export_type {
            if !SUPPORTED_FILE_TYPES
                .split(',')
                .any(|allowed_type| allowed_type.trim() == found_type)
            {
                return Err(RuntimeError::InvalidOptions(format!(
                    "{found_type} is not a valid export type! Must be one of <{SUPPORTED_FILE_TYPES}>"
                )));
            }
        }

        // Ensure an export type is specified if other export options are selected
        if attachment_manager_type.is_some() && export_type.is_none() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Option {OPTION_ATTACHMENT_MANAGER} is enabled, which requires `--{OPTION_EXPORT_TYPE}`"
            )));
        }
        if export_path.is_some() && export_type.is_none() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Option {OPTION_EXPORT_PATH} is enabled, which requires `--{OPTION_EXPORT_TYPE}`"
            )));
        }
        if no_lazy && export_type != Some("html") {
            return Err(RuntimeError::InvalidOptions(format!(
                "Option {OPTION_DISABLE_LAZY_LOADING} is enabled, which requires `--{OPTION_EXPORT_TYPE}`"
            )));
        }

        // Ensure that if diagnostics are enabled, no other options are
        if diagnostic && attachment_manager_type.is_some() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Diagnostics are enabled; {OPTION_ATTACHMENT_MANAGER} is disallowed"
            )));
        }
        if diagnostic && export_path.is_some() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Diagnostics are enabled; {OPTION_EXPORT_PATH} is disallowed"
            )));
        }
        if diagnostic && export_type.is_some() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Diagnostics are enabled; {OPTION_EXPORT_TYPE} is disallowed"
            )));
        }

        // Build query context
        let mut query_context = QueryContext::default();
        if let Some(start) = start_date {
            if let Err(why) = query_context.set_start(start) {
                return Err(RuntimeError::InvalidOptions(format!("{why}")));
            }
        }
        if let Some(end) = end_date {
            if let Err(why) = query_context.set_end(end) {
                return Err(RuntimeError::InvalidOptions(format!("{why}")));
            }
        }

        // We have to allocate a PathBuf here because it can be created from data owned by this function in the default state
        let db_path = match user_path {
            Some(path) => PathBuf::from(path),
            None => default_db_path(),
        };

        // Determine the attachment manager mode
        let attachment_manager_mode = match attachment_manager_type {
            Some(manager) => {
                AttachmentManager::from_cli(manager).ok_or(RuntimeError::InvalidOptions(format!(
                    "{manager} is not a valid attachment manager mode! Must be one of <{SUPPORTED_ATTACHMENT_MANAGER_MODES}>"
                )))?
            }
            None => AttachmentManager::default(),
        };

        // Build the Platform
        let platform = match platform_type {
            Some(platform_str) => Platform::from_cli(platform_str).ok_or(
                RuntimeError::InvalidOptions(format!(
                "{platform_str} is not a valid platform! Must be one of <{SUPPORTED_PLATFORMS}>")),
            )?,
            None => Platform::determine(&db_path),
        };

        Ok(Options {
            db_path,
            attachment_manager: attachment_manager_mode,
            diagnostic,
            export_type,
            export_path: validate_path(export_path, export_type)?,
            query_context,
            no_lazy,
            custom_name,
            platform,
        })
    }

    /// Generate a path to the database based on the currently selected platform
    pub fn get_db_path(&self) -> PathBuf {
        match self.platform {
            Platform::iOS => self.db_path.join(DEFAULT_PATH_IOS),
            Platform::macOS => self.db_path.clone(),
        }
    }
}

/// Ensure export path is empty or does not contain files of the existing export type
///
/// We have to allocate a PathBuf here because it can be created from data owned by this function in the default state
fn validate_path(
    export_path: Option<&str>,
    export_type: Option<&str>,
) -> Result<PathBuf, RuntimeError> {
    let resolved_path =
        PathBuf::from(export_path.unwrap_or(&format!("{}/{DEFAULT_OUTPUT_DIR}", home())));
    if let Some(export_type) = export_type {
        if resolved_path.exists() {
            let path_word = match export_path {
                Some(_) => "Specified",
                None => "Default",
            };

            match resolved_path.read_dir() {
                Ok(files) => {
                    for file in files.flatten() {
                        if file
                            .path()
                            .extension()
                            .map(|s| s.to_str().unwrap_or("") == export_type)
                            .unwrap_or(false)
                        {
                            return Err(RuntimeError::InvalidOptions(format!(
                                        "{path_word} export path {resolved_path:?} contains existing \"{export_type}\" export data!"
                                    )));
                        }
                    }
                }
                Err(why) => {
                    return Err(RuntimeError::InvalidOptions(format!(
                        "{path_word} export path {resolved_path:?} is not a valid directory: {why}"
                    )));
                }
            }
        }
    };

    Ok(resolved_path)
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
            Arg::new(OPTION_ATTACHMENT_MANAGER)
            .short('c')
            .long(OPTION_ATTACHMENT_MANAGER)
            .help(&*format!("Specify a method to use when copying message attachments\nCompatible will convert HEIC files to JPEG\nEfficient will copy files without converting anything\nIf omitted, the default is `{}`", AttachmentManager::default()))
            .takes_value(true)
            .display_order(2)
            .value_name(SUPPORTED_ATTACHMENT_MANAGER_MODES),
        )
        .arg(
            Arg::new(OPTION_DB_PATH)
                .short('p')
                .long(OPTION_DB_PATH)
                .help(&*format!("Specify a custom path for the iMessage database location\nFor macOS, specify a path to a `chat.db` file\nFor iOS, specify a path to the root of an unencrypted backup directory\nIf omitted, the default directory is {}", default_db_path().display()))
                .takes_value(true)
                .display_order(3)
                .value_name("path/to/source"),
        )
        .arg(
            Arg::new(OPTION_PLATFORM)
            .short('a')
            .long(OPTION_PLATFORM)
            .help("Specify the platform the database was created on\nIf omitted, the platform type is determined automatically")
            .takes_value(true)
            .display_order(4)
            .value_name(SUPPORTED_PLATFORMS),
        )
        .arg(
            Arg::new(OPTION_EXPORT_PATH)
                .short('o')
                .long(OPTION_EXPORT_PATH)
                .help(&*format!("Specify a custom directory for outputting exported data\nIf omitted, the default directory is {}/{DEFAULT_OUTPUT_DIR}", home()))
                .takes_value(true)
                .display_order(5)
                .value_name("path/to/save/files"),
        )
        .arg(
            Arg::new(OPTION_START_DATE)
                .short('s')
                .long(OPTION_START_DATE)
                .help("The start date filter. Only messages sent on or after this date will be included")
                .takes_value(true)
                .display_order(6)
                .value_name("YYYY-MM-DD"),
        )
        .arg(
            Arg::new(OPTION_END_DATE)
                .short('e')
                .long(OPTION_END_DATE)
                .help("The end date filter. Only messages sent before this date will be included")
                .takes_value(true)
                .display_order(7)
                .value_name("YYYY-MM-DD"),
        )
        .arg(
            Arg::new(OPTION_DISABLE_LAZY_LOADING)
                .short('l')
                .long(OPTION_DISABLE_LAZY_LOADING)
                .help("Do not include `loading=\"lazy\"` in HTML export `img` tags\nThis will make pages load slower but PDF generation work")
                .display_order(8),
        )
        .arg(
            Arg::new(OPTION_CUSTOM_NAME)
                .short('m')
                .long(OPTION_CUSTOM_NAME)
                .help("Specify an optional custom name for the database owner's messages in exports")
                .takes_value(true)
                .display_order(9)
        )
        .get_matches();
    matches
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::app::options::{validate_path, DEFAULT_OUTPUT_DIR};
    use imessage_database::util::dirs::home;

    #[test]
    fn can_validate_empty() {
        let export_path = Some("/tmp");
        let export_type = Some("txt");

        let result = validate_path(export_path, export_type);

        assert_eq!(result.unwrap(), PathBuf::from("/tmp"))
    }

    #[test]
    fn can_validate_different_type() {
        let export_path = Some("/tmp");
        let export_type = Some("txt");

        let result = validate_path(export_path, export_type);

        let mut tmp = PathBuf::from("/tmp");
        tmp.push("fake1.html");
        let mut file = fs::File::create(&tmp).unwrap();
        file.write_all(&[]).unwrap();

        assert_eq!(result.unwrap(), PathBuf::from("/tmp"));
        fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn can_validate_same_type() {
        let export_path = Some("/tmp");
        let export_type = Some("txt");

        let result = validate_path(export_path, export_type);

        let mut tmp = PathBuf::from("/tmp");
        tmp.push("fake2.txt");
        let mut file = fs::File::create(&tmp).unwrap();
        file.write_all(&[]).unwrap();

        assert_eq!(result.unwrap(), PathBuf::from("/tmp"));
        fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn can_validate_none() {
        let export_path = None;
        let export_type = None;

        let result = validate_path(export_path, export_type);

        assert_eq!(
            result.unwrap(),
            PathBuf::from(&format!("{}/{DEFAULT_OUTPUT_DIR}", home()))
        );
    }
}
