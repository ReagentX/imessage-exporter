/// Reactions to iMessages
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
