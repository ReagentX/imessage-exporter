/*!
 Message classification helpers for rows from the `message` table.
*/

use std::fmt::Display;

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::{
        app_store::AppStoreMessage, collaboration::CollaborationMessage, music::MusicMessage,
        placemark::PlacemarkMessage, url::URLMessage,
    },
    tables::messages::models::GroupAction,
};

/// # Tapbacks
///
/// Tapbacks look like normal messages in the database. Only the latest tapback
/// state is stored. For example:
///
/// - user receives message -> user likes message
///   - This creates a message and a like message.
/// - user receives message -> user likes message -> user unlikes message
///   - This creates a message and a like message.
///   - The like message is removed when the unlike message arrives.
///   - Removed rows leave gaps in `ROWID`; the row ID is not reused.
///   - The database keeps the latest tapback state, not the full tapback history.
///
/// ## Technical detail
///
/// The index specified by the prefix identifies a parsed message body part;
/// see [`Message::components`](crate::tables::messages::Message::components).
///
/// - `bp:` GUID prefix for bubble message tapbacks (url previews, apps, etc).
/// - `p:0/` GUID prefix for normal messages (body text, attachments).
///
/// If a message has 3 attachments followed by some text:
/// - 0 is the first image
/// - 1 is the second image
/// - 2 is the third image
/// - 3 is the text of the message
///
/// In this example, a Like on `p:2/` is a like on the third image.
#[derive(Debug, PartialEq, Eq)]
pub enum Tapback<'a> {
    /// Heart
    Loved,
    /// Thumbs up
    Liked,
    /// Thumbs down
    Disliked,
    /// Laughing face
    Laughed,
    /// Exclamation points
    Emphasized,
    /// Question marks
    Questioned,
    /// Custom emoji tapbacks
    Emoji(Option<&'a str>),
    /// Custom sticker tapbacks
    Sticker,
}

impl Display for Tapback<'_> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tapback::Emoji(emoji) => match emoji {
                Some(e) => write!(fmt, "{e}"),
                None => write!(fmt, "Unknown emoji!"),
            },
            _ => write!(fmt, "{self:?}"),
        }
    }
}

/// iMessage app balloon kind.
///
/// App integrations use custom balloons instead of the normal text bubble. This
/// enum identifies the supported balloon families.
#[derive(Debug, PartialEq, Eq)]
pub enum CustomBalloon<'a> {
    /// Generic third-party [application](crate::message_types::app).
    Application(&'a str),
    /// [URL](crate::message_types::url) preview.
    URL,
    /// Handwritten animated message.
    Handwriting,
    /// Digital Touch message.
    DigitalTouch,
    /// Apple Pay message (one of Sent, Requested, Received)
    ApplePay,
    /// Fitness.app message.
    Fitness,
    /// Photos.app slideshow message.
    Slideshow,
    /// [Check In](https://support.apple.com/guide/iphone/use-check-in-iphc143bb7e9/ios) message.
    CheckIn,
    /// Find My message.
    FindMy,
    /// Poll message.
    Polls,
    /// Apple [Business Chat](crate::message_types::business_chat) message.
    Business,
}

/// Specialized payload carried by a URL balloon.
///
/// Apple reuses `com.apple.messages.URLBalloonProvider` for link previews and a
/// few richer payloads. This enum stores the parsed result.
#[derive(Debug, PartialEq)]
pub enum URLOverride<'a> {
    /// Standard [`URL`](crate::message_types::url) preview.
    Normal(URLMessage<'a>),
    /// [`Apple Music`](crate::message_types::music) message.
    AppleMusic(MusicMessage<'a>),
    /// [`App Store`](crate::message_types::app_store) message.
    AppStore(AppStoreMessage<'a>),
    /// [`Collaboration`](crate::message_types::collaboration) message.
    Collaboration(CollaborationMessage<'a>),
    /// [`Placemark`](crate::message_types::placemark) message.
    SharedPlacemark(PlacemarkMessage<'a>),
}

/// Non-balloon announcement represented by a message row.
///
/// Announcements cover thread-level events such as group changes and fully
/// unsent messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Announcement<'a> {
    /// All parts of the message were unsent.
    FullyUnsent,
    /// Group action.
    GroupAction(GroupAction<'a>),
    /// User kept an audio message.
    AudioMessageKept,
    /// Unmapped `item_type`.
    Unknown(&'a i32),
}

/// Whether a tapback was added or removed.
///
#[derive(Debug, PartialEq, Eq)]
pub enum TapbackAction {
    /// Tapback was added to the message.
    Added,
    /// Tapback was removed from the message.
    Removed,
}

/// High-level classification for a message row.
#[derive(Debug, PartialEq, Eq)]
pub enum Variant<'a> {
    /// Standard message body, possibly with attachments.
    Normal,
    /// Message with edited or unsent parts.
    Edited,
    /// A [tapback](https://support.apple.com/guide/messages/react-with-tapbacks-icht504f698a/mac)
    ///
    /// The `usize` is the body component index the tapback applies to.
    Tapback(usize, TapbackAction, Tapback<'a>),
    /// Message generated by an iMessage app integration.
    App(CustomBalloon<'a>),
    /// SharePlay message.
    SharePlay,
    /// Vote cast on a poll.
    Vote,
    /// New option sent to a poll.
    PollUpdate,
    /// Unmapped `item_type`.
    Unknown(i32),
}

/// Parser for custom balloon payloads stored in message plist data.
pub trait BalloonProvider<'a> {
    /// Parse the type from a plist payload.
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError>
    where
        Self: Sized;
}

/// URL fields shared by payloads that store both final and original URLs.
pub trait HasUrl {
    /// The URL that ended up serving content, after redirects.
    fn url(&self) -> Option<&str>;

    /// The original URL before redirects.
    fn original_url(&self) -> Option<&str>;

    /// Return the final URL, falling back to the original URL.
    #[must_use]
    fn get_url(&self) -> Option<&str> {
        self.url().or(self.original_url())
    }
}
