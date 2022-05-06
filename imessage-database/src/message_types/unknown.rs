use std::fmt::Display;

use crate::Variant;

/// Apple Pay Requests
#[derive(Debug)]
pub enum Unknown {
    Unknown(i32),
}

impl Variant for Unknown {}

impl Display for Unknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unknown::Unknown(id) => write!(f, "Unkown message type: {}", id),
        }
    }
}
