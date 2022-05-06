use std::fmt::Display;

use crate::Variant;

/// Apple Pay Requests
#[derive(Debug)]
pub enum ApplePay {
    Request(String), // Unused currently since this uses associated_message_type == 0
    Send(String),
    Recieve(String),
}

impl Variant for ApplePay {}

impl Display for ApplePay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplePay::Request(amount) => write!(f, "{amount}"),
            ApplePay::Send(amount) => write!(f, "{amount}"),
            ApplePay::Recieve(amount) => write!(f, "{amount}"),
        }
    }
}
