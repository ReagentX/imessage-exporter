//! Persistent UI settings, stored as JSON next to the executable so the app is
//! portable (no registry, no per-user install location required). If the
//! executable's directory is not writable, falls back to the OS config dir.
//!
//! The database password is intentionally never persisted.

use std::{io::ErrorKind, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{CopyMethod, FormatChoice, NameMode, PlatformChoice};

const FILE_NAME: &str = "imessage-gui-settings.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub backup_path: String,
    pub platform: PlatformStr,
    pub contacts_path: String,
    pub attachment_root: String,

    pub format: FormatStr,
    pub copy_method: CopyStr,
    pub name_mode: NameStr,
    pub custom_name: String,
    pub no_lazy: bool,
    pub ignore_disk_space: bool,
    pub export_path: String,

    pub start_enabled: bool,
    pub start_date: String,
    pub start_time: String,
    pub end_enabled: bool,
    pub end_date: String,
    pub end_time: String,

    pub sort_by_count: bool,
    pub show_activity_log: bool,
}

pub struct LoadResult {
    pub settings: Settings,
    pub warning: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backup_path: String::new(),
            platform: PlatformStr::Auto,
            contacts_path: String::new(),
            attachment_root: String::new(),
            format: FormatStr::Html,
            copy_method: CopyStr::Disabled,
            name_mode: NameStr::Me,
            custom_name: String::new(),
            no_lazy: false,
            ignore_disk_space: false,
            export_path: String::new(),
            start_enabled: false,
            start_date: String::new(),
            start_time: String::new(),
            end_enabled: false,
            end_date: String::new(),
            end_time: String::new(),
            sort_by_count: false,
            show_activity_log: false,
        }
    }
}

// Serializable mirrors of the model enums (kept here so the model stays free of
// serde derives and the on-disk format is stable and human-readable).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PlatformStr {
    Auto,
    MacOS,
    #[allow(clippy::upper_case_acronyms)]
    IOS,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum FormatStr {
    Html,
    Txt,
    Pdf,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CopyStr {
    Disabled,
    Clone,
    Basic,
    Full,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum NameStr {
    Me,
    Custom,
    CallerId,
}

impl From<PlatformChoice> for PlatformStr {
    fn from(v: PlatformChoice) -> Self {
        match v {
            PlatformChoice::Auto => PlatformStr::Auto,
            PlatformChoice::MacOS => PlatformStr::MacOS,
            PlatformChoice::IOS => PlatformStr::IOS,
        }
    }
}
impl From<PlatformStr> for PlatformChoice {
    fn from(v: PlatformStr) -> Self {
        match v {
            PlatformStr::Auto => PlatformChoice::Auto,
            PlatformStr::MacOS => PlatformChoice::MacOS,
            PlatformStr::IOS => PlatformChoice::IOS,
        }
    }
}
impl From<FormatChoice> for FormatStr {
    fn from(v: FormatChoice) -> Self {
        match v {
            FormatChoice::Html => FormatStr::Html,
            FormatChoice::Txt => FormatStr::Txt,
            FormatChoice::Pdf => FormatStr::Pdf,
        }
    }
}
impl From<FormatStr> for FormatChoice {
    fn from(v: FormatStr) -> Self {
        match v {
            FormatStr::Html => FormatChoice::Html,
            FormatStr::Txt => FormatChoice::Txt,
            FormatStr::Pdf => FormatChoice::Pdf,
        }
    }
}
impl From<CopyMethod> for CopyStr {
    fn from(v: CopyMethod) -> Self {
        match v {
            CopyMethod::Disabled => CopyStr::Disabled,
            CopyMethod::Clone => CopyStr::Clone,
            CopyMethod::Basic => CopyStr::Basic,
            CopyMethod::Full => CopyStr::Full,
        }
    }
}
impl From<CopyStr> for CopyMethod {
    fn from(v: CopyStr) -> Self {
        match v {
            CopyStr::Disabled => CopyMethod::Disabled,
            CopyStr::Clone => CopyMethod::Clone,
            CopyStr::Basic => CopyMethod::Basic,
            CopyStr::Full => CopyMethod::Full,
        }
    }
}
impl From<NameMode> for NameStr {
    fn from(v: NameMode) -> Self {
        match v {
            NameMode::Me => NameStr::Me,
            NameMode::Custom => NameStr::Custom,
            NameMode::CallerId => NameStr::CallerId,
        }
    }
}
impl From<NameStr> for NameMode {
    fn from(v: NameStr) -> Self {
        match v {
            NameStr::Me => NameMode::Me,
            NameStr::Custom => NameMode::Custom,
            NameStr::CallerId => NameMode::CallerId,
        }
    }
}

/// Preferred settings path: next to the executable.
fn primary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    Some(dir.join(FILE_NAME))
}

/// Fallback settings path in the OS config/app-data directory.
fn fallback_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    let dir = base.join("imessage-gui");
    Some(dir.join(FILE_NAME))
}

