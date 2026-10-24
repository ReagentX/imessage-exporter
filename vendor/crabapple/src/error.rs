//! Custom error type for all iOS backup operations.

use std::{array::TryFromSliceError, fmt};

/// Custom error type for all iOS backup operations in this library.
///
/// Represents various failure modes encountered when reading,
/// parsing, or decrypting an iOS backup.
#[derive(Debug)]
pub enum BackupError {
    /// An underlying I/O error occurred.
    Io(std::io::Error),

    /// `SQLite` database error (e.g., opening or querying Manifest.db).
    Database(rusqlite::Error),

    /// The database was already closed
    DatabaseClosed,

    /// Cryptographic operation failed, with a descriptive message.
    Crypto(String),

    /// Conversion from slice to integer failed.
    ConversionFailed(TryFromSliceError),

    /// Attempted to use encryption features on an unencrypted backup.
    NotEncrypted,

    /// A password or derived key was required but not provided.
    PasswordOrKeyRequired,

    /// The password or derived key was incorrect.
    PasswordOrKeyIncorrect,

    /// `Manifest.plist` file was not found at the expected path.
    ManifestPlistNotFound(String),

    /// `Manifest.db` file was not found or could not be decrypted.
    ManifestDbNotFound,

    /// A requested file was not found in the backup catalog.
    FileNotFoundInBackup(String),

    /// The provided backup root path is invalid or not a directory.
    InvalidBackupRoot(String),

    /// No backup found for the given device UDID.
    DeviceNotFound(String),

    /// Failed to decode a hexadecimal string: descriptive message.
    HexDecode(String),

    /// UTF-8 conversion failed.
    Utf8(std::string::FromUtf8Error),

    /// A general, catch-all error with a descriptive message.
    General(String),

    /// A required key was missing in `Manifest.plist`.
    MissingPlistKey(String),

    /// Failed to unwrap a protected key for the given class.
    KeyUnwrapFailed(u32),

    /// Cryptographic data had an unexpected length.
    InvalidCryptoDataLength {
        /// The expected length of the data in bytes.
        expected: usize,
        /// The actual length of the data in bytes.
        actual: usize,
    },

    /// Invalid TLV data encountered.
    InvalidTlvData(String),

    /// Failed to parse data from a `plist`.
    PlistParseError(String),
}

/// Alias for a `Result` with this crate's `BackupError`.
pub type Result<T> = std::result::Result<T, BackupError>;

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::Io(err) => write!(f, "IO error: {err}"),
            BackupError::Database(err) => write!(f, "SQLite database error: {err}"),
            BackupError::DatabaseClosed => write!(f, "Manifest.db was already closed!"),
            BackupError::Crypto(msg) => write!(f, "Cryptography error: {msg}"),
            BackupError::ConversionFailed(why) => {
                write!(f, "Conversion failed: {why}")
            }
            BackupError::NotEncrypted => write!(f, "Backup is not encrypted"),
            BackupError::PasswordOrKeyRequired => write!(
                f,
                "Password or derived key is required for encrypted backups"
            ),
            BackupError::PasswordOrKeyIncorrect => {
                write!(f, "Password or derived key is incorrect")
            }
            BackupError::ManifestPlistNotFound(path) => {
                write!(f, "Manifest.plist not found at {path}")
            }
            BackupError::ManifestDbNotFound => {
                write!(f, "Manifest.db not found or could not be decrypted")
            }
            BackupError::FileNotFoundInBackup(path) => {
                write!(f, "File not found in backup: {path}")
            }
            BackupError::InvalidBackupRoot(path) => write!(f, "Invalid backup root: {path}"),
            BackupError::DeviceNotFound(udid) => write!(f, "Device not found: {udid}"),
            BackupError::HexDecode(msg) => write!(f, "Hex decoding error: {msg}"),
            BackupError::Utf8(err) => write!(f, "UTF-8 conversion error: {err}"),
            BackupError::General(msg) => write!(f, "General backup error: {msg}"),
            BackupError::MissingPlistKey(key) => {
                write!(f, "Missing required key in Manifest.plist: {key}")
            }
            BackupError::KeyUnwrapFailed(class_id) => {
                write!(f, "Key unwrapping failed for class {class_id}")
            }
            BackupError::InvalidCryptoDataLength { expected, actual } => write!(
                f,
                "Invalid data length for cryptographic operation: expected {expected}, got {actual}"
            ),
            BackupError::InvalidTlvData(msg) => write!(f, "Invalid TLV data: {msg}"),
            BackupError::PlistParseError(msg) => write!(f, "Plist parse error: {msg}"),
        }
    }
}

impl std::error::Error for BackupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackupError::Io(err) => Some(err),
            BackupError::Database(err) => Some(err),
            BackupError::Utf8(err) => Some(err),
            _ => None,
        }
    }
}

// Manual From implementations
impl From<std::io::Error> for BackupError {
    fn from(err: std::io::Error) -> Self {
        BackupError::Io(err)
    }
}

impl From<rusqlite::Error> for BackupError {
    fn from(err: rusqlite::Error) -> Self {
        BackupError::Database(err)
    }
}

impl From<std::string::FromUtf8Error> for BackupError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        BackupError::Utf8(err)
    }
}
