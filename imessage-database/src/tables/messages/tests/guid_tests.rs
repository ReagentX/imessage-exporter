#[cfg(test)]
mod tests {
    use crate::tables::messages::Message;

    #[test]
    fn can_get_valid_guid() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((0usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_p() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("p:1/A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((1usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid_p() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("p:1/FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_bp() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("bp:A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());

        assert_eq!(
            Some((0usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }

    #[test]
    fn cant_get_invalid_guid_bp() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("bp:FAKE_GUID".to_string());

        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_empty() {
        let mut m = Message::blank();
        m.associated_message_guid = Some(String::new());
        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_too_short() {
        let mut m = Message::blank();
        m.associated_message_guid = Some("A44CE9D7-AAAA-BBBB-CCCC".to_string());
        assert_eq!(None, m.clean_associated_guid());
    }

    #[test]
    fn can_get_valid_guid_p_invalid_index() {
        let mut m = Message::blank();
        m.associated_message_guid =
            Some("p:invalid/A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A".to_string());
        assert_eq!(
            Some((0usize, "A44CE9D7-AAAA-BBBB-CCCC-23C54E1A9B6A")),
            m.clean_associated_guid()
        );
    }
}
