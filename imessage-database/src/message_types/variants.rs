/*!
 Variants represent the different types of iMessages that exist in the `messages` table.
*/

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

/// Apple Pay Requests
#[derive(Debug)]
pub enum ApplePay {
    Request(String), // Unused currently since this uses associated_message_type == 0
    Send(String),
    Recieve(String),
}

/// Variant container
#[derive(Debug)]
pub enum Variant {
    ApplePay(ApplePay),
    Reaction(usize, bool, Reaction),
    Unknown(i32),
    Normal,
}