impl Settings {
    /// Load settings from the primary location, then the fallback. Returns
    /// `Settings::default()` if neither exists, with warnings for real read or
    /// parse failures so the GUI can surface them.
    pub fn load() -> LoadResult {
        let mut warnings = Vec::new();
        for path in [primary_path(), fallback_path()].into_iter().flatten() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Settings>(&text) {
                    Ok(settings) => {
                        return LoadResult {
                            settings,
                            warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
                        };
                    }
                    Err(why) => warnings.push(format!(
                        "Could not parse settings at {}: {why}",
                        path.display()
                    )),
                },
                Err(why) if why.kind() == ErrorKind::NotFound => {}
                Err(why) => warnings.push(format!(
                    "Could not read settings at {}: {why}",
                    path.display()
                )),
            }
        }
        LoadResult {
            settings: Settings::default(),
            warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
        }
    }

    #[cfg(test)]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    #[cfg(test)]
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap()
    }

    /// Persist settings. Tries beside the executable first; if that location is
    /// not writable, falls back to the OS config directory. Returns the path
    /// written, or an error string.
    pub fn save(&self) -> Result<PathBuf, String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut last_err = String::from("no writable settings location found");
        for path in [primary_path(), fallback_path()].into_iter().flatten() {
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    last_err = format!("{}: {e}", parent.display());
                    continue;
                }
            }
            match std::fs::write(&path, &json) {
                Ok(()) => return Ok(path),
                Err(e) => last_err = format!("{}: {e}", path.display()),
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            backup_path: r"C:\Users\me\Backup".to_string(),
            platform: PlatformStr::IOS,
            format: FormatStr::Pdf,
            copy_method: CopyStr::Full,
            name_mode: NameStr::Custom,
            custom_name: "Randy".to_string(),
            start_enabled: true,
            start_date: "2023-01-01".to_string(),
            sort_by_count: true,
            show_activity_log: true,
            ..Default::default()
        };

        let json = s.to_json();
        let back = Settings::from_json(&json);

        assert_eq!(back.backup_path, s.backup_path);
        assert!(matches!(back.platform, PlatformStr::IOS));
        assert!(matches!(back.format, FormatStr::Pdf));
        assert!(matches!(back.copy_method, CopyStr::Full));
        assert!(matches!(back.name_mode, NameStr::Custom));
        assert_eq!(back.custom_name, "Randy");
        assert!(back.start_enabled);
        assert_eq!(back.start_date, "2023-01-01");
        assert!(back.sort_by_count);
        assert!(back.show_activity_log);
    }

    #[test]
    fn unknown_or_missing_fields_use_defaults() {
        // `#[serde(default)]` means an empty object yields defaults, and the
        // password is never present in the schema.
        let back = Settings::from_json("{}");
        assert!(matches!(back.format, FormatStr::Html));
        assert!(!back.start_enabled);
    }
}
