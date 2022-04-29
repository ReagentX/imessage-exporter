use std::{collections::HashMap, fmt::Display};

enum Reaction {
    Loved,
    Liked,
    Disliked,
    Laughed,
    Emphasized,
    Questioned,
}

impl Display for Reaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reaction::Loved => write!(f, "Loved"),
            Reaction::Liked => write!(f, "Liked"),
            Reaction::Disliked => write!(f, "Disliked"),
            Reaction::Laughed => write!(f, "Laughed"),
            Reaction::Emphasized => write!(f, "Emphasized"),
            Reaction::Questioned => write!(f, "Questioned"),
        }
    }
}

pub fn get_types_table() -> HashMap<i32, Box<dyn Display + 'static>> {
    let mut types: HashMap<i32, Box<dyn Display + 'static>> = HashMap::new();
    // Reactions:
    types.insert(2000, Box::new(Reaction::Loved));
    types.insert(2001, Box::new(Reaction::Liked));
    types.insert(2002, Box::new(Reaction::Disliked));
    types.insert(2003, Box::new(Reaction::Laughed));
    types.insert(2004, Box::new(Reaction::Emphasized));
    types.insert(2005, Box::new(Reaction::Questioned));
    types
}
