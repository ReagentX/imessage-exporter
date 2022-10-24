use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing the plist data stored in the `payload_data` field
#[derive(Debug)]
pub enum PlistParseError<'a> {
    MissingKey(&'a str),
    NoValueAtIndex(usize),
    InvalidType(&'a str, &'a str),
    InvalidTypeIndex(usize, &'a str),
    InvalidDictionarySize(usize, usize),
}

impl<'a> Display for PlistParseError<'a> {
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
        }
    }
}
