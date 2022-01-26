use std::collections::HashMap;

use rusqlite::{Connection, OpenFlags, Result, Row, Statement};

pub trait Table {
    fn from_row(row: &Row) -> Result<Self>
    where
        Self: Sized;
    fn get(db: &Connection) -> Statement;
}

pub trait Cacheable {
    type T;
    fn cache(db: &Connection) -> HashMap<i32, Self::T>;
}

pub trait Diagnostic {
    fn run_diagnostic(db: &Connection);
}

pub fn get_connection(path: &str) -> Connection {
    match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(res) => res,
        Err(why) => panic!("Unable to read from chat database: {}\nEnsure full disk access is enabled for your terminal emulator in System Preferences > Security and Privacy > Full Disk Access", why),
    }
}

// Table Names
pub const HANDLE: &str = "handle";
pub const MESSAGE: &str = "message";
pub const CHAT: &str = "chat";
pub const ATTACHMENT: &str = "attachment";
pub const CHAT_MESSAGE_JOIN: &str = "chat_message_join";
pub const MESSAGE_ATTACHMENT_JOIN: &str = "message_attachment_join";
pub const CHAT_HANDLE_JOIN: &str = "chat_handle_join";

// CLI Arg Names
pub const OPTION_PATH: &str = "db-path";
pub const OPTION_COPY: &str = "no-copy";
pub const OPTION_DIAGNOSTIC: &str = "diagnostics";

// Default information
pub const ME: &str = "Me";
pub const DEFAULT_PATH: &str = "Library/Messages/chat.db";
