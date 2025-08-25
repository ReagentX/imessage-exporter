#[cfg(test)]
mod tests {
    use crate::{message_types::expressives, tables::messages::Message};

    #[test]
    fn can_get_message_expression_none() {
        let m = Message::blank();
        assert_eq!(m.get_expressive(), expressives::Expressive::None);
    }

    #[test]
    fn can_get_message_expression_bubble() {
        let mut m = Message::blank();
        m.expressive_send_style_id = Some("com.apple.MobileSMS.expressivesend.gentle".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Bubble(expressives::BubbleEffect::Gentle)
        );
    }

    #[test]
    fn can_get_message_expression_screen() {
        let mut m = Message::blank();
        m.expressive_send_style_id =
            Some("com.apple.messages.effect.CKHappyBirthdayEffect".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Screen(expressives::ScreenEffect::Balloons)
        );
    }

    #[test]
    fn can_get_message_expression_slam() {
        let mut m = Message::blank();
        m.expressive_send_style_id = Some("com.apple.MobileSMS.expressivesend.impact".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Bubble(expressives::BubbleEffect::Slam)
        );
    }

    #[test]
    fn can_get_message_expression_invisible_ink() {
        let mut m = Message::blank();
        m.expressive_send_style_id =
            Some("com.apple.MobileSMS.expressivesend.invisibleink".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Bubble(expressives::BubbleEffect::InvisibleInk)
        );
    }

    #[test]
    fn can_get_message_expression_spotlight() {
        let mut m = Message::blank();
        m.expressive_send_style_id =
            Some("com.apple.messages.effect.CKSpotlightEffect".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Screen(expressives::ScreenEffect::Spotlight)
        );
    }

    #[test]
    fn can_get_message_expression_echo() {
        let mut m = Message::blank();
        m.expressive_send_style_id = Some("com.apple.messages.effect.CKEchoEffect".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Screen(expressives::ScreenEffect::Echo)
        );
    }

    #[test]
    fn can_get_message_expression_unknown() {
        let mut m = Message::blank();
        m.expressive_send_style_id = Some("com.apple.messages.effect.Unknown".to_string());
        assert_eq!(
            m.get_expressive(),
            expressives::Expressive::Unknown("com.apple.messages.effect.Unknown")
        );
    }
}
