/*!
Errors that can happen during the application's runtime
*/

use std::fmt::{Display, Formatter, Result};

use imessage_database::error::table::TableError;

/// Errors that can happen during the application's runtime
#[derive(Debug)]
pub enum RuntimeError {
    InvalidOptions,
    DatabaseError(TableError),
}

impl Display for RuntimeError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            RuntimeError::InvalidOptions => write!(fmt, "Invalid options!"),
            RuntimeError::DatabaseError(why) => write!(fmt, "{why}"),
        }
    }
}
