use rusqlite::{Row, Result};

pub trait Table {
    type A;
    fn from_row(row: &Row) -> Result<Self::A>;
}

pub const HANDLE: &str = "handle";
pub const MESSAGE: &str = "message";
