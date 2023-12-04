/*!
 Variants represent the different types of iMessages that exist in the `messages` table.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::{
        app_store::AppStoreMessage, collaboration::CollaborationMessage, music::MusicMessage,
        placemark::PlacemarkMessage, url::URLMessage,
    },
};

/// Reactions to iMessages
///
/// `bp:` GUID prefix for bubble message reactions (links, apps, etc).
///
/// `p:0/` GUID prefix for normal messages (body text, attachments).
///
/// In `p:#/`, the # represents the message index. If a message has 3 attachments:
/// - 0 is the first image
/// - 1 is the second image
/// - 2 is the third image
/// - 3 is the text of the message
///
/// In this example, a Like on `p:2/` is a like on the third image
///
/// Reactions are normal messages in the database, but only the latest reaction
/// is stored. For example:
/// - user receives message -> user likes message
///   - This will create a message and a like message
/// - user receives message -> user likes message -> user unlikes message
///   - This will create a message and a like message
///   - but that like message will get dropped when the unlike message arrives
///   - When messages drop the ROWIDs become non-sequential: the ID of the dropped message row is not reused
///   - This means unliking an old message will make it look like the reaction was applied/removed at the
///   time of latest change; the history of reaction statuses is not kept
#[derive(Debug)]
pub enum Reaction {
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
    /// Apple Pay (one of Sent, Requested, Received)
    ApplePay,
    /// Fitness.app messages
    Fitness,
    /// Photos.app slideshow messages
    Slideshow,
    /// [Check In](https://support.apple.com/guide/iphone/use-check-in-iphc143bb7e9/ios) messages
    CheckIn,
    /// Find My messages
    FindMy
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
    /// Someone changed the name of the group
    NameChange(&'a str),
    /// Someone updated the group photo
    PhotoChange,
    /// Types that may occur in the future, i.e. someone leaving or joining a group
    Unknown(&'a i32),
}

/// Message variant container
///
/// Messages can exist as one of many different variants, this encapsulates
/// all of the possibilities.
#[derive(Debug)]
pub enum Variant<'a> {
    /// A reaction to another message
    Reaction(usize, bool, Reaction),
    /// A sticker message, either placed on another message or by itself
    Sticker(usize),
    /// Container for new or unknown messages
    Unknown(i32),
    /// An [iMessage app](https://support.apple.com/en-us/HT206906) generated message
    App(CustomBalloon<'a>),
    /// An iMessage with a standard text body that may include attachments
    Normal,
    /// A message that has been edited or unsent
    Edited,
    /// A SharePlay message
    SharePlay,
}

/// Defines behavior for different types of messages that have custom balloons
pub trait BalloonProvider<'a> {
    /// Creates the object from a `HashMap` of item attributes
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError>
    where
        Self: Sized;
}
