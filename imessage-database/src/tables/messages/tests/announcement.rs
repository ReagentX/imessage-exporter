#[cfg(test)]
mod group_action_tests {
    use crate::tables::messages::{message::Message, models::GroupAction};

    #[test]
    fn test_group_action_participant_added() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.other_handle = Some(123);

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ParticipantAdded(123))
        ));
    }

    #[test]
    fn test_group_action_participant_added_missing_handle() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.other_handle = None;

        assert!(Message::group_action(&msg).is_none());
    }

    #[test]
    fn test_group_action_participant_removed() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 1;
        msg.other_handle = Some(456);

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ParticipantRemoved(456))
        ));
    }

    #[test]
    fn test_group_action_participant_removed_missing_handle() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 1;
        msg.other_handle = None;

        assert!(Message::group_action(&msg).is_none());
    }

    #[test]
    fn test_group_action_name_change() {
        let mut msg = Message::blank();
        msg.item_type = 2;
        msg.group_title = Some("New Group Name".to_string());

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::NameChange("New Group Name"))
        ));
    }

    #[test]
    fn test_group_action_name_change_missing_title() {
        let mut msg = Message::blank();
        msg.item_type = 2;
        msg.group_title = None;

        assert!(Message::group_action(&msg).is_none());
    }

    #[test]
    fn test_group_action_participant_left() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 0;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ParticipantLeft)
        ));
    }

    #[test]
    fn test_group_action_icon_changed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 1;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::GroupIconChanged)
        ));
    }

    #[test]
    fn test_group_action_icon_removed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 2;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::GroupIconRemoved)
        ));
    }
}

#[cfg(test)]
mod announcement_tests {
    use crate::{
        message_types::{
            edited::{EditStatus, EditedMessage, EditedMessagePart},
            variants::Announcement,
        },
        tables::messages::{message::Message, models::GroupAction},
    };

    #[test]
    fn test_announcement_participant_added() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.other_handle = Some(123);

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::ParticipantAdded(
                123
            )))
        ));
    }

    #[test]
    fn test_announcement_participant_removed() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 1;
        msg.other_handle = Some(123);

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::ParticipantRemoved(
                123
            )))
        ));
    }

    #[test]
    fn test_announcement_name_change() {
        let mut msg = Message::blank();
        msg.item_type = 2;
        msg.group_title = Some("Test Group".to_string());

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::NameChange(
                "Test Group"
            )))
        ));
    }

    #[test]
    fn test_announcement_participant_left() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 0;

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::ParticipantLeft))
        ));
    }

    #[test]
    fn test_announcement_icon_changed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 1;

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::GroupIconChanged))
        ));
    }

    #[test]
    fn test_announcement_icon_removed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 2;

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::GroupIconRemoved))
        ));
    }

    #[test]
    fn test_announcement_single_part_fully_unsent() {
        let mut msg = Message::blank();
        msg.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::FullyUnsent)
        ));
    }

    #[test]
    fn test_announcement_multi_part_fully_unsent() {
        let mut msg = Message::blank();
        msg.edited_parts = Some(EditedMessage {
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

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::FullyUnsent)
        ));
    }

    #[test]
    fn test_announcement_partially_unsent() {
        let mut msg = Message::blank();
        msg.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Edited,
                    edit_history: vec![],
                },
            ],
        });

        assert!(msg.get_announcement().is_none());
    }

    #[test]
    fn test_announcement_regular_message() {
        let msg = Message::blank();
        assert!(msg.get_announcement().is_none());
    }

    #[test]
    fn test_announcement_edited_not_unsent() {
        let mut msg = Message::blank();
        msg.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![],
            }],
        });

        assert!(msg.get_announcement().is_none());
    }

    #[test]
    fn test_announcement_no_special_properties() {
        let mut msg = Message::blank();
        msg.item_type = 0;
        msg.group_action_type = 0;
        msg.edited_parts = None;

        assert!(msg.get_announcement().is_none());
    }

    #[test]
    fn test_announcement_kept_audio_message() {
        let mut msg = Message::blank();
        msg.item_type = 5;

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::AudioMessageKept)
        ));
    }
}
