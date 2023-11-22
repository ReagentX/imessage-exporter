/*!
 Errors that can happen when parsing message data.
*/

use std::fmt::{Display, Formatter, Result};

use crate::error::{plist::PlistParseError, streamtyped::StreamTypedError};

/// Errors that can happen when working with message table data
#[derive(Debug)]
pub enum MessageError {
    MissingData,
    NoText,
    StreamTypedParseError(StreamTypedError),
    PlistParseError(PlistParseError),
    InvalidTimestamp(i64),
}

impl Display for MessageError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            MessageError::MissingData => write!(fmt, "No attributedBody found!"),
            MessageError::NoText => write!(fmt, "Message has no text!"),
            MessageError::StreamTypedParseError(why) => {
                write!(fmt, "Failed to parse attributedBody: {why}")
            }
            MessageError::PlistParseError(why) => {
                write!(fmt, "Failed to parse plist data: {why}")
            }
            MessageError::InvalidTimestamp(when) => {
                write!(fmt, "Timestamp is invalid: {when}")
            }
        }
    }
}
