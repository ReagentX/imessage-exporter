#[cfg(test)]
mod tests {
    use crate::{
        message_types::variants::{CustomBalloon, Tapback, TapbackAction, Variant},
        tables::messages::Message,
    };

    #[test]
    fn test_standard_message() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        assert!(matches!(m.variant(), Variant::Normal));
    }

    #[test]
    fn test_built_in_app_url() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.messages.URLBalloonProvider".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::URL)));
    }

    #[test]
    fn test_built_in_app_handwriting() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.Handwriting.HandwritingProvider".to_string(),
        );
        assert!(matches!(
            m.variant(),
            Variant::App(CustomBalloon::Handwriting)
        ));
    }

    #[test]
    fn test_built_in_app_find_my() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.findmy.FindMyMessagesApp".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::FindMy)));
    }

    #[test]
    fn test_built_in_app_business() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.icloud.apps.messages.business.extension".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::Business)));
    }

    #[test]
    fn test_tapback_added_heart() {
        let mut m = Message::blank();
        m.associated_message_type = Some(2000);
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Added, Tapback::Loved)
        ));
    }

    #[test]
    fn test_tapback_removed_like() {
        let mut m = Message::blank();
        m.associated_message_type = Some(3001);
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Removed, Tapback::Liked)
        ));
    }

    #[test]
    fn test_sticker() {
        let mut m = Message::blank();
        m.associated_message_type = Some(1000);
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Added, Tapback::Sticker)
        ));
    }

    #[test]
    fn test_shareplay() {
        let mut m = Message::blank();
        m.item_type = 6;
        assert!(matches!(m.variant(), Variant::SharePlay));
    }

    #[test]
    fn test_custom_emoji_tapback() {
        let mut m = Message::blank();
        m.associated_message_type = Some(2006);
        m.associated_message_emoji = Some("🎉".to_owned());
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Added, Tapback::Emoji(Some("🎉")))
        ));
    }

    #[test]
    fn can_get_variant_third_party_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some("com.apple.messages.MSMessageExtensionBalloonPlugin:QPU8QS3E62:com.contextoptional.OpenTable.Messages".to_owned());

        assert!(matches!(
            m.variant(),
            Variant::App(CustomBalloon::Application(
                "com.contextoptional.OpenTable.Messages"
            ))
        ));
    }

    #[test]
    fn test_edited_message() {
        let mut m = Message::blank();
        m.date_edited = 1234567890;
        assert!(matches!(m.variant(), Variant::Edited));
    }

    #[test]
    fn test_unknown_type() {
        let mut m = Message::blank();
        m.associated_message_type = Some(9999);
        assert!(matches!(m.variant(), Variant::Unknown(9999)));
    }

    #[test]
    fn test_fitness_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.ActivityMessagesApp.MessagesExtension".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::Fitness)));
    }

    #[test]
    fn test_apple_pay_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.PassbookUIService.PeerPaymentMessagesExtension".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::ApplePay)));
    }

    #[test]
    fn test_slideshow_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.mobileslideshow.PhotosMessagesApp".to_string(),
        );
        assert!(matches!(
            m.variant(),
            Variant::App(CustomBalloon::Slideshow)
        ));
    }

    #[test]
    fn test_check_in_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.SafetyMonitorApp.SafetyMonitorMessages".to_string(),
        );
        assert!(matches!(m.variant(), Variant::App(CustomBalloon::CheckIn)));
    }

    #[test]
    fn test_digital_touch_app() {
        let mut m = Message::blank();
        m.associated_message_type = Some(0);
        m.balloon_bundle_id = Some(
            "com.apple.messages.MSMessageExtensionBalloonPlugin:XXX:com.apple.DigitalTouchBalloonProvider".to_string(),
        );
        assert!(matches!(
            m.variant(),
            Variant::App(CustomBalloon::DigitalTouch)
        ));
    }

    #[test]
    fn test_removed_custom_emoji_tapback() {
        let mut m = Message::blank();
        m.associated_message_type = Some(3006);
        m.associated_message_emoji = Some("🎉".to_owned());
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Removed, Tapback::Emoji(Some("🎉")))
        ));
    }

    #[test]
    fn test_sticker_reply() {
        let mut m = Message::blank();
        m.associated_message_type = Some(2007);
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Added, Tapback::Sticker)
        ));
    }

    #[test]
    fn test_remove_sticker_reply() {
        let mut m = Message::blank();
        m.associated_message_type = Some(3007);
        assert!(matches!(
            m.variant(),
            Variant::Tapback(0, TapbackAction::Removed, Tapback::Sticker)
        ));
    }
}

#[cfg(test)]
mod conversation_av_mode_tests {
    use plist::{Dictionary, Value};

    use crate::tables::messages::{Message, models::ConversationAVMode};

    /// `message_summary_info` of a FaceTime video call, as Messages writes it:
    /// a binary plist holding only `conversationAVMode` = 2.
    const VIDEO_SUMMARY: &[u8] = &[
        0x62, 0x70, 0x6C, 0x69, 0x73, 0x74, 0x30, 0x30, 0xD1, 0x01, 0x02, 0x5F, 0x10, 0x12, 0x63,
        0x6F, 0x6E, 0x76, 0x65, 0x72, 0x73, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x41, 0x56, 0x4D, 0x6F,
        0x64, 0x65, 0x10, 0x02, 0x08, 0x0B, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    ];

    /// Binary plist `message_summary_info` holding `conversationAVMode` set to
    /// `value`.
    fn summary(value: Value) -> Vec<u8> {
        let mut dict = Dictionary::new();
        dict.insert("conversationAVMode".to_string(), value);
        let mut bytes = vec![];
        Value::Dictionary(dict)
            .to_writer_binary(&mut bytes)
            .unwrap();
        bytes
    }

    fn parsed(item_type: i32, summary: Option<&[u8]>) -> Message {
        let mut msg = Message::blank();
        msg.item_type = item_type;
        msg.parse_body(None, summary, false);
        msg
    }

    #[test]
    fn reads_messages_video_summary() {
        assert_eq!(
            parsed(6, Some(VIDEO_SUMMARY)).conversation_av_mode,
            Some(ConversationAVMode::Video)
        );
    }

    #[test]
    fn reads_each_mode() {
        for (raw, mode) in [
            (0, ConversationAVMode::AVLess),
            (1, ConversationAVMode::Audio),
            (2, ConversationAVMode::Video),
            (3, ConversationAVMode::Unknown(3)),
        ] {
            let bytes = summary(Value::Integer(raw.into()));
            assert_eq!(
                parsed(6, Some(&bytes)).conversation_av_mode,
                Some(mode),
                "{raw}"
            );
        }
    }

    #[test]
    fn absent_summary_has_no_mode() {
        assert_eq!(parsed(6, None).conversation_av_mode, None);
    }

    #[test]
    fn summary_without_key_has_no_mode() {
        let mut bytes = vec![];
        Value::Dictionary(Dictionary::new())
            .to_writer_binary(&mut bytes)
            .unwrap();
        assert_eq!(parsed(6, Some(&bytes)).conversation_av_mode, None);
    }

    #[test]
    fn non_integer_mode_is_none() {
        for value in [Value::Integer((-1).into()), Value::String("2".to_string())] {
            let bytes = summary(value);
            assert_eq!(parsed(6, Some(&bytes)).conversation_av_mode, None);
        }
    }

    #[test]
    fn other_item_types_ignore_the_key() {
        for item_type in [0, 1, 5, 7] {
            assert_eq!(
                parsed(item_type, Some(VIDEO_SUMMARY)).conversation_av_mode,
                None,
                "{item_type}"
            );
        }
    }
}
