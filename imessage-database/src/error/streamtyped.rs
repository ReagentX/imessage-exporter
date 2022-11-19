use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub enum StreamTypedError {
    CannotParse,
    NoStartPattern,
    NoEndPatter,
}

impl Display for StreamTypedError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            StreamTypedError::CannotParse => write!(fmt, "No attributedBody found!"),
            StreamTypedError::NoStartPattern => write!(fmt, "No start pattern found!"),
            StreamTypedError::NoEndPatter => write!(fmt, "No end pattern found!"),
        }
    }
}
