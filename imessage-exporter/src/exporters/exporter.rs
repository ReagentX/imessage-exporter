use std::path::Path;

use imessage_database::{Attachment, Message};

use crate::app::runtime::Config;

pub trait Exporter<'a> {
    /// Create a new exporter with references to the cached data
    fn new(config: &'a Config) -> Self;
    /// Begin iterating over the messages table
    fn iter_messages(&mut self);
    /// Get the file handle to write to, otherwise create a new one
    fn get_or_create_file(&mut self, message: &Message) -> &Path;
}

pub(super) trait Writer<'a> {
    /// Format a message, including its reactions and replies
    fn format_message(&self, msg: &Message, indent: usize) -> String;
    /// Format an attachment, possibly by reading the disk
    fn format_attachment(&self, msg: &'a mut Attachment) -> Result<String, &'a str>;
    /// Format an app message by parsing some of its fields
    fn format_app(&self, msg: &'a Message) -> &'a str;
    /// Format a reaction (displayed under a message)
    fn format_reaction(&self, msg: &Message) -> String;
    /// Format an expressive message
    fn format_expressive(&self, msg: &'a Message) -> &'a str;
    /// Format an annoucement message
    fn format_annoucement(&self, msg: &'a Message) -> String;
    fn write_to_file(file: &Path, text: &str);
}
