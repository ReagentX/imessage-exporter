#![forbid(unsafe_code)]
#![doc = include_str!("../../docs/library/README.md")]

pub mod message_types;
pub mod tables;
pub mod util;

pub use {
    message_types::{
        expressives::{BubbleEffect, Expressive, ScreenEffect},
        variants::{ApplePay, Reaction, Variant},
    },
    tables::{
        attachment::Attachment, chat::Chat, chat_handle::ChatToHandle, handle::Handle,
        messages::Message, table::Table,
    },
};
