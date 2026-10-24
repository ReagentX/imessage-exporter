use std::{fmt::Display, path::PathBuf};

use crate::app::error::RuntimeError;

/// Attachment conversion and resolution failure.
#[derive(Debug)]
pub enum ConversionError {
    Io(std::io::Error),
    /// The attachment row's database `filename` column was NULL.
    ///
    /// `transfer_name` (the sender-supplied display name) may still be
    /// populated and is carried here so logs can identify the row.
    UnresolvedPath {
        transfer_name: Option<String>,
    },
    /// Decrypting an iOS backup file failed.
    DecryptFailed {
        path: PathBuf,
        source: RuntimeError,
    },
    /// The resolved attachment path doesn't exist on disk.
    NotFound {
        path: PathBuf,
    },
}

impl Display for ConversionError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionError::Io(e) => write!(fmt, "Attachment output failed: {e}"),
            ConversionError::UnresolvedPath {
                transfer_name: Some(name),
            } => {
                write!(
                    fmt,
                    "Attachment \"{name}\" has no path on disk (filename column is null)"
                )
            }
            ConversionError::UnresolvedPath {
                transfer_name: None,
            } => {
                write!(
                    fmt,
                    "Attachment row has no path on disk (filename column is null)"
                )
            }
            ConversionError::DecryptFailed { path, source } => {
                write!(fmt, "Unable to decrypt {}: {source}", path.display())
            }
            ConversionError::NotFound { path } => {
                write!(
                    fmt,
                    "Attachment not found at specified path: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConversionError::Io(e) => Some(e),
            ConversionError::DecryptFailed { source, .. } => Some(source),
            ConversionError::UnresolvedPath { .. } | ConversionError::NotFound { .. } => None,
        }
    }
}

impl From<std::io::Error> for ConversionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
