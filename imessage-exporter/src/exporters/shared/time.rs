use imessage_database::{
    tables::messages::Message,
    util::dates::{format, to_utc},
};

use crate::app::{runtime::Config, zone::Zone};

/// Format `message`'s timestamp in `zone` via [`format()`]. On failure, use
/// the timestamp error's `Display` output.
pub fn format_message_date(message: &Message, zone: &Zone) -> String {
    match message.date() {
        Ok(d) => format(&d.with_timezone(zone)),
        Err(why) => why.to_string(),
    }
}

/// Same as [`format_message_date`] but for a raw `i64` iMessage timestamp
/// (used by edit-history events, which carry their own `date` field rather
/// than a `Message`).
pub fn format_timestamp(timestamp: i64, zone: &Zone) -> String {
    match to_utc(timestamp) {
        Ok(d) => format(&d.with_timezone(zone)),
        Err(why) => why.to_string(),
    }
}

/// Compute the formatted timestamp and read receipt for a message.
/// Returns `(formatted_date, read_receipt)` where `read_receipt` is
/// empty if there is no read receipt data.
pub fn message_time(config: &Config, message: &Message) -> (String, String) {
    let zone = &config.options.timezone;
    let date = format_message_date(message, zone);
    let mut read_receipt = String::new();
    if let Some(time) = message.time_until_read(zone)
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

#[cfg(test)]
mod tests {
    use super::message_time;
    use crate::{Config, Options, app::export_type::ExportType};

    fn make_config_with_custom_name(custom_name: Option<&str>) -> Config {
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = custom_name.map(str::to_string);
        Config::fake_app(options)
    }

    #[test]
    fn message_time_no_read_receipt() {
        let config = make_config_with_custom_name(None);
        let mut msg = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        msg.date = 674526582885055488;
        // date_read=0 yields no read receipt
        let (date, read) = message_time(&config, &msg);
        assert_eq!(date, "May 17, 2022  5:29:42 PM");
        assert!(read.is_empty(), "expected empty read receipt, got {read:?}");
    }

    #[test]
    fn message_time_read_from_them_uses_default_you() {
        let config = make_config_with_custom_name(None);
        let mut msg = Config::fake_message();
        msg.date = 674526582885055488;
        msg.date_delivered = 674526582885055488;
        msg.date_read = 674530231992568192;
        // is_from_me defaults to false → reader is "you"
        let (date, read) = message_time(&config, &msg);
        assert_eq!(date, "May 17, 2022  5:29:42 PM");
        assert_eq!(read, "(Read by you after 1 hour, 49 seconds)");
    }

    #[test]
    fn message_time_read_from_them_uses_custom_name() {
        let config = make_config_with_custom_name(Some("Chris"));
        let mut msg = Config::fake_message();
        msg.date = 674526582885055488;
        msg.date_delivered = 674526582885055488;
        msg.date_read = 674530231992568192;
        let (_, read) = message_time(&config, &msg);
        assert_eq!(read, "(Read by Chris after 1 hour, 49 seconds)");
    }

    #[test]
    fn message_time_read_from_me_uses_them() {
        let config = make_config_with_custom_name(Some("Chris"));
        let mut msg = Config::fake_message();
        // Sent at 8:29:42 PM, delivered ~1 hour later. For sent messages,
        // time_until_read measures sent → delivered (not date_read).
        msg.date = 674526582885055488;
        msg.date_delivered = 674530231992568192;
        msg.is_from_me = true;
        // When the message is from me, the reader on the other side is "them"
        // regardless of custom_name (custom_name renames you, not the recipient).
        let (_, read) = message_time(&config, &msg);
        assert_eq!(read, "(Read by them after 1 hour, 49 seconds)");
    }
}
