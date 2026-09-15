/*!
 Filter category assigned to a message.
*/

use std::fmt::{Display, Formatter, Result};

/// Filter category stored in `message.filter_action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterAction {
    /// No filter category.
    Unfiltered,
    /// Allow category.
    Allow,
    /// Junk category.
    Junk,
    /// Promotion category.
    Promotion,
    /// Transaction category.
    Transaction,
    /// Unrecognized raw code.
    Unknown(i32),
}

impl FilterAction {
    /// Convert a raw `filter_action` code, preserving a missing value as `None`.
    #[must_use]
    pub fn from_code(code: Option<i32>) -> Option<Self> {
        Some(match code? {
            0 => Self::Unfiltered,
            1 => Self::Allow,
            2 => Self::Junk,
            3 => Self::Promotion,
            4 => Self::Transaction,
            other => Self::Unknown(other),
        })
    }

    /// `true` for junk, promotional, and transactional categories.
    #[must_use]
    pub fn is_filtered(&self) -> bool {
        matches!(self, Self::Junk | Self::Promotion | Self::Transaction)
    }
}

impl Display for FilterAction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            Self::Unknown(code) => write!(fmt, "Unknown ({code})"),
            _ => write!(fmt, "{self:?}"),
        }
    }
}
