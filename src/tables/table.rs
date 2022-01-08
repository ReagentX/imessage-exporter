use rusqlite::{Connection, Result, Row, Statement};

pub trait Table {
    fn from_row(row: &Row) -> Result<Self>
    where
        Self: Sized;
    fn get(db: &Connection) -> Statement;
}

pub const HANDLE: &str = "handle";
pub const MESSAGE: &str = "message";
pub const CHAT: &str = "chat";
pub const CHAT_MESSAGE_JOIN: &str = "chat_message_join";
pub const CHAT_HANDLE_JOIN: &str = "chat_handle_join";
pub const ME: &str = "Me";
