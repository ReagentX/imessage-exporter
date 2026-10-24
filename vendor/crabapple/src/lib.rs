#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod backup;
pub mod error;

/// Main library for interacting with iOS backups, providing types and methods for decryption and metadata access.
pub use backup::Backup;

/// Authentication options for encrypted backups: either a password or a pre-derived key.
pub use backup::models::auth::Authentication;
