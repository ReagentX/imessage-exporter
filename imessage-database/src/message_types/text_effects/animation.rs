/// Animation applied to a message text range.
///
/// A message's [`typedstream`](https://github.com/ReagentX/crabstep) contains an [`i64`] identifier under the key `__kIMTextEffectAttributeName`.
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Animation {
    /// Identifier `5`.
    Big,
    /// Identifier `11`.
    Small,
    /// Identifier `9`.
    Shake,
    /// Identifier `8`.
    Nod,
    /// Identifier `12`.
    Explode,
    /// Identifier `4`.
    Ripple,
    /// Identifier `6`.
    Bloom,
    /// Identifier `10`.
    Jitter,
    /// Identifier not mapped by this crate.
    Unknown(i64),
}

impl Animation {
    /// Map the `__kIMTextEffectAttributeName` integer to an animation.
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::message_types::text_effects::animation::Animation;
    ///
    /// assert_eq!(Animation::from_id(5), Animation::Big);
    /// assert_eq!(Animation::from_id(42), Animation::Unknown(42));
    /// ```
    #[must_use]
    pub fn from_id(value: i64) -> Self {
        match value {
            // In order of appearance in the text effects menu
            5 => Self::Big,
            11 => Self::Small,
            9 => Self::Shake,
            8 => Self::Nod,
            12 => Self::Explode,
            4 => Self::Ripple,
            6 => Self::Bloom,
            10 => Self::Jitter,
            _ => Self::Unknown(value),
        }
    }
}
