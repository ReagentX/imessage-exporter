use imessage_database::{
    error::plist::PlistParseError,
    message_types::{
        app::{AppMessage, CheckInKind},
        business_chat::BusinessMessage,
        digital_touch::DigitalTouchMessage,
        handwriting::HandwrittenMessage,
        url::URLMessage,
        variants::{BalloonProvider, CustomBalloon, URLOverride, Variant},
    },
    tables::{
        attachment::Attachment,
        messages::Message,
        table::{FITNESS_RECEIVER, YOU},
    },
    util::{bundle_id::parse_balloon_bundle_id, dates::format, plist::parse_ns_keyed_archiver},
};

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::formatter::BalloonFormatter,
};

// MARK: Dispatch

/// Drive the App-balloon decision tree: pick the right payload source
/// (raw vs keyed-archiver), parse it, and dispatch to the matching
/// [`BalloonFormatter`] method.
pub fn dispatch_app_balloon<F: BalloonFormatter>(
    formatter: &F,
    message: &Message,
    attachments: &mut Vec<Attachment>,
    config: &Config,
) -> Result<String, RuntimeError> {
    // First, determine if is a balloon message; if it is not, bail out early
    let Variant::App(balloon) = message.variant() else {
        return Err(PlistParseError::WrongMessageType.into());
    };

    // Handwritten messages use a different payload type
    if message.is_handwriting()
        && let Some(payload) = message.raw_payload_data(config.data_source.db())
    {
        return match HandwrittenMessage::from_payload(&payload) {
            Ok(bubble) => Ok(formatter.format_handwriting(message, &bubble)),
            Err(why) => Err(PlistParseError::HandwritingError(why).into()),
        };
    }

    // Digital touch messages use a different payload type
    if message.is_digital_touch()
        && let Some(payload) = message.raw_payload_data(config.data_source.db())
    {
        return match DigitalTouchMessage::from_payload(&payload) {
            Ok(bubble) => Ok(formatter.format_digital_touch(message, &bubble)),
            Err(why) => Err(PlistParseError::DigitalTouchError(why).into()),
        };
    }

    // Poll messages use a different payload type
    if message.is_poll() {
        let poll = message.as_poll(config.data_source.db())?;
        return match poll {
            Some(poll) => Ok(formatter.format_poll(&poll)),
            None => Err(PlistParseError::PollError.into()),
        };
    }

    // Otherwise, we expect an NSKeyedArchiver payload
    let Some(payload) = message.payload_data(config.data_source.db()) else {
        // URL messages may omit the NSKeyedArchiver payload; in that case
        // re-render via the normal URL path with an empty balloon.
        if message.is_url() && message.text.is_some() {
            return Ok(formatter.format_url(message, &URLMessage::default()));
        }
        return Err(PlistParseError::NoPayload.into());
    };

    let parsed = parse_ns_keyed_archiver(&payload)?;

    let rendered = if message.is_url() {
        let bubble = URLMessage::get_url_message_override(&parsed)?;
        match bubble {
            URLOverride::Normal(b) => formatter.format_url(message, &b),
            URLOverride::AppleMusic(b) => formatter.format_music(&b),
            URLOverride::Collaboration(b) => formatter.format_collaboration(&b),
            URLOverride::AppStore(b) => formatter.format_app_store(&b),
            URLOverride::SharedPlacemark(b) => formatter.format_placemark(&b),
        }
    } else {
        {
            let bubble = AppMessage::from_map(&parsed)?;
            match balloon {
                CustomBalloon::Application(bundle_id) => {
                    formatter.format_generic_app(&bubble, bundle_id, attachments, message)
                }
                CustomBalloon::ApplePay => formatter.format_apple_pay(&bubble),
                CustomBalloon::Fitness => formatter.format_fitness(&bubble),
                CustomBalloon::Slideshow => formatter.format_slideshow(&bubble),
                CustomBalloon::CheckIn => formatter.format_check_in(&bubble),
                CustomBalloon::FindMy => formatter.format_find_my(&bubble),
                CustomBalloon::Business => match BusinessMessage::from_map(&parsed) {
                    Ok(business) => formatter.format_business(&business),
                    // Older business payloads use the same bundle ID but do
                    // not carry a supported interactive schema. Preserve the
                    // generic app-card fallback for those rows.
                    Err(_) => {
                        let bundle_id =
                            parse_balloon_bundle_id(message.balloon_bundle_id.as_deref())
                                .unwrap_or_default();
                        formatter.format_generic_app(&bubble, bundle_id, attachments, message)
                    }
                },
                CustomBalloon::Polls
                | CustomBalloon::Handwriting
                | CustomBalloon::DigitalTouch
                | CustomBalloon::URL => {
                    return Err(PlistParseError::WrongMessageType.into());
                }
            }
        }
    };

    Ok(rendered)
}

// MARK: Check In

/// Build the footer line that accompanies a Check In balloon.
///
/// The `"<verb> at <local time>"` phrase derived from the balloon's first
/// [`CheckInKind`] entry. Returns `None` when the balloon has no decodable
/// check-in metadata, so callers can omit the footer entirely.
pub fn resolve_check_in_footer(balloon: &AppMessage) -> Option<String> {
    balloon.check_in_kind(0).map(|(kind, at)| {
        let at = format(&at);
        match kind {
            CheckInKind::Expected => format!("Expected at {at}"),
            CheckInKind::WasExpected => format!("Was expected at {at}"),
            CheckInKind::CheckedIn => format!("Checked in at {at}"),
        }
    })
}

// MARK: Fitness

/// Replace the leading [`FITNESS_RECEIVER`] sentinel emitted by Fitness app
/// messages with [`YOU`] so the rendered string reads in first person.
/// Returns the input unchanged if the sentinel isn't present.
pub fn rewrite_fitness_receiver(text: String) -> String {
    if let Some(rest) = text.strip_prefix(FITNESS_RECEIVER) {
        format!("{YOU}{rest}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_fitness_receiver;

    #[test]
    fn rewrite_fitness_receiver_replaces_sentinel_prefix() {
        let input = "$(kIMTranscriptPluginBreadcrumbTextReceiverIdentifier) closed all three rings"
            .to_string();
        assert_eq!(
            rewrite_fitness_receiver(input),
            "You closed all three rings".to_string(),
        );
    }

    #[test]
    fn rewrite_fitness_receiver_passes_non_sentinel_text_through() {
        let input = "Alice closed all three rings".to_string();
        assert_eq!(rewrite_fitness_receiver(input.clone()), input);
    }
}
