use rusqlite::{Row, Result, Connection, Statement};

pub trait Table {
    fn from_row(row: &Row) -> Result<Self> where Self: Sized;
    fn get(db: &Connection) -> Statement;
}

pub const HANDLE: &str = "handle";
pub const MESSAGE: &str = "message";
