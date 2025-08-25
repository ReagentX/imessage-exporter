/*!
 Errors that can happen when parsing attachment data.
*/

use std::{
    fmt::{Display, Formatter, Result},
    io::Error,
};

/// Errors that can happen when working with attachment table data
#[derive(Debug)]
pub enum AttachmentError {
    /// The attachment file exists but could not be read due to an IO error
    Unreadable(String, Error),
}

impl Display for AttachmentError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            AttachmentError::Unreadable(path, why) => {
                write!(fmt, "Unable to read file at {path}: {why}")
            }
        }
    }
}
