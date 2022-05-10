use imessage_database::Message;
use rusqlite::Connection;

use crate::app::runtime::Config;

pub trait Exporter {
    fn new(config: &Config) -> Self;
    fn format_message(msg: &Message) -> String;
    fn format_attachment(msg: &Message) -> String;
    fn iter_messages(db: &Connection);
    fn write_to_file(file: &str, text: &str);
}
