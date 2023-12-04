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
    FileNotFound(String),
    Unreadable(String, Error),
}

impl Display for AttachmentError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            AttachmentError::FileNotFound(path) => {
                write!(fmt, "File not found at location: {path}")
            }
            AttachmentError::Unreadable(path, why) => {
                write!(fmt, "Unable to read file at {path}: {why}")
            }
        }
    }
}
