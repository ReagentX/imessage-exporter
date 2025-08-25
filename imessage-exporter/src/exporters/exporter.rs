use std::{borrow::Cow, fs::File, io::BufWriter, marker::Sized};

use imessage_database::{
    error::{plist::PlistParseError, table::TableError},
    message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        digital_touch::DigitalTouch,
        edited::EditedMessage,
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        text_effects::{Animation, Style, TextEffect, Unit},
        url::URLMessage,
    },
    tables::{
        attachment::Attachment,
        messages::{
            Message,
            models::{AttachmentMeta, TextAttributes},
        },
    },
};

use crate::app::{error::RuntimeError, runtime::Config};

pub(crate) const ATTACHMENT_NO_FILENAME: &str = "Attachment missing name metadata!";

/// Defines behavior for iterating over messages from the iMessage database and managing export files
pub trait Exporter<'a> {
    /// Create a new exporter with references to the cached data
    fn new(config: &'a Config) -> Result<Self, RuntimeError>
    where
        Self: Sized;
    /// Begin iterating over the messages table
    fn iter_messages(&mut self) -> Result<(), RuntimeError>;
    /// Get the file handle to write to, otherwise create a new one
    fn get_or_create_file(
        &mut self,
        message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError>;
}

/// Defines behavior for formatting message instances to the desired output format
pub(super) trait Writer<'a> {
    /// Format a message, including its tapbacks and replies
    fn format_message(&self, msg: &Message, indent: usize) -> Result<String, TableError>;
    /// Format an attachment, possibly by reading the disk
    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        msg: &'a Message,
        metadata: &AttachmentMeta,
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
    /// Format a tapback (displayed under a message)
    fn format_tapback(&self, msg: &Message) -> Result<String, TableError>;
    /// Format an expressive message
    fn format_expressive(&self, msg: &'a Message) -> &'a str;
    /// Format an announcement message
    fn format_announcement(&self, msg: &'a Message) -> String;
    /// Format a `SharePlay` message
    fn format_shareplay(&self) -> &str;
    /// Format a legacy Shared Location message
    fn format_shared_location(&self, msg: &'a Message) -> &str;
    /// Format an edited message
    fn format_edited(
        &self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        indent: &str,
    ) -> Option<String>;
    /// Format all [`TextAttributes`]s applied to a given set of text
    fn format_attributes(&'a self, text: &'a str, attributes: &'a [TextAttributes]) -> String;
    fn write_to_file(file: &mut BufWriter<File>, text: &str) -> Result<(), RuntimeError>;
}

/// Defines behavior for formatting custom balloons to the desired output format
pub(super) trait BalloonFormatter<T> {
    /// Format a URL message
    fn format_url(&self, msg: &Message, balloon: &URLMessage, indent: T) -> String;
    /// Format an Apple Music message
    fn format_music(&self, balloon: &MusicMessage, indent: T) -> String;
    /// Format a Rich Collaboration message
    fn format_collaboration(&self, balloon: &CollaborationMessage, indent: T) -> String;
    /// Format an App Store link
    fn format_app_store(&self, balloon: &AppStoreMessage, indent: T) -> String;
    /// Format a shared location message
    fn format_placemark(&self, balloon: &PlacemarkMessage, indent: T) -> String;
    /// Format a handwritten note message
    fn format_handwriting(&self, msg: &Message, balloon: &HandwrittenMessage, indent: T) -> String;
    /// Format a digital touch message
    fn format_digital_touch(&self, msg: &Message, balloon: &DigitalTouch, indent: T) -> String;
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

/// Defines behavior for applying a [`TextEffect`] to the desired output format
pub(super) trait TextEffectFormatter<'a> {
    /// Format a specific [`TextEffect`]
    fn format_effect(&'a self, text: &'a str, effect: &'a TextEffect) -> Cow<'a, str>;
    /// Format message text containing a [`Mention`](imessage_database::message_types::text_effects::TextEffect::Mention)
    fn format_mention(&self, text: &str, mentioned: &str) -> String;
    /// Format message text containing a [`Link`](imessage_database::message_types::text_effects::TextEffect::Link)
    fn format_link(&self, text: &str, url: &str) -> String;
    /// Format message text containing an [`OTP`](imessage_database::message_types::text_effects::TextEffect::OTP)
    fn format_otp(&self, text: &str) -> String;
    /// Format message text containing a [`Conversion`](imessage_database::message_types::text_effects::TextEffect::Conversion)
    fn format_conversion(&self, text: &str, unit: &Unit) -> String;
    /// Format message text containing some [`Styles`](imessage_database::message_types::text_effects::TextEffect::Styles)
    fn format_styles(&self, text: &str, styles: &[Style]) -> String;
    /// Format [`Animated`](imessage_database::message_types::text_effects::TextEffect::Animated) message text
    fn format_animated(&self, text: &str, animation: &Animation) -> String;
}
