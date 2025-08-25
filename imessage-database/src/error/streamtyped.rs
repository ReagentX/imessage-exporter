/*!
 Errors that can happen when parsing `typedstream` data. This module is for the legacy simple `typedstream` parser.
*/

use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing `typedstream` data
#[derive(Debug)]
pub enum StreamTypedError {
    /// Error when the expected start pattern is not found
    NoStartPattern,
    /// Error when the expected end pattern is not found
    NoEndPattern,
    /// Error when the prefix length does not match the standard
    InvalidPrefix,
    /// Error when the timestamp cannot be parsed as a valid integer
    InvalidTimestamp,
}

impl Display for StreamTypedError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            StreamTypedError::NoStartPattern => write!(fmt, "No start pattern found!"),
            StreamTypedError::NoEndPattern => write!(fmt, "No end pattern found!"),
            StreamTypedError::InvalidPrefix => write!(fmt, "Prefix length is not standard!"),
            StreamTypedError::InvalidTimestamp => write!(fmt, "Timestamp integer is not valid!"),
        }
    }
}
