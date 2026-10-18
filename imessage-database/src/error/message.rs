/*!
 Errors that can happen when parsing message data.
*/

use std::fmt::{Display, Formatter, Result};

use crate::error::plist::PlistParseError;

/// Errors that can happen when working with message table data
#[derive(Debug)]
pub enum MessageError {
    /// Error occurred when parsing a property list
    PlistParseError(PlistParseError),
    /// Timestamp value is invalid or out of range
    InvalidTimestamp(i64),
}

impl Display for MessageError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            MessageError::InvalidTimestamp(when) => {
                write!(fmt, "Timestamp is invalid: {when}")
            }
            MessageError::PlistParseError(why) => {
                write!(fmt, "Failed to parse property list: {why}")
            }
        }
    }
}

impl std::error::Error for MessageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MessageError::PlistParseError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<PlistParseError> for MessageError {
    fn from(err: PlistParseError) -> Self {
        MessageError::PlistParseError(err)
    }
}
