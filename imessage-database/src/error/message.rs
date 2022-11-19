use std::fmt::{Display, Formatter, Result};

use super::streamtyped::StreamTypedError;

#[derive(Debug)]
pub enum MessageError {
    MissingData,
    NoText,
    ParseError(StreamTypedError),
}

impl Display for MessageError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            MessageError::MissingData => write!(fmt, "No attributedBody found!"),
            MessageError::NoText => write!(fmt, "Message has no text!"),
            MessageError::ParseError(why) => write!(fmt, "Failed to parse attributedBody: {why}"),
        }
    }
}
