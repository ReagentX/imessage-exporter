/*!
 Service a message was sent over.
*/

use std::fmt::{Display, Formatter, Result};

/// Defines different types of [services](https://support.apple.com/en-us/104972) we can receive messages from.
#[derive(Debug, PartialEq, Eq)]
pub enum Service<'a> {
    /// iMessage.
    #[allow(non_camel_case_types)]
    iMessage,
    /// SMS.
    SMS,
    /// RCS.
    RCS,
    /// A message sent via [satellite](https://support.apple.com/en-us/120930) (literally: `iMessageLite` in the database).
    Satellite,
    /// Unrecognized service name.
    Other(&'a str),
    /// Missing service field.
    Unknown,
}

impl<'a> Service<'a> {
    /// Map the database service name to a [`Service`] variant.
    #[must_use]
    pub fn from_name(service: Option<&'a str>) -> Self {
        if let Some(service_name) = service {
            return match service_name.trim() {
                "iMessage" => Service::iMessage,
                "iMessageLite" => Service::Satellite,
                "SMS" => Service::SMS,
                "rcs" | "RCS" => Service::RCS,
                service_name => Service::Other(service_name),
            };
        }
        Service::Unknown
    }
}

impl Display for Service<'_> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            Service::iMessage => write!(fmt, "iMessage"),
            Service::SMS => write!(fmt, "SMS"),
            Service::RCS => write!(fmt, "RCS"),
            Service::Satellite => write!(fmt, "Satellite"),
            Service::Other(other) => write!(fmt, "{other}"),
            Service::Unknown => write!(fmt, "Unknown"),
        }
    }
}
