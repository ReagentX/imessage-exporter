/*!
 Errors that can happen when parsing `streamtyped` data
*/

use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub enum StreamTypedError {
    NoStartPattern,
    NoEndPattern,
    InvalidPrefix,
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
