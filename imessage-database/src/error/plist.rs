/*!
 Errors that can happen when parsing plist data.
*/

use crabstep::error::TypedStreamError;

use crate::error::handwriting::HandwritingError;
use crate::error::streamtyped::StreamTypedError;
use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing the plist data stored in the `payload_data` field
#[derive(Debug)]
pub enum PlistParseError {
    /// Expected key was not found in the plist data
    MissingKey(String),
    /// No value was found at the specified index
    NoValueAtIndex(usize),
    /// Value had an incorrect type for the specified key
    InvalidType(String, String),
    /// Value had an incorrect type at the specified index
    InvalidTypeIndex(usize, String),
    /// Dictionary has mismatched number of keys and values
    InvalidDictionarySize(usize, usize),
    /// No payload data was found
    NoPayload,
    /// Message is not of the expected type
    WrongMessageType,
    /// Could not parse an edited message
    InvalidEditedMessage(String),
    /// Error from stream typed parsing
    StreamTypedError(StreamTypedError),
    /// Error from typedstream parsing
    TypedStreamError(TypedStreamError),
    /// Error from handwriting data parsing
    HandwritingError(HandwritingError),
    /// Error parsing Digital Touch message
    DigitalTouchError,
}

impl Display for PlistParseError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            PlistParseError::MissingKey(key) => write!(fmt, "Expected key {key}, found nothing!"),
            PlistParseError::NoValueAtIndex(idx) => {
                write!(fmt, "Payload referenced index {idx}, but there is no data!")
            }
            PlistParseError::InvalidType(key, value) => {
                write!(fmt, "Invalid data found at {key}, expected {value}")
            }
            PlistParseError::InvalidTypeIndex(idx, value) => {
                write!(
                    fmt,
                    "Invalid data found at object index {idx}, expected {value}"
                )
            }
            PlistParseError::InvalidDictionarySize(a, b) => write!(
                fmt,
                "Invalid dictionary size, found {a} keys and {b} values"
            ),
            PlistParseError::NoPayload => write!(fmt, "Unable to acquire payload data!"),
            PlistParseError::WrongMessageType => write!(fmt, "Message is not an app message!"),
            PlistParseError::InvalidEditedMessage(message) => {
                write!(fmt, "Unable to parse message from binary data: {message}")
            }
            PlistParseError::StreamTypedError(why) => write!(fmt, "{why}"),
            PlistParseError::HandwritingError(why) => write!(fmt, "{why}"),
            PlistParseError::DigitalTouchError => {
                write!(fmt, "Unable to parse Digital Touch Message!")
            }
            PlistParseError::TypedStreamError(typed_stream_error) => {
                write!(fmt, "TypedStream error: {typed_stream_error}")
            }
        }
    }
}

impl From<TypedStreamError> for PlistParseError {
    fn from(error: TypedStreamError) -> Self {
        PlistParseError::TypedStreamError(error)
    }
}

impl From<StreamTypedError> for PlistParseError {
    fn from(error: StreamTypedError) -> Self {
        PlistParseError::StreamTypedError(error)
    }
}
