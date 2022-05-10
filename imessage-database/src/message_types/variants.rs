/// Reactions to iMessages
/// `bp:` GUID prefix for bubble message reactions (links, apps, etc)
/// `p:0/` GUID prefix for normal messages (text, attachment)
/// for `p:#/`, the # is the message index, so if a message has 3 attachments: 
/// - 0 is the first image
/// - 1 is the second image
/// - 2 is the third image
/// - 3 is the text of the message
/// So, a Like on `p:2/` is a like on the second message
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

#[derive(Debug)]
pub enum Variant {
    ApplePay(ApplePay),
    Reaction(usize, bool, Reaction),
    Unknown(i32),
    Normal,
}
