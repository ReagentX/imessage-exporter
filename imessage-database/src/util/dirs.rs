/*!
 Contains functions that generate the correct path to the default iMessage database location
*/

use std::{env::var, path::PathBuf};

use crate::tables::table::DEFAULT_PATH_MACOS;

/// Get the user's home directory (macOS only)
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::home;
///
/// let path = home();
/// println!("{path}");
/// ```
pub fn home() -> String {
    match var("HOME") {
        Ok(path) => path,
        Err(why) => panic!("Unable to resolve user home directory: {why}"),
    }
}

/// Get the default path the macOS iMessage database is located at (macOS only)
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::default_db_path;
///
/// let path = default_db_path();
/// println!("{path:?}");
/// ```
pub fn default_db_path() -> PathBuf {
    PathBuf::from(format!("{}/{DEFAULT_PATH_MACOS}", home()))
}
