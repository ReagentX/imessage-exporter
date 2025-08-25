/*!
 Contains functions that generate the correct path to the default iMessage database location.
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
#[must_use]
pub fn home() -> String {
    var("HOME").unwrap_or_default()
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
#[must_use]
pub fn default_db_path() -> PathBuf {
    PathBuf::from(format!("{}/{DEFAULT_PATH_MACOS}", home()))
}
