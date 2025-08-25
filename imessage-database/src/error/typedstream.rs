/*!
 Errors that can happen when parsing `typedstream` data. This module is for the new `typedstream` deserializer.
*/

use std::{
    array::TryFromSliceError,
    fmt::{Display, Formatter, Result},
    str::Utf8Error,
};

/// Errors that can happen when parsing `typedstream` data
#[derive(Debug)]
pub enum TypedStreamError {
    /// Indicates an attempt to access data beyond the bounds of the buffer.
    /// The first parameter is the attempted index, second is the buffer length
    OutOfBounds(usize, usize),
    /// Indicates that the typedstream header is invalid or corrupted
    InvalidHeader,
    /// Error that occurs when trying to convert a slice
    SliceError(TryFromSliceError),
    /// Error that occurs when parsing a UTF-8 string
    StringParseError(Utf8Error),
    /// Indicates that an array could not be properly parsed
    InvalidArray,
    /// Indicates that a pointer could not be parsed, with the invalid byte value
    InvalidPointer(usize),
}

impl Display for TypedStreamError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            TypedStreamError::OutOfBounds(idx, len) => {
                write!(fmt, "Index {idx:x} is outside of range {len:x}!")
            }
            TypedStreamError::InvalidHeader => write!(fmt, "Invalid typedstream header!"),
            TypedStreamError::SliceError(why) => {
                write!(fmt, "Unable to slice source stream: {why}")
            }
            TypedStreamError::StringParseError(why) => write!(fmt, "Failed to parse string: {why}"),
            TypedStreamError::InvalidArray => write!(fmt, "Failed to parse array data"),
            TypedStreamError::InvalidPointer(why) => write!(fmt, "Failed to parse pointer: {why}"),
        }
    }
}
