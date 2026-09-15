/*!
  App messages are messages that developers can generate with their apps.
  Some built-in functionality also uses App Messages, like Apple Pay or Handwriting.
*/

use std::collections::HashMap;

use chrono::{DateTime, Local};
use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::{
        dates::{TIMESTAMP_FACTOR, get_local_time},
        plist::{get_string_from_dict, get_string_from_nested_dict},
    },
};

/// This struct represents Apple's [`MSMessageTemplateLayout`](https://developer.apple.com/documentation/messages/msmessagetemplatelayout).
#[derive(Debug, PartialEq, Eq)]
pub struct AppMessage<'a> {
    /// An image used to represent the message in the transcript
    pub image: Option<&'a str>,
    /// A URL pointing to a media file used to represent the message in the transcript
    pub url: Option<&'a str>,
    /// The title for the image or media file
    pub title: Option<&'a str>,
    /// The subtitle for the image or media file
    pub subtitle: Option<&'a str>,
    /// A left-aligned caption for the message bubble
    pub caption: Option<&'a str>,
    /// A left-aligned subcaption for the message bubble
    pub subcaption: Option<&'a str>,
    /// A right-aligned caption for the message bubble
    pub trailing_caption: Option<&'a str>,
    /// A right-aligned subcaption for the message bubble
    pub trailing_subcaption: Option<&'a str>,
    /// The name of the app that created this message
    pub app_name: Option<&'a str>,
    /// This property is set only for Apple system messages,
    /// it represents the text that displays in the center of the bubble
    pub ldtext: Option<&'a str>,
}

impl<'a> BalloonProvider<'a> for AppMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        let user_info = payload
            .as_dictionary()
            .ok_or_else(|| {
                PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
            })?
            .get("userInfo")
            .ok_or_else(|| PlistParseError::MissingKey("userInfo".to_string()))?;
        Ok(AppMessage {
            image: get_string_from_dict(payload, "image"),
            url: get_string_from_nested_dict(payload, "URL"),
            title: get_string_from_dict(user_info, "image-title"),
            subtitle: get_string_from_dict(user_info, "image-subtitle"),
            caption: get_string_from_dict(user_info, "caption"),
            subcaption: get_string_from_dict(user_info, "subcaption"),
            trailing_caption: get_string_from_dict(user_info, "secondary-subcaption"),
            trailing_subcaption: get_string_from_dict(user_info, "tertiary-subcaption"),
            app_name: get_string_from_dict(payload, "an"),
            ldtext: get_string_from_dict(payload, "ldtext"),
        })
    }
}

impl AppMessage<'_> {
    /// Parse key/value pairs from the query string in the balloon's URL
    #[must_use]
    pub fn parse_query_string(&self) -> HashMap<&str, &str> {
        let mut map = HashMap::new();

        if let Some(url) = self.url
            && url.starts_with('?')
        {
            let parts = url.strip_prefix('?').unwrap_or(url).split('&');
            for part in parts {
                let key_val_split: Vec<&str> = part.split('=').collect();
                if key_val_split.len() == 2 {
                    map.insert(key_val_split[0], key_val_split[1]);
                }
            }
        }
        map
    }

    /// Identifies the metadata state of a Check In balloon and resolves its
    /// associated timestamp to local time. Returns `None` when no recognized
    /// Check In key is present in [`parse_query_string`](Self::parse_query_string)
    /// or the value isn't a parseable iMessage timestamp.
    ///
    /// `offset` is the seconds adjustment to apply to the iMessage epoch when
    /// converting to local time: pass `0` to use the system's current
    /// timezone, or a [`get_offset`](crate::util::dates::get_offset)-derived
    /// value when reading a database exported from a different timezone.
    #[must_use]
    pub fn check_in_kind(&self, offset: i64) -> Option<(CheckInKind, DateTime<Local>)> {
        let metadata = self.parse_query_string();
        let (kind, date_str) = if let Some(d) = metadata.get("estimatedEndTime") {
            (CheckInKind::Expected, *d)
        } else if let Some(d) = metadata.get("triggerTime") {
            (CheckInKind::WasExpected, *d)
        } else {
            let d = metadata.get("sendDate")?;
            (CheckInKind::CheckedIn, *d)
        };
        let date_stamp = (date_str.parse::<f64>().ok()? as i64).checked_mul(TIMESTAMP_FACTOR)?;
        let date_time = get_local_time(date_stamp, offset).ok()?;
        Some((kind, date_time))
    }
}

