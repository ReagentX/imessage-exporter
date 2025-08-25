/*!
 [Expressives](https://support.apple.com/en-us/HT206894) are effects that you can select by tapping and holding the send button.
*/

/// Bubble effects are effects that alter the display of the chat bubble.
///
/// Read more [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad).
#[derive(Debug, PartialEq, Eq)]
pub enum BubbleEffect {
    /// Creates a slam effect that makes the bubble appear to slam down onto the screen.
    Slam,
    /// Creates a loud effect that makes the bubble appear to enlarge temporarily.
    Loud,
    /// Creates a gentle effect that makes the bubble appear to shrink temporarily.
    Gentle,
    /// Creates an invisible ink effect that hides the message until the recipient swipes over it.
    InvisibleInk,
}

/// Screen effects are effects that alter the entire background of the message view.
///
/// Read more [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad).
#[derive(Debug, PartialEq, Eq)]
pub enum ScreenEffect {
    /// Creates a confetti effect that sprinkles confetti across the screen.
    Confetti,
    /// Creates an echo effect that sends multiple copies of the message across the screen.
    Echo,
    /// Creates a fireworks effect that displays colorful explosions on the screen.
    Fireworks,
    /// Creates a balloons effect that sends balloons floating up from the bottom of the screen.
    Balloons,
    /// Creates a heart effect that displays a large heart on the screen.
    Heart,
    /// Creates a laser light show effect across the screen.
    Lasers,
    /// Creates a shooting star effect that moves across the screen.
    ShootingStar,
    /// Creates a sparkle effect that twinkles across the screen.
    Sparkles,
    /// Creates a spotlight effect that highlights the message.
    Spotlight,
}

/// Expressive effect container.
///
/// Read more about expressive messages [here](https://www.imore.com/how-to-use-bubble-and-screen-effects-imessage-iphone-ipad).
///
/// Bubble:
/// - `com.apple.MobileSMS.expressivesend.gentle`
/// - `com.apple.MobileSMS.expressivesend.impact`
/// - `com.apple.MobileSMS.expressivesend.invisibleink`
/// - `com.apple.MobileSMS.expressivesend.loud`
///
/// Screen:
/// - `com.apple.messages.effect.CKConfettiEffect`
/// - `com.apple.messages.effect.CKEchoEffect`
/// - `com.apple.messages.effect.CKFireworksEffect`
/// - `com.apple.messages.effect.CKHappyBirthdayEffect`
/// - `com.apple.messages.effect.CKHeartEffect`
/// - `com.apple.messages.effect.CKLasersEffect`
/// - `com.apple.messages.effect.CKShootingStarEffect`
/// - `com.apple.messages.effect.CKSparklesEffect`
/// - `com.apple.messages.effect.CKSpotlightEffect`
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
