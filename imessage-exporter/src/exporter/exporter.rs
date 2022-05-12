use imessage_database::{Message, Attachment};

use crate::app::runtime::Config;

pub trait Exporter<'a> {
    /// Create a new exporter with references to the cached data
    fn new(config: &'a Config) -> Self;
    /// Begin iterating over the messages table
    fn iter_messages(&self);
    /// Get the file handle to write to, otherwise create a new one
    fn get_or_create_file(&self) -> String;
}

pub(super) trait Writer<'a> {
    fn format_message(&self, msg: &Message, indent: usize) -> String;
    // fn format_reply(&self, msg: &Message) -> String;
    fn format_attachment(&self, msg: &'a Attachment) -> Result<&'a str, &'a str>;
    fn format_app(&self, msg: &'a Message) -> &'a str;
    fn format_reaction(&self, msg: &Message) -> String;
    fn write_to_file(&self, file: &str, text: &str);
}
