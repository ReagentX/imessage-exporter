#[cfg(test)]
mod group_action_tests {
    use crate::tables::messages::{message::Message, models::GroupAction};

    #[test]
    fn test_group_action_participant_added() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.handle_id = Some(100);
        msg.other_handle = Some(123);

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ParticipantAdded(123))
        ));
    }

    #[test]
    fn test_group_action_phone_number_changed() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.handle_id = Some(349);
        msg.other_handle = Some(349);

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::PhoneNumberChanged(349))
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
    fn test_group_action_name_removed_null_title() {
        let mut msg = Message::blank();
        msg.item_type = 2;
        msg.group_title = None;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::NameRemoved)
        ));
    }

    #[test]
    fn test_group_action_name_removed_empty_title() {
        let mut msg = Message::blank();
        msg.item_type = 2;
        msg.group_title = Some(String::new());

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::NameRemoved)
        ));
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

    #[test]
    fn test_group_action_background_changed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 4;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ChatBackgroundChanged)
        ));
    }

    #[test]
    fn test_group_action_background_removed() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 6;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ChatBackgroundRemoved)
        ));
    }

    #[test]
    fn test_group_action_background_sensitive() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 5;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ChatBackgroundSensitive)
        ));
    }

    #[test]
    fn test_group_action_icon_changed_hidden() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 3;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::GroupIconChangedHidden)
        ));
    }

    #[test]
    fn test_group_action_background_changed_hidden() {
        let mut msg = Message::blank();
        msg.item_type = 3;
        msg.group_action_type = 7;

        assert!(matches!(
            Message::group_action(&msg),
            Some(GroupAction::ChatBackgroundChangedHidden)
        ));
    }
}

#[cfg(test)]
mod announcement_tests {
    use crate::{
        message_types::{
            edited::{EditStatus, EditedMessage, EditedMessagePart},
            variants::{Announcement, KeptMessage, UnknownEvent},
        },
        tables::messages::{message::Message, models::GroupAction},
    };

