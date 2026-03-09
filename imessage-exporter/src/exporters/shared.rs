use imessage_database::{
    message_types::expressives::{BubbleEffect, Expressive, ScreenEffect},
    tables::messages::Message,
    util::dates::format,
};

use crate::app::runtime::Config;

// MARK: Expressive

/// Format the expressive send style for a message. This is identical
/// across all export formats.
pub(super) fn format_expressive(msg: &Message) -> &str {
    match msg.get_expressive() {
        Expressive::Screen(effect) => match effect {
            ScreenEffect::Confetti => "Sent with Confetti",
            ScreenEffect::Echo => "Sent with Echo",
            ScreenEffect::Fireworks => "Sent with Fireworks",
            ScreenEffect::Balloons => "Sent with Balloons",
            ScreenEffect::Heart => "Sent with Heart",
            ScreenEffect::Lasers => "Sent with Lasers",
            ScreenEffect::ShootingStar => "Sent with Shooting Star",
            ScreenEffect::Sparkles => "Sent with Sparkles",
            ScreenEffect::Spotlight => "Sent with Spotlight",
        },
        Expressive::Bubble(effect) => match effect {
            BubbleEffect::Slam => "Sent with Slam",
            BubbleEffect::Loud => "Sent with Loud",
            BubbleEffect::Gentle => "Sent with Gentle",
            BubbleEffect::InvisibleInk => "Sent with Invisible Ink",
        },
        Expressive::Unknown(effect) => effect,
        Expressive::None => "",
    }
}

// MARK: Time

/// Compute the formatted timestamp and read receipt for a message.
/// Returns `(formatted_date, read_receipt)` where `read_receipt` is
/// empty if there is no read receipt data.
pub(super) fn message_time(config: &Config, message: &Message) -> (String, String) {
    let date = format(&message.date(&config.offset));
    let mut read_receipt = String::new();
    if let Some(time) = message.time_until_read(&config.offset)
        && !time.is_empty()
    {
        let who = if message.is_from_me() {
            "them"
        } else {
            config.options.custom_name.as_deref().unwrap_or("you")
        };
        read_receipt = format!("(Read by {who} after {time})");
    }
    (date, read_receipt)
}
