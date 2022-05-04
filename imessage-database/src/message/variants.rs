use std::{collections::HashMap, fmt::Display};

use crate::message::{reactions::Reaction, applepay::ApplePay};

pub fn get_types_table() -> HashMap<i32, Box<dyn Display + 'static>> {
    let mut types: HashMap<i32, Box<dyn Display + 'static>> = HashMap::new();
    // Normal message is 0
    // Sticker:
    types.insert(1000, Box::new("sticker placeholder"));
    // Apple Pay
    types.insert(2, Box::new(ApplePay::Send));
    types.insert(3, Box::new(ApplePay::Recieve));

    // Reactions
    types.insert(2000, Box::new(Reaction::Loved(true)));
    types.insert(2001, Box::new(Reaction::Liked(true)));
    types.insert(2002, Box::new(Reaction::Disliked(true)));
    types.insert(2003, Box::new(Reaction::Laughed(true)));
    types.insert(2004, Box::new(Reaction::Emphasized(true)));
    types.insert(2005, Box::new(Reaction::Questioned(true)));

    // Negative reactions
    types.insert(3000, Box::new(Reaction::Loved(false)));
    types.insert(3001, Box::new(Reaction::Liked(false)));
    types.insert(3002, Box::new(Reaction::Disliked(false)));
    types.insert(3003, Box::new(Reaction::Laughed(false)));
    types.insert(3004, Box::new(Reaction::Emphasized(false)));
    types.insert(3005, Box::new(Reaction::Questioned(false)));
    types
}
