/*!
 Errors that can happen when parsing query context data.
*/

use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing query context data
#[derive(Debug)]
pub enum QueryContextError {
    InvalidDate(String),
    InvalidTelephone(String),
}

impl Display for QueryContextError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            QueryContextError::InvalidDate(date) => write!(
                fmt,
                "Invalid date provided: {date}! Must be in format YYYY-MM-DD."
            ),
            QueryContextError::InvalidTelephone(telephone) => write!(
                fmt,
                "Invalid telephone provided: {telephone}! Must be in format XXXXXXXXXXX, e.g., 16468885555"
            ),
        }
    }
}
