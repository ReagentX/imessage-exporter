use std::fmt::Display;

pub enum ApplePay {
    Request, // Unused currently since this uses associated_message_type == 0
    Send,
    Recieve,
}

impl Display for ApplePay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplePay::Request => write!(f, "Requested Apple Pay"),
            ApplePay::Send => write!(f, "Sent Apple Pay"),
            ApplePay::Recieve => write!(f, "Recieved Apple Pay"),
        }
    }
}