/// One of the three metadata states a Check In balloon can advertise. The
/// variant choice mirrors the query-string key the timestamp came from
/// (`estimatedEndTime`, `triggerTime`, `sendDate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckInKind {
    /// `estimatedEndTime`: Check In is scheduled and still pending.
    Expected,
    /// `triggerTime`: Check In window has passed without confirmation.
    WasExpected,
    /// `sendDate`: Check In was manually confirmed.
    CheckedIn,
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{
            app::{AppMessage, CheckInKind},
            variants::BalloonProvider,
        },
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::fs::File;
    use std::{collections::HashMap, env::current_dir};

    fn check_in_msg(url: &str) -> AppMessage<'_> {
        AppMessage {
            image: None,
            url: Some(url),
            title: None,
            subtitle: None,
            caption: None,
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: None,
        }
    }

    #[test]
    fn check_in_kind_prefers_estimated_end_time() {
        let balloon = check_in_msg(
            "?estimatedEndTime=1697316869.688709&triggerTime=1697316869.688709&sendDate=1697316869.688709",
        );
        assert!(matches!(
            balloon.check_in_kind(0),
            Some((CheckInKind::Expected, _)),
        ));
    }

    #[test]
    fn check_in_kind_falls_back_to_trigger_time() {
        let balloon = check_in_msg("?triggerTime=1697316869.688709&sendDate=1697316869.688709");
        assert!(matches!(
            balloon.check_in_kind(0),
            Some((CheckInKind::WasExpected, _)),
        ));
    }

    #[test]
    fn check_in_kind_uses_send_date_when_only_option() {
        let balloon = check_in_msg("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709");
        assert!(matches!(
            balloon.check_in_kind(0),
            Some((CheckInKind::CheckedIn, _)),
        ));
    }

    #[test]
    fn check_in_kind_returns_none_for_unparsable_timestamp() {
        let balloon = check_in_msg("?sendDate=not_a_number");
        assert!(balloon.check_in_kind(0).is_none());
    }

    #[test]
    fn check_in_kind_returns_none_for_overflowing_timestamp() {
        // The nanosecond conversion (`seconds * 1_000_000_000`) overflows i64 for
        // this value; it must degrade to None rather than panic (debug) or wrap to
        // a bogus date (release).
        let balloon = check_in_msg("?sendDate=99999999999");
        assert!(balloon.check_in_kind(0).is_none());
    }

    #[test]
    fn check_in_kind_returns_none_without_recognized_key() {
        let balloon = check_in_msg("?messageType=1&interfaceVersion=1");
        assert!(balloon.check_in_kind(0).is_none());
    }

    #[test]
    fn test_parse_apple_pay_sent_265() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Sent265.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some("data:application/vnd.apple.pkppm;base64,FAKE_BASE64_DATA="),
            title: None,
            subtitle: None,
            caption: Some("Apple\u{a0}Cash"),
            subcaption: Some("$265\u{a0}Payment"),
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Apple\u{a0}Pay"),
            ldtext: Some("Sent $265 with Apple\u{a0}Pay."),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_apple_pay_recurring_1() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/ApplePayRecurring.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some("data:application/vnd.apple.pkppm;base64,FAKEDATA"),
            title: None,
            subtitle: None,
            caption: None,
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Apple\u{a0}Cash"),
            ldtext: Some("Sending you $1 weekly starting Nov 18, 2023"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_opentable_invite() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/OpenTableInvited.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some(
                "https://www.opentable.com/book/view?rid=0000000&confnumber=00000&invitationId=1234567890-abcd-def-ghij-4u5t1sv3ryc00l",
            ),
            title: Some("Rusty Grill - Boise"),
            subtitle: Some("Reservation Confirmed"),
            caption: Some("Table for 4 people\nSunday, October 17 at 7:45 PM"),
            subcaption: Some("You're invited! Tap to accept."),
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("OpenTable"),
            ldtext: None,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_slideshow() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Slideshow.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some("https://share.icloud.com/photos/1337h4x0r_jk#Home"),
            title: None,
            subtitle: None,
            caption: Some("Home"),
            subcaption: Some("37 Photos"),
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Photos"),
            ldtext: Some("Home - 37 Photos"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_game() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Game.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some("data:?ver=48&data=pr3t3ndth3r3154b10b0fd4t4h3re=3"),
            title: None,
            subtitle: None,
            caption: Some("Your move."),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("GamePigeon"),
            ldtext: Some("Dots & Boxes"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_business() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Business.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some(
                "?receivedMessage=33c309ab520bc2c76e99c493157ed578&replyMessage=6a991da615f2e75d4aa0de334e529024",
            ),
            title: None,
            subtitle: None,
            caption: Some("Yes, connect me with Goldman Sachs."),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Business"),
            ldtext: Some("Yes, connect me with Goldman Sachs."),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_business_query_string() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Business.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let mut expected = HashMap::new();
        expected.insert("receivedMessage", "33c309ab520bc2c76e99c493157ed578");
        expected.insert("replyMessage", "6a991da615f2e75d4aa0de334e529024");

        assert_eq!(balloon.parse_query_string(), expected);
    }

    #[test]
    fn test_parse_check_in_timer() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/CheckinTimer.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();

        let expected = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_check_in_timer_late() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/CheckinLate.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();

        let expected = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Has not checked in when expected, location shared"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Has not checked in when expected, location shared"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_check_in_location() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/CheckinLocation.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();

        let expected = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Fake Location"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Fake Location"),
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_check_in_query_string() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/CheckinTimer.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let mut expected = HashMap::new();
        expected.insert("messageType", "1");
        expected.insert("interfaceVersion", "1");
        expected.insert("sendDate", "1697316869.688709");

        assert_eq!(balloon.parse_query_string(), expected);
    }

    #[test]
    fn test_parse_find_my() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/FindMy.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = AppMessage::from_map(&parsed).unwrap();
        let expected = AppMessage {
            image: None,
            url: Some(
                "?FindMyMessagePayloadVersionKey=v0&FindMyMessagePayloadZippedDataKey=FAKEDATA",
            ),
            title: None,
            subtitle: None,
            caption: None,
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Find My"),
            ldtext: Some("Started Sharing Location"),
        };

        assert_eq!(balloon, expected);
    }
}
