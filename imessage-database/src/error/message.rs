/*!
 Errors that can happen when parsing message data.
*/

use std::fmt::{Display, Formatter, Result};

use crabstep::error::TypedStreamError;

use crate::error::streamtyped::StreamTypedError;

/// Errors that can happen when working with message table data
#[derive(Debug)]
pub enum MessageError {
    /// Message has no text content
    NoText,
    /// Error occurred when parsing with the `StreamTyped` parser
    StreamTypedParseError(StreamTypedError),
    /// Error occurred when deserializing a `typedstream`
    TypedStreamError(TypedStreamError),
    /// Timestamp value is invalid or out of range
    InvalidTimestamp(i64),
}

impl Display for MessageError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            MessageError::NoText => write!(fmt, "Message has no text!"),
            MessageError::StreamTypedParseError(why) => {
                write!(
                    fmt,
                    "Failed to parse attributedBody with legacy parser: {why}"
                )
            }
            MessageError::InvalidTimestamp(when) => {
                write!(fmt, "Timestamp is invalid: {when}")
            }
            MessageError::TypedStreamError(typed_stream_error) => {
                write!(
                    fmt,
                    "Failed to deserialize typed stream: {typed_stream_error}"
                )
            }
        }
    }
}

impl From<StreamTypedError> for MessageError {
    fn from(err: StreamTypedError) -> Self {
        MessageError::StreamTypedParseError(err)
    }
}

impl From<TypedStreamError> for MessageError {
    fn from(err: TypedStreamError) -> Self {
        MessageError::TypedStreamError(err)
    }
}
