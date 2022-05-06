use imessage_database::Message;
use rusqlite::Connection;

pub trait Exporter {
    fn format_message(msg: &Message) -> String;
    fn format_attachment(msg: &Message) -> String;
    fn iter_messages(db: &Connection);
    fn write_to_file(file: &str, text: &str);
}
