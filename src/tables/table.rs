use rusqlite::{Row, Result};

pub trait Table {
    fn from_row(row: &Row) -> Result<Self> where Self: Sized;
}

pub const HANDLE: &str = "handle";
pub const MESSAGE: &str = "message";
