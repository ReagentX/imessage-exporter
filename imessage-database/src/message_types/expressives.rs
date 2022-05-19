/// Bubble effect variants
#[derive(Debug, PartialEq)]
pub enum BubbleEffect {
    Gentle,
    Impact,
    InvisibleInk,
    Loud,
}

/// Screen effect variants
#[derive(Debug, PartialEq)]
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

/// Expressive Container
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
#[derive(Debug, PartialEq)]
pub enum Expressive<'a> {
    Screen(ScreenEffect),
    Bubble(BubbleEffect),
    Unknown(&'a str),
    Normal,
}
