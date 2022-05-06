use std::fmt::Display;

use crate::Variant;

/// Apple Pay Requests
#[derive(Debug, Clone, Copy)]
pub enum ApplePay {
    Request(i32), // Unused currently since this uses associated_message_type == 0
    Send(i32),
    Recieve(i32),
}

impl Variant for ApplePay {}

impl Display for ApplePay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplePay::Request(amount) => write!(f, "Requested Apple Pay"),
            ApplePay::Send(amount) => write!(f, "Sent Apple Pay"),
            ApplePay::Recieve(amount) => write!(f, "Recieved Apple Pay"),
        }
    }
}
