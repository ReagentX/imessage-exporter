/// Reactions to iMessages
/// `bp:` GUID prefix for bubble message reactions (links, apps, etc)
/// `p:0/` GUID prefix for normal messages (text, attachment)
#[derive(Debug)]
pub enum Reaction {
    Loved(bool),
    Liked(bool),
    Disliked(bool),
    Laughed(bool),
    Emphasized(bool),
    Questioned(bool),
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
    Reaction(Reaction),
    Unknown(i32),
    Normal,
}
