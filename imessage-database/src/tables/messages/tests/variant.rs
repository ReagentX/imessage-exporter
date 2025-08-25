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
