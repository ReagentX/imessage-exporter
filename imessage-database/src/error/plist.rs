/*!
 Errors that can happen when parsing plist data.
*/

use crabstep::error::TypedStreamError;

use crate::error::digital_touch::DigitalTouchError;
use crate::error::handwriting::HandwritingError;
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
    /// UID value cannot be represented as an object-table index on this target.
    UidOutOfRange(u64),
    /// No payload data was found
    NoPayload,
    /// Message is not of the expected type
    WrongMessageType,
    /// Could not parse an edited message
    InvalidEditedMessage(String),
    /// Error from typedstream parsing
    TypedStreamError(TypedStreamError),
    /// Error from handwriting data parsing
    HandwritingError(HandwritingError),
    /// Error from Digital Touch data parsing
    DigitalTouchError(DigitalTouchError),
    /// Error parsing a poll message
    PollError,
    /// Exceeded the maximum UID-reference resolution depth (likely a reference cycle)
    RecursionLimit,
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
            PlistParseError::UidOutOfRange(uid) => {
                write!(fmt, "UID value {uid} cannot be used as an object index")
            }
            PlistParseError::NoPayload => write!(fmt, "Unable to acquire payload data!"),
            PlistParseError::WrongMessageType => write!(fmt, "Message is not an app message!"),
            PlistParseError::InvalidEditedMessage(message) => {
                write!(fmt, "Unable to parse message from binary data: {message}")
            }
            PlistParseError::HandwritingError(why) => write!(fmt, "{why}"),
            PlistParseError::DigitalTouchError(why) => write!(fmt, "{why}"),
            PlistParseError::TypedStreamError(typed_stream_error) => {
                write!(fmt, "TypedStream error: {typed_stream_error}")
            }
            PlistParseError::PollError => write!(fmt, "Unable to parse Poll Message!"),
            PlistParseError::RecursionLimit => write!(
                fmt,
                "Exceeded maximum depth while resolving UID references; the archive may contain a reference cycle"
            ),
        }
    }
}

impl std::error::Error for PlistParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PlistParseError::TypedStreamError(e) => Some(e),
            PlistParseError::HandwritingError(e) => Some(e),
            PlistParseError::DigitalTouchError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TypedStreamError> for PlistParseError {
    fn from(error: TypedStreamError) -> Self {
        PlistParseError::TypedStreamError(error)
    }
}
