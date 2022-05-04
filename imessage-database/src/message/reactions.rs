use std::fmt::Display;

/// Reactions to iMessages
pub enum Reaction {
    Loved(bool),
    Liked(bool),
    Disliked(bool),
    Laughed(bool),
    Emphasized(bool),
    Questioned(bool),
}

impl Display for Reaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Heart
            Reaction::Loved(true) => write!(f, "Loved"),
            Reaction::Loved(false) => write!(f, "Removed love"),
            // Thumbs Up
            Reaction::Liked(true) => write!(f, "Liked"),
            Reaction::Liked(false) => write!(f, "Removed like"),
            // Thumbs Down
            Reaction::Disliked(true) => write!(f, "Disliked"),
            Reaction::Disliked(false) => write!(f, "Removed Dislike"),
            // Haha
            Reaction::Laughed(true) => write!(f, "Laughed"),
            Reaction::Laughed(false) => write!(f, "Removed laugh"),
            // !
            Reaction::Emphasized(true) => write!(f, "Emphasized"),
            Reaction::Emphasized(false) => write!(f, "Removed emphasis"),
            // ?
            Reaction::Questioned(true) => write!(f, "Questioned"),
            Reaction::Questioned(false) => write!(f, "Removed Question"),
        }
    }
}
