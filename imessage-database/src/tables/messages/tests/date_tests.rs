#[cfg(test)]
mod tests {

    use crate::{tables::messages::Message, util::dates::get_offset};

    #[test]
    fn can_get_time_date_read_after_date() {
        // Get offset
        let offset = get_offset();

        // Create message
        let mut message = Message::blank();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        assert_eq!(
            message.time_until_read(&offset),
            Some("1 hour, 49 seconds".to_string())
        );
    }

    #[test]
    fn can_get_time_date_read_before_date() {
        // Get offset
        let offset = get_offset();

        // Create message
        let mut message = Message::blank();
        // May 17, 2022  9:30:31 PM
        message.date = 674530231992568192;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        // May 17, 2022  8:29:42 PM
        message.date_read = 674526582885055488;

        assert_eq!(message.time_until_read(&offset), None);
    }

    #[test]
    fn can_get_time_until_read_none() {
        let offset = get_offset();
        let m = Message::blank();
        assert_eq!(m.time_until_read(&offset), None);
    }

    #[test]
    fn can_get_date_edited() {
        let offset = get_offset();
        let mut m = Message::blank();
        // May 17, 2022 8:29:42 PM
        m.date_edited = 674526582885055488;

        let result = m.date_edited(&offset);
        assert!(result.is_ok());
    }

    #[test]
    fn can_get_date_delivered() {
        let offset = get_offset();
        let mut m = Message::blank();
        // May 17, 2022 8:29:42 PM
        m.date_delivered = 674526582885055488;

        let result = m.date_delivered(&offset);
        assert!(result.is_ok());
    }
}
