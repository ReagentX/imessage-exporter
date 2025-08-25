/*!
 Effects that can alter the appearance of message text.
*/

/// Text effect container
///
/// Message text may contain any number of traditional styles or one animation.
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TextEffect {
    /// Default, unstyled text
    Default,
    /// A [mentioned](https://support.apple.com/guide/messages/mention-a-person-icht306ee34b/mac) contact in the conversation
    ///
    /// The embedded data contains information about the mentioned contact.
    Mention(String),
    /// A clickable link, i.e. `https://`, `tel:`, `mailto:`, and others
    ///
    /// The embedded data contains the url.
    Link(String),
    /// A one-time code, i.e. from a 2FA message
    OTP,
    /// Traditional formatting styles
    ///
    /// The embedded data contains the formatting styles applied to the range.
    Styles(Vec<Style>),
    /// Animation applied to the text
    ///
    /// The embedded data contains the animation applied to the range.
    Animated(Animation),
    /// Conversions that can be applied to text
    ///
    /// The embedded data contains the unit that the range represents.
    Conversion(Unit),
}

/// Unit conversion text effect container
///
/// Read more about unit conversions [here](https://www.macrumors.com/how-to/convert-currencies-temperatures-more-ios-16/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Unit {
    /// Currency conversion
    Currency,
    /// Distance conversion
    Distance,
    /// Temperature conversion
    Temperature,
    /// Timezone conversion
    Timezone,
    /// Volume conversion
    Volume,
    /// Weight conversion
    Weight,
}

/// Traditional text effect container
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Style {
    /// **Bold** styled text
    Bold,
    /// *Italic* styled text
    Italic,
    /// ~~Strikethrough~~ styled text
    Strikethrough,
    /// <u>Underline</u> styled text
    Underline,
}

/// Animated text effect container
///
/// A message's [`typedstream`](crate::util::typedstream) contains an [`i64`] identifier under the key `__kIMTextEffectAttributeName`.
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Animation {
    /// Denoted by an ID of `5`
    Big,
    /// Denoted by an ID of `11`
    Small,
    /// Denoted by an ID of `9`
    Shake,
    /// Denoted by an ID of `8`
    Nod,
    /// Denoted by an ID of `12`
    Explode,
    /// Denoted by an ID of `4`
    Ripple,
    /// Denoted by an ID of `6`
    Bloom,
    /// Denoted by an ID of `10`
    Jitter,
    /// A new identifier not currently supported
    Unknown(i64),
}

impl Animation {
    /// Get the animation from its ID given in a message's [`typedstream`](crate::util::typedstream) data, under the `__kIMTextEffectAttributeName` key.
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::message_types::text_effects::Animation;
    ///
    /// let animation = Animation::from_id(5); // Animation::Big
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
