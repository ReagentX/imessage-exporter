/*!
 Default local paths for Messages data.
*/

use std::{borrow::Cow, env::home_dir, path::PathBuf};

use crate::tables::table::DEFAULT_PATH_MACOS;

/// Return the default macOS Messages database path.
///
/// Resolves the home directory with [`std::env::home_dir`]: `HOME` on Unix and `USERPROFILE` on
/// Windows. Returns `None` when neither the environment nor the OS reports a home directory.
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::default_db_path;
///
/// let path = default_db_path().unwrap();
/// println!("{path:?}");
/// ```
#[must_use]
pub fn default_db_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(DEFAULT_PATH_MACOS))
}

/// Replace a leading `~` with the current user's home directory.
///
/// macOS stores attachment paths with a literal `~` prefix. The path is returned unchanged when it
/// has no `~` prefix or when the home directory is unknown.
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::expand_home;
///
/// let path = expand_home("~/Library/Messages");
/// println!("{path}");
/// ```
#[must_use]
pub fn expand_home(path: &str) -> Cow<'_, str> {
    match (path.strip_prefix('~'), home_dir()) {
        (Some(rest), Some(home)) => Cow::Owned(format!("{}{rest}", home.display())),
        _ => Cow::Borrowed(path),
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, env::home_dir};

    use crate::{
        tables::table::DEFAULT_PATH_MACOS, util::dirs::default_db_path, util::dirs::expand_home,
    };

    #[test]
    fn default_db_path_is_under_home() {
        let home = home_dir().unwrap();
        assert_eq!(default_db_path().unwrap(), home.join(DEFAULT_PATH_MACOS));
    }

    #[test]
    fn expand_home_replaces_leading_tilde() {
        let home = home_dir().unwrap();
        assert_eq!(
            expand_home("~/Library/Messages"),
            format!("{}/Library/Messages", home.display())
        );
    }

    #[test]
    fn expand_home_leaves_other_paths_borrowed() {
        assert!(matches!(
            expand_home("/Library/Messages"),
            Cow::Borrowed("/Library/Messages")
        ));
        assert!(matches!(expand_home("a/~/b"), Cow::Borrowed("a/~/b")));
    }
}
