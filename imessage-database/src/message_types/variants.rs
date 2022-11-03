/*!
 Variants represent the different types of iMessages that exist in the `messages` table.
*/

use plist::Value;

use crate::error::plist::PlistParseError;

/// Reactions to iMessages
///
/// `bp:` GUID prefix for bubble message reactions (links, apps, etc)
/// `p:0/` GUID prefix for normal messages (body text, attachments)
/// for `p:#/`, the # is the message index, so if a message has 3 attachments:
/// - 0 is the first image
/// - 1 is the second image
/// - 2 is the third image
/// - 3 is the text of the message
/// In this example, a Like on `p:2/` is a like on the second message
///
/// Reactions are normal messages in the database, but only the latest reaction
/// is stored. For example:
/// - user recieves message -> user likes message
///   - This will create a message and a like message
/// - user recieves message -> user likes message -> user unlikes message
///   - This will create a message and a like message
///   - but that like message will get dropped when the unlike message arrives
///   - When messages drop the ROWIDs become non-sequential: the ID of the dropped message row is not reused
///   - This means unliking an old message will make it look like the reaction was applied/removed at the
///   - time of latest change; the history of reaction statuses is not kept
#[derive(Debug)]
pub enum Reaction {
    Loved,
    Liked,
    Disliked,
    Laughed,
    Emphasized,
    Questioned,
}

#[derive(Debug)]
pub enum CustomBalloon<'a> {
    /// Generic third party applications
    Application(&'a str),
    /// URL previews
    URL,
    /// Handwritten animated messages
    Handwriting,
    /// Apple Pay (one of Sent, Requested, Received)
    ApplePay,
    /// Fitness.app messages
    Fitness,
    /// Photos.app slideshow messages
    Slideshow,
}

/// Variant container
#[derive(Debug)]
pub enum Variant<'a> {
    Reaction(usize, bool, Reaction),
    Sticker(usize),
    Unknown(i32),
    App(CustomBalloon<'a>),
    Normal,
}

/// Defines behavior for different types of messages that have custom balloons
pub trait BalloonProvider<'a> {
    /// Creates the object from a HashMap of item attributes
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError>
    where
        Self: Sized;
}