    #[test]
    fn test_announcement_participant_added() {
        let mut msg = Message::blank();
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.handle_id = Some(100);
        msg.other_handle = Some(123);

        assert!(matches!(
            msg.get_announcement(),
            Some(Announcement::GroupAction(GroupAction::ParticipantAdded(
                123
            )))
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
    fn test_announcement_kept_message_kinds() {
        for (code, kind) in [
            (0, KeptMessage::Audio),
            (1, KeptMessage::Video),
            (2, KeptMessage::App),
            (3, KeptMessage::Unknown(3)),
            (-1, KeptMessage::Unknown(-1)),
        ] {
            let mut msg = Message::blank();
            msg.item_type = 5;
            msg.message_action_type = code;

            assert_eq!(
                msg.get_announcement(),
                Some(Announcement::MessageKept(kind)),
                "{code}"
            );
        }
    }

    #[test]
    fn test_announcement_unknown_group_event() {
        for (item_type, action, other_handle) in [(1, 2, Some(7)), (3, 8, None), (1, 0, None)] {
            let mut msg = Message::blank();
            msg.item_type = item_type;
            msg.group_action_type = action;
            msg.other_handle = other_handle;

            assert_eq!(
                msg.get_announcement(),
                Some(Announcement::UnknownEvent(UnknownEvent::Group {
                    item_type,
                    group_action_type: action,
                })),
                "({item_type}, {action})"
            );
        }
    }

    #[test]
    fn test_announcement_unknown_location_event() {
        for (status, direction) in [(2, None), (0, Some(2))] {
            let mut msg = Message::blank();
            msg.item_type = 4;
            msg.share_status = status;
            msg.share_direction = direction;

            assert_eq!(
                msg.get_announcement(),
                Some(Announcement::UnknownEvent(UnknownEvent::Location {
                    share_status: status,
                    share_direction: direction,
                })),
                "({status}, {direction:?})"
            );
        }
    }

    #[test]
    fn test_announcement_transcript_sharing_stopped() {
        let mut msg = Message::blank();
        msg.item_type = 7;
        msg.handle_id = Some(1);

        assert_eq!(
            msg.get_announcement(),
            Some(Announcement::TranscriptSharingStopped)
        );
    }

    #[test]
    fn test_announcement_emergency_location_sent() {
        let mut msg = Message::blank();
        msg.item_type = 8;
        msg.handle_id = Some(1);

        assert_eq!(
            msg.get_announcement(),
            Some(Announcement::EmergencyLocationSent)
        );
    }

    #[test]
    fn test_announcement_unknown_item_type() {
        for item_type in [9, -1] {
            let mut msg = Message::blank();
            msg.item_type = item_type;

            assert_eq!(
                msg.get_announcement(),
                Some(Announcement::UnknownEvent(UnknownEvent::ItemType(
                    item_type
                ))),
                "{item_type}"
            );
        }
    }

    #[test]
    fn test_announcement_mapped_location_and_shareplay_are_not_events() {
        let mut location = Message::blank();
        location.item_type = 4;
        location.share_status = 1;
        location.share_direction = Some(0);
        assert!(location.get_announcement().is_none());

        let mut shareplay = Message::blank();
        shareplay.item_type = 6;
        assert!(shareplay.get_announcement().is_none());
    }
}

#[cfg(test)]
mod unattributed_tests {
    use plist::{Dictionary, Value};

    use crate::tables::messages::message::Message;

    /// Build a binary `message_summary_info` plist with `uat` set to `value`;
    /// omit the key for `None`.
    fn summary(value: Option<Value>) -> Vec<u8> {
        let mut dict = Dictionary::new();
        dict.insert("ams".to_string(), Value::String("Sender".to_string()));
        if let Some(value) = value {
            dict.insert("uat".to_string(), value);
        }
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
    fn group_events_read_uat_true() {
        let bytes = summary(Some(Value::Boolean(true)));
        for item_type in 1..=3 {
            assert!(
                parsed(item_type, Some(&bytes)).is_unattributed,
                "{item_type}"
            );
        }
    }

    #[test]
    fn group_events_read_nonzero_integer_uat() {
        let bytes = summary(Some(Value::Integer(1.into())));
        assert!(parsed(1, Some(&bytes)).is_unattributed);
    }

    #[test]
    fn group_events_reject_false_and_zero_uat() {
        for value in [Value::Boolean(false), Value::Integer(0.into())] {
            let bytes = summary(Some(value));
            assert!(!parsed(1, Some(&bytes)).is_unattributed);
        }
    }

    #[test]
    fn group_events_without_uat_are_attributed() {
        let bytes = summary(None);
        assert!(!parsed(1, Some(&bytes)).is_unattributed);
        assert!(!parsed(1, None).is_unattributed);
        assert!(!parsed(1, Some(&[0, 1, 2])).is_unattributed);
    }

    #[test]
    fn other_item_types_ignore_uat() {
        let bytes = summary(Some(Value::Boolean(true)));
        for item_type in [0, 4, 5, 6] {
            assert!(
                !parsed(item_type, Some(&bytes)).is_unattributed,
                "{item_type}"
            );
        }
    }
}

#[cfg(test)]
mod failed_tests {
    use crate::tables::messages::message::Message;

    fn event(item_type: i32, error: i64) -> Message {
        let mut msg = Message::blank();
        msg.item_type = item_type;
        msg.error = error;
        msg
    }

    #[test]
    fn group_events_fail_on_low_bits() {
        for item_type in 1..=3 {
            assert!(!event(item_type, 0).is_failed(), "({item_type}, 0)");
            assert!(event(item_type, 1).is_failed(), "({item_type}, 1)");
            assert!(
                event(item_type, 0xFFFF_FFFF).is_failed(),
                "({item_type}, 0xFFFF_FFFF)"
            );
        }
    }

    #[test]
    fn group_events_ignore_high_bits() {
        for item_type in 1..=3 {
            assert!(!event(item_type, 1 << 32).is_failed(), "{item_type}");
        }
    }

    #[test]
    fn other_item_types_never_fail() {
        for item_type in [0, 4, 5, 6, 7, 8] {
            assert!(!event(item_type, 1).is_failed(), "{item_type}");
        }
    }
}
