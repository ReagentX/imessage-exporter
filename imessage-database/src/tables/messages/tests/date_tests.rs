#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::tables::messages::Message;

    #[test]
    fn can_get_time_date_read_after_date() {
        let mut message = Message::blank();
        // May 18, 2022 00:29:42 UTC
        message.date = 674526582885055488;
        // May 18, 2022 00:29:42 UTC
        message.date_delivered = 674526582885055488;
        // May 18, 2022 01:30:31 UTC
        message.date_read = 674530231992568192;

        assert_eq!(
            message.time_until_read(&Utc),
            Some("1 hour, 49 seconds".to_string())
        );
    }

    #[test]
    fn can_get_time_date_read_before_date() {
        let mut message = Message::blank();
        // May 18, 2022 01:30:31 UTC
        message.date = 674530231992568192;
        // May 18, 2022 01:30:31 UTC
        message.date_delivered = 674530231992568192;
        // May 18, 2022 00:29:42 UTC
        message.date_read = 674526582885055488;

        assert_eq!(message.time_until_read(&Utc), None);
    }

    #[test]
    fn can_get_time_until_read_none() {
        let m = Message::blank();
        assert_eq!(m.time_until_read(&Utc), None);
    }
}
