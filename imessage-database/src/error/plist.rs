/*!
 Errors that can happen when parsing plist data
*/

use std::fmt::{Display, Formatter, Result};

use crate::error::streamtyped::StreamTypedError;

/// Errors that can happen when parsing the plist data stored in the `payload_data` field
#[derive(Debug)]
pub enum PlistParseError {
    MissingKey(String),
    NoValueAtIndex(usize),
    InvalidType(String, String),
    InvalidTypeIndex(usize, String),
    InvalidDictionarySize(usize, usize),
    NoPayload,
    WrongMessageType,
    InvalidEditedMessage(String),
    StreamTypedError(StreamTypedError),
}

impl Display for PlistParseError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            PlistParseError::MissingKey(key) => write!(fmt, "Expected key {}, found nothing!", key),
            PlistParseError::NoValueAtIndex(idx) => write!(
                fmt,
                "Payload referenced index {}, but there is no data!",
                idx
            ),
            PlistParseError::InvalidType(key, value) => {
                write!(fmt, "Invalid data found at {}, expected {}", key, value)
            }
            PlistParseError::InvalidTypeIndex(idx, value) => {
                write!(
                    fmt,
                    "Invalid data found at object index {}, expected {}",
                    idx, value
                )
            }
            PlistParseError::InvalidDictionarySize(a, b) => write!(
                fmt,
                "Invalid dictionary size, found {} keys and {} values",
                a, b
            ),
            PlistParseError::NoPayload => write!(fmt, "Unable to acquire payload data!"),
            PlistParseError::WrongMessageType => write!(fmt, "Message is not an app message!"),
            PlistParseError::InvalidEditedMessage(message) => {
                write!(fmt, "Unable to parse message from binary data: {message}")
            }
            PlistParseError::StreamTypedError(why) => write!(fmt, "{why}"),
        }
    }
}
