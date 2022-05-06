use crate::{
    message_types::{applepay::ApplePay, reactions::Reaction},
    Message, Unknown,
};

pub fn get_variant(msg: &Message) -> Option<Box<dyn Variant>> {
    match msg.associated_message_type {
        0 => None,
        2 => Some(Box::new(ApplePay::Send(0))),
        3 => Some(Box::new(ApplePay::Recieve(0))),
        2000 => Some(Box::new(Reaction::Loved(true))),
        2001 => Some(Box::new(Reaction::Liked(true))),
        2002 => Some(Box::new(Reaction::Disliked(true))),
        2003 => Some(Box::new(Reaction::Laughed(true))),
        2004 => Some(Box::new(Reaction::Emphasized(true))),
        2005 => Some(Box::new(Reaction::Questioned(true))),
        3000 => Some(Box::new(Reaction::Loved(false))),
        3001 => Some(Box::new(Reaction::Liked(false))),
        3002 => Some(Box::new(Reaction::Disliked(false))),
        3003 => Some(Box::new(Reaction::Laughed(false))),
        3004 => Some(Box::new(Reaction::Emphasized(false))),
        3005 => Some(Box::new(Reaction::Questioned(false))),
        x => Some(Box::new(Unknown::Unknown(x))),
    }
}

pub trait Variant: std::fmt::Display + std::fmt::Debug {}
