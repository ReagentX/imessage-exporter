use std::path::Path;

use imessage_database::{
    error::plist::PlistParseError,
    message_types::{app::AppMessage, music::MusicMessage, url::URLMessage},
    Attachment, Message,
};

use crate::app::runtime::Config;

/// Defines behavior for iterating over messages from the iMessage database and managing export files
pub trait Exporter<'a> {
    /// Create a new exporter with references to the cached data
    fn new(config: &'a Config) -> Self;
    /// Begin iterating over the messages table
    fn iter_messages(&mut self) -> Result<(), String>;
    /// Get the file handle to write to, otherwise create a new one
    fn get_or_create_file(&mut self, message: &Message) -> &Path;
}

/// Defines behavior for formatting message instances to the desired output format
pub(super) trait Writer<'a> {
    /// Format a message, including its reactions and replies
    fn format_message(&self, msg: &Message, indent: usize) -> Result<String, String>;
    /// Format an attachment, possibly by reading the disk
    fn format_attachment(&self, msg: &'a mut Attachment) -> Result<String, &'a str>;
    /// Format an app message by parsing some of its fields
    fn format_app(
        &self,
        msg: &'a Message,
        attachments: &mut Vec<Attachment>,
    ) -> Result<String, PlistParseError>;
    /// Format a reaction (displayed under a message)
    fn format_reaction(&self, msg: &Message) -> Result<String, String>;
    /// Format an expressive message
    fn format_expressive(&self, msg: &'a Message) -> &'a str;
    /// Format an annoucement message
    fn format_annoucement(&self, msg: &'a Message) -> String;
    /// Format an edited message
    fn format_edited(&self, msg: &'a Message) -> Result<String, PlistParseError>;
    fn write_to_file(file: &Path, text: &str);
}

/// Defines behavior for formatting custom balloons to the desired output format
pub(super) trait BalloonFormatter {
    /// Format a URL message
    fn format_url(&self, balloon: &URLMessage) -> String;
    /// Format an Apple Music message
    fn format_music(&self, balloon: &MusicMessage) -> String;
    /// Format a handwritten note message
    fn format_handwriting(&self, balloon: &AppMessage) -> String;
    /// Format an Apple Pay message
    fn format_apple_pay(&self, balloon: &AppMessage) -> String;
    /// Format a Fitness message
    fn format_fitness(&self, balloon: &AppMessage) -> String;
    /// Format a Photo Slideshow message
    fn format_slideshow(&self, balloon: &AppMessage) -> String;
    /// Format a generic app, generally third party
    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &mut Vec<Attachment>,
    ) -> String;
}
