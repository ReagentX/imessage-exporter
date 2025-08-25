/*!
 This module contains Data structures and models that represent message data.
*/

use std::fmt::{Display, Formatter, Result};

use crabstep::{PropertyIterator, deserializer::iter::Property};

use crate::{
    message_types::text_effects::TextEffect,
    tables::messages::message::Message,
    util::typedstream::{as_float, as_nsstring},
};

/// Defines the parts of a message bubble, i.e. the content that can exist in a single message.
///
/// # Component Types
///
/// A single iMessage contains data that may be represented across multiple bubbles.
#[derive(Debug, PartialEq, Clone)]
pub enum BubbleComponent {
    /// A text message with associated formatting, generally representing ranges present in a `NSAttributedString`
    Text(Vec<TextAttributes>),
    /// An attachment
    Attachment(AttachmentMeta),
    /// An [app integration](crate::message_types::app)
    App,
    /// A component that was retracted, found by parsing the [`EditedMessage`](crate::message_types::edited::EditedMessage)
    Retracted,
}

/// Defines different types of [services](https://support.apple.com/en-us/104972) we can receive messages from.
#[derive(Debug)]
pub enum Service<'a> {
    /// An iMessage
    #[allow(non_camel_case_types)]
    iMessage,
    /// A message sent as SMS
    SMS,
    /// A message sent as RCS
    RCS,
    /// A message sent via [satellite](https://support.apple.com/en-us/120930)
    Satellite,
    /// Any other type of message
    Other(&'a str),
    /// Used when service field is not set
    Unknown,
}

impl<'a> Service<'a> {
    /// Creates a [`Service`] enum variant based on the provided service name string.
    #[must_use]
    pub fn from(service: Option<&'a str>) -> Self {
        if let Some(service_name) = service {
            return match service_name.trim() {
                "iMessage" => Service::iMessage,
                "iMessageLite" => Service::Satellite,
                "SMS" => Service::SMS,
                "rcs" | "RCS" => Service::RCS,
                service_name => Service::Other(service_name),
            };
        }
        Service::Unknown
    }
}

impl Display for Service<'_> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            Service::iMessage => write!(fmt, "iMessage"),
            Service::SMS => write!(fmt, "SMS"),
            Service::RCS => write!(fmt, "RCS"),
            Service::Satellite => write!(fmt, "Satellite"),
            Service::Other(other) => write!(fmt, "{other}"),
            Service::Unknown => write!(fmt, "Unknown"),
        }
    }
}

/// Defines ranges of text and associated attributes parsed from [`typedstream`](crate::util::typedstream) `attributedBody` data.
///
/// Ranges specify locations where attributes are applied to specific portions of a [`Message`]'s [`text`](crate::tables::messages::Message::text). For example, given message text with a [`Mention`](TextEffect::Mention) like:
///
/// ```
/// let message_text = "What's up, Christopher?";
/// ```
///
/// There will be 3 ranges:
///
/// ```
/// use imessage_database::message_types::text_effects::TextEffect;
/// use imessage_database::tables::messages::models::{TextAttributes, BubbleComponent};
///  
/// let result = vec![BubbleComponent::Text(vec![
///     TextAttributes::new(0, 11, vec![TextEffect::Default]),  // `What's up, `
///     TextAttributes::new(11, 22, vec![TextEffect::Mention("+5558675309".to_string())]), // `Christopher`
///     TextAttributes::new(22, 23, vec![TextEffect::Default])  // `?`
/// ])];
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TextAttributes {
    /// The start index of the affected range of message text
    pub start: usize,
    /// The end index of the affected range of message text
    pub end: usize,
    /// The effects applied to the specified range
    pub effects: Vec<TextEffect>,
}

impl TextAttributes {
    /// Creates a new [`TextAttributes`] with the specified start index, end index, and text effects.
    #[must_use]
    pub fn new(start: usize, end: usize, effects: Vec<TextEffect>) -> Self {
        Self {
            start,
            end,
            effects,
        }
    }
}

/// Representation of attachment metadata used for rendering message body in a conversation feed.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct AttachmentMeta {
    /// GUID of the attachment in the `attachment` table
    pub guid: Option<String>,
    /// The transcription, if the attachment was an [audio message](https://support.apple.com/guide/iphone/send-and-receive-audio-messages-iph2e42d3117/ios) sent from or received on a [supported platform](https://www.apple.com/ios/feature-availability/#messages-audio-message-transcription).
    pub transcription: Option<String>,
    /// The height of the attachment in points
    pub height: Option<f64>,
    /// The width of the attachment in points
    pub width: Option<f64>,
    /// The attachment's original filename
    pub name: Option<String>,
}

impl AttachmentMeta {
    /// Populates the attachment metadata fields from a typedstream property iterator.
    #[must_use]
    pub(crate) fn from_components<'a>(
        first_key: &str,
        components: &'a mut PropertyIterator<'a, 'a>,
    ) -> Self {
        let mut meta = Self::default();

        if let Some(mut prop) = components.next() {
            meta.set_from_key_value(first_key, &mut prop);
        }

        while let Some(mut key) = components.next() {
            if let Some(key_name) = as_nsstring(&mut key) {
                if let Some(mut value) = components.next() {
                    meta.set_from_key_value(key_name, &mut value);
                }
            }
        }

        meta
    }

    fn set_from_key_value<'a>(&'a mut self, key: &'a str, value: &'a mut Property<'a, 'a>) {
        match key {
            "__kIMFileTransferGUIDAttributeName" => {
                self.guid = as_nsstring(value).map(String::from);
            }
            "IMAudioTranscription" => self.transcription = as_nsstring(value).map(String::from),
            "__kIMInlineMediaHeightAttributeName" => self.height = as_float(value),
            "__kIMInlineMediaWidthAttributeName" => self.width = as_float(value),
            "__kIMFilenameAttributeName" => self.name = as_nsstring(value).map(String::from),
            _ => {}
        }
    }
}

/// Represents different types of group message actions that can occur in a chat system
#[derive(Debug)]
pub enum GroupAction<'a> {
    /// A new participant has been added to the group
    ParticipantAdded(i32),
    /// A participant has been removed from the group
    ParticipantRemoved(i32),
    /// The group name has been changed
    NameChange(&'a str),
    /// A participant has voluntarily left the group
    ParticipantLeft,
    /// The group icon/avatar has been updated with a new image
    GroupIconChanged,
    /// The group icon/avatar has been removed, reverting to default
    GroupIconRemoved,
}

impl<'a> GroupAction<'a> {
    /// Creates a new `GroupAction` event type based on the provided message's item and group action data.
    #[must_use]
    pub(crate) fn from_message(message: &'a Message) -> Option<Self> {
        match (
            message.item_type,
            message.group_action_type,
            message.other_handle,
            &message.group_title,
        ) {
            (1, 0, Some(who), _) => Some(Self::ParticipantAdded(who)),
            (1, 1, Some(who), _) => Some(Self::ParticipantRemoved(who)),
            (2, _, _, Some(name)) => Some(Self::NameChange(name)),
            (3, 0, _, _) => Some(Self::ParticipantLeft),
            (3, 1, _, _) => Some(Self::GroupIconChanged),
            (3, 2, _, _) => Some(Self::GroupIconRemoved),
            _ => None,
        }
    }
}
