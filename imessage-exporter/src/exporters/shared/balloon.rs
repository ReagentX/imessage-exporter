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
        let poll = message.as_poll(config.data_source.db(), &config.data_source.capabilities)?;
        return match poll {
            Some(poll) => Ok(formatter.format_poll(&poll)),
            None => Err(PlistParseError::PollError.into()),
        };
    }

    // Otherwise, we expect an NSKeyedArchiver payload
    let parsed = match message
        .payload_data(config.data_source.db())
        .ok_or(PlistParseError::NoPayload)
        .and_then(|payload| parse_ns_keyed_archiver(&payload))
    {
        Ok(parsed) => parsed,
        Err(why) if message.is_url() => return url_text_fallback(formatter, message, why),
        Err(why) => return Err(why.into()),
    };

    if message.is_url() {
        return format_url_balloon(
            formatter,
            message,
            URLMessage::get_url_message_override(&parsed),
        );
    }

    let bubble = AppMessage::from_map(&parsed)?;
    let rendered = match balloon {
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
                let bundle_id = parse_balloon_bundle_id(message.balloon_bundle_id.as_deref())
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
    };

    Ok(rendered)
}

/// Format a URL balloon from its resolved subtype.
///
/// `NoPayload` reaches the fallback from three sources: `payload_data` is
/// absent, the archive's root is null, or no URL subtype matched the parsed
/// metadata. Each means "no preview," not "corrupt": when the message carries
/// text (the URL itself) render that instead of an error stub. Every other
/// parse error surfaces unchanged.
fn format_url_balloon<F: BalloonFormatter>(
    formatter: &F,
    message: &Message,
    bubble: Result<URLOverride<'_>, PlistParseError>,
) -> Result<String, RuntimeError> {
    Ok(match bubble {
        Ok(URLOverride::Normal(b)) => formatter.format_url(message, &b),
        Ok(URLOverride::AppleMusic(b)) => formatter.format_music(&b),
        Ok(URLOverride::Collaboration(b)) => formatter.format_collaboration(&b),
        Ok(URLOverride::AppStore(b)) => formatter.format_app_store(&b),
        Ok(URLOverride::SharedPlacemark(b)) => formatter.format_placemark(&b),
        Err(why) => return url_text_fallback(formatter, message, why),
    })
}

/// Render a URL balloon with no preview metadata as its bare text.
fn url_text_fallback<F: BalloonFormatter>(
    formatter: &F,
    message: &Message,
    why: PlistParseError,
) -> Result<String, RuntimeError> {
    match why {
        PlistParseError::NoPayload if message.text.is_some() => {
            Ok(formatter.format_url(message, &URLMessage::default()))
        }
        why => Err(why.into()),
    }
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
    use super::{format_url_balloon, rewrite_fitness_receiver};
    use crate::{Config, Options, app::export_type::ExportType, exporters::html::HTML};
    use imessage_database::error::plist::PlistParseError;

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

    #[test]
    fn url_payload_without_metadata_uses_message_text_fallback() {
        let config = Config::fake_app(Options::fake_options(ExportType::Html));
        let formatter = HTML::new(&config).unwrap();
        let mut message = Config::fake_message();
        message.text = Some("https://example.com".to_string());

        let actual =
            format_url_balloon(&formatter, &message, Err(PlistParseError::NoPayload)).unwrap();

        assert!(actual.contains("https://example.com"));
    }

    #[test]
    fn url_payload_without_metadata_still_errors_without_message_text() {
        let config = Config::fake_app(Options::fake_options(ExportType::Html));
        let formatter = HTML::new(&config).unwrap();
        let mut message = Config::fake_message();
        message.text = None;

        assert!(format_url_balloon(&formatter, &message, Err(PlistParseError::NoPayload)).is_err());
    }
}
