use std::collections::HashMap;

use imessage_database::{
    message_types::expressives::Expressive,
    rusqlite::Connection,
    tables::{attachment::Attachment, capabilities::Capabilities, messages::Message},
};

use crate::app::error::RuntimeError;

/// Per-message data resolved before iterating over the parts to avoid repeated
/// DB queries and `Expressive::None` filtering. Own the loaded `attachments`
/// here and borrow them while rendering each part. Use `get_mut` on
/// `replies_map` entries to render replies recursively.
pub(crate) struct MessageContext<'a> {
    pub attachments: Vec<Attachment>,
    pub replies_map: HashMap<usize, Vec<Message>>,
    pub expressive: Option<Expressive<'a>>,
}

impl<'a> MessageContext<'a> {
    pub fn resolve(
        message: &'a Message,
        db: &Connection,
        capabilities: &Capabilities,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            attachments: Attachment::from_message(db, message, capabilities)?,
            replies_map: message.get_replies(db, capabilities)?,
            expressive: match message.get_expressive() {
                Expressive::None | Expressive::Unknown("") => None,
                other => Some(other),
            },
        })
    }
}
