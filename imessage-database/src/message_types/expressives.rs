/*!
 [Expressives](https://support.apple.com/en-us/HT206894) are effects that you can select by tapping and holding the send button.
*/

/// Bubble effects are effects that alter the display of the chat bubble.
///
/// Read more [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad).
#[derive(Debug, PartialEq, Eq)]
pub enum BubbleEffect {
    Slam,
    Loud,
    Gentle,
    InvisibleInk,
}

/// Screen effects are effects that alter the entire background of the message view.
///
/// Read more [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad).
#[derive(Debug, PartialEq, Eq)]
pub enum ScreenEffect {
    Confetti,
    Echo,
    Fireworks,
    Balloons,
    Heart,
    Lasers,
    ShootingStar,
    Sparkles,
    Spotlight,
}

/// Expressive effect container
///
/// Read more about expressive messages [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad)
///
/// Bubble:
/// - com.apple.MobileSMS.expressivesend.gentle
/// - com.apple.MobileSMS.expressivesend.impact
/// - com.apple.MobileSMS.expressivesend.invisibleink
/// - com.apple.MobileSMS.expressivesend.loud
///
/// Screen:
/// - com.apple.messages.effect.CKConfettiEffect
/// - com.apple.messages.effect.CKEchoEffect
/// - com.apple.messages.effect.CKFireworksEffect
/// - com.apple.messages.effect.CKHappyBirthdayEffect
/// - com.apple.messages.effect.CKHeartEffect
/// - com.apple.messages.effect.CKLasersEffect
/// - com.apple.messages.effect.CKShootingStarEffect
/// - com.apple.messages.effect.CKSparklesEffect
/// - com.apple.messages.effect.CKSpotlightEffect
#[derive(Debug, PartialEq, Eq)]
pub enum Expressive<'a> {
    /// Effects that use the entire screen
    Screen(ScreenEffect),
    /// Effects that display on a single bubble
    Bubble(BubbleEffect),
    /// Container for new or unknown messages
    Unknown(&'a str),
    /// Message is not an expressive
    None,
}
