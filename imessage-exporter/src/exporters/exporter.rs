use std::path::Path;

use imessage_database::{
    error::{message::MessageError, plist::PlistParseError, table::TableError},
    message_types::{
        app::AppMessage, app_store::AppStoreMessage, collaboration::CollaborationMessage,
        handwriting::HandwrittenMessage, music::MusicMessage, placemark::PlacemarkMessage,
        url::URLMessage,
    },
    tables::{attachment::Attachment, messages::Message},
};

use crate::app::{error::RuntimeError, runtime::Config};

/// Defines behavior for iterating over messages from the iMessage database and managing export files
pub trait Exporter<'a> {
    /// Create a new exporter with references to the cached data
    fn new(config: &'a Config) -> Self;
    /// Begin iterating over the messages table
    fn iter_messages(&mut self) -> Result<(), RuntimeError>;
    /// Get the file handle to write to, otherwise create a new one
    fn get_or_create_file(&mut self, message: &Message) -> &Path;
}

/// Defines behavior for formatting message instances to the desired output format
pub(super) trait Writer<'a> {
    /// Format a message, including its reactions and replies
    fn format_message(&self, msg: &Message, indent: usize) -> Result<String, TableError>;
    /// Format an attachment, possibly by reading the disk
    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        msg: &'a Message,
    ) -> Result<String, &'a str>;
    /// Format a sticker, possibly by reading the disk
    fn format_sticker(&self, attachment: &'a mut Attachment, msg: &'a Message) -> String;
    /// Format an app message by parsing some of its fields
    fn format_app(
        &self,
        msg: &'a Message,
        attachments: &mut Vec<Attachment>,
        indent: &str,
    ) -> Result<String, PlistParseError>;
    /// Format a reaction (displayed under a message)
    fn format_reaction(&self, msg: &Message) -> Result<String, TableError>;
    /// Format an expressive message
    fn format_expressive(&self, msg: &'a Message) -> &'a str;
    /// Format an announcement message
    fn format_announcement(&self, msg: &'a Message) -> String;
    /// Format a `SharePlay` message
    fn format_shareplay(&self) -> &str;
    /// Format an edited message
    fn format_edited(&self, msg: &'a Message, indent: &str) -> Result<String, MessageError>;
    fn write_to_file(file: &Path, text: &str);
}

/// Defines behavior for formatting custom balloons to the desired output format
pub(super) trait BalloonFormatter<T> {
    /// Format a URL message
    fn format_url(&self, balloon: &URLMessage, indent: T) -> String;
    /// Format an Apple Music message
    fn format_music(&self, balloon: &MusicMessage, indent: T) -> String;
    /// Format a Rich Collaboration message
    fn format_collaboration(&self, balloon: &CollaborationMessage, indent: T) -> String;
    /// Format an App Store link
    fn format_app_store(&self, balloon: &AppStoreMessage, indent: T) -> String;
    /// Format a shared location message
    fn format_placemark(&self, balloon: &PlacemarkMessage, indent: T) -> String;
    /// Format a handwritten note message
    fn format_handwriting(&self, balloon: &HandwrittenMessage, indent: T) -> String;
    /// Format an Apple Pay message
    fn format_apple_pay(&self, balloon: &AppMessage, indent: T) -> String;
    /// Format a Fitness message
    fn format_fitness(&self, balloon: &AppMessage, indent: T) -> String;
    /// Format a Photo Slideshow message
    fn format_slideshow(&self, balloon: &AppMessage, indent: T) -> String;
    /// Format a Find My message
    fn format_find_my(&self, balloon: &AppMessage, indent: T) -> String;
    /// Format a Check In message
    fn format_check_in(&self, balloon: &AppMessage, indent: T) -> String;
    /// Format a generic app, generally third party
    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &mut Vec<Attachment>,
        indent: T,
    ) -> String;
}
