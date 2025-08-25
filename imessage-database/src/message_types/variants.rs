/*!
 Variants represent the different types of iMessages that exist in the `messages` table.
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
/// [Tapbacks](https://support.apple.com/guide/messages/react-with-tapbacks-icht504f698a/mac) look like normal messages in the database. Only the latest tapback
/// is stored. For example:
///
/// - user receives message -> user likes message
///   - This will create a message and a like message
/// - user receives message -> user likes message -> user unlikes message
///   - This will create a message and a like message
///   - but that like message will get dropped when the unlike message arrives
///   - When messages drop the ROWIDs become non-sequential: the ID of the dropped message row is not reused
///   - This means unliking an old message will make it look like the tapback was applied/removed at the
///     time of latest change; the history of tapback statuses is not kept
///
/// ## Technical detail
///
/// The index specified by the prefix maps to the index of the body part given by [`Message::body()`](crate::tables::messages::Message#method.body).
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
#[derive(Debug)]
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

/// Application Messages
///
/// Messages sent via an app's iMessage integration will send in a special balloon instead of a normal
/// text balloon. This represents the different variants of message balloon.
#[derive(Debug)]
pub enum CustomBalloon<'a> {
    /// Generic third party [applications](crate::message_types::app)
    Application(&'a str),
    /// [URL](crate::message_types::url) previews
    URL,
    /// Handwritten animated messages
    Handwriting,
    /// Digital Touch message
    DigitalTouch,
    /// Apple Pay (one of Sent, Requested, Received)
    ApplePay,
    /// Fitness.app messages
    Fitness,
    /// Photos.app slideshow messages
    Slideshow,
    /// [Check In](https://support.apple.com/guide/iphone/use-check-in-iphc143bb7e9/ios) messages
    CheckIn,
    /// Find My messages
    FindMy,
}

/// URL Message Types
///
/// Apple sometimes overloads `com.apple.messages.URLBalloonProvider` with
/// other types of messages; this enum represents those variants.
#[derive(Debug)]
pub enum URLOverride<'a> {
    /// [`URL`](crate::message_types::url) previews
    Normal(URLMessage<'a>),
    /// [`Apple Music`](crate::message_types::music) messages
    AppleMusic(MusicMessage<'a>),
    /// [`App Store`](crate::message_types::app_store) messages
    AppStore(AppStoreMessage<'a>),
    /// [`Collaboration`](crate::message_types::collaboration) messages
    Collaboration(CollaborationMessage<'a>),
    /// [`Placemark`](crate::message_types::placemark) messages
    SharedPlacemark(PlacemarkMessage<'a>),
}

/// Announcement Message Types
///
/// Announcements are messages sent to a thread for actions that are not balloons, i.e.
/// updating the name of the group or changing the group photo
#[derive(Debug)]
pub enum Announcement<'a> {
    /// All parts of the message were unsent
    FullyUnsent,
    /// A group action
    GroupAction(GroupAction<'a>),
    /// A user kept an audio message
    AudioMessageKept,
    /// Types that may occur in the future
    Unknown(&'a i32),
}

/// Tapback Action Container
///
/// Tapbacks can either be added or removed; this enum represents those states
#[derive(Debug)]
pub enum TapbackAction {
    /// Tapback was added to the message
    Added,
    /// Tapback was removed from the message
    Removed,
}

/// Message variant container
///
/// Messages can exist as one of many different variants, this encapsulates
/// all of the possibilities.
#[derive(Debug)]
pub enum Variant<'a> {
    /// An iMessage with a standard text body that may include attachments
    Normal,
    /// A message that has been [edited](crate::message_types::edited::EditStatus::Edited) or [unsent](crate::message_types::edited::EditStatus::Unsent)
    Edited,
    /// A [tapback](https://support.apple.com/guide/messages/react-with-tapbacks-icht504f698a/mac)
    ///
    /// The `usize` indicates the index of the message's [`body()`](crate::tables::messages::Message#method.body) the tapback is applied to.
    Tapback(usize, TapbackAction, Tapback<'a>),
    /// An [iMessage app](https://support.apple.com/en-us/HT206906) generated message
    App(CustomBalloon<'a>),
    /// A `SharePlay` message
    SharePlay,
    /// Container for new or unknown messages
    Unknown(i32),
}

/// Defines behavior for different types of messages that have custom balloons
pub trait BalloonProvider<'a> {
    /// Creates the object from a `HashMap` of item attributes
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError>
    where
        Self: Sized;
}
