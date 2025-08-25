#[cfg(test)]
mod tests {
    use crate::{
        message_types::edited::{EditStatus, EditedMessage, EditedMessagePart},
        tables::messages::Message,
    };

    #[test]
    fn can_get_fully_unsent_true_single() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        assert!(m.is_fully_unsent());
    }

    #[test]
    fn can_get_fully_unsent_true_multiple() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        });

        assert!(m.is_fully_unsent());
    }

    #[test]
    fn can_get_fully_unsent_false() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Original,
                edit_history: vec![],
            }],
        });

        assert!(!m.is_fully_unsent());
    }

    #[test]
    fn can_get_fully_unsent_false_multiple() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
            ],
        });

        assert!(!m.is_fully_unsent());
    }

    #[test]
    fn can_get_part_edited_true() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
            ],
        });

        assert!(m.is_part_edited(0));
    }

    #[test]
    fn can_get_part_edited_false() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
            ],
        });

        assert!(!m.is_part_edited(1));
    }

    #[test]
    fn can_get_part_edited_blank() {
        let m = Message::blank();

        assert!(!m.is_part_edited(0));
    }

    #[test]
    fn can_get_fully_unsent_none() {
        let m = Message::blank();

        assert!(!m.is_fully_unsent());
    }

    #[test]
    fn can_get_part_edited_multiple_parts() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
            ],
        });

        assert!(m.is_part_edited(0));
        assert!(m.is_part_edited(1));
        assert!(!m.is_part_edited(2));
    }

    #[test]
    fn can_get_fully_unsent_mixed_statuses() {
        let mut m = Message::blank();
        m.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
            ],
        });

        assert!(!m.is_fully_unsent());
    }
}
