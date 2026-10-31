use imessage_database::{
    message_types::variants::Announcement,
    tables::{
        attachment::Attachment,
        messages::{Message, models::GroupAction},
        table::ME,
    },
};

use crate::{app::runtime::Config, exporters::shared::time::format_message_date};

/// Display name used in `ParticipantAdded` / `ParticipantRemoved`
/// announcements when the handle can't be resolved.
pub const UNKNOWN_PARTICIPANT: &str = "someone";
/// Second-person display name for the database owner.
pub(crate) const YOU: &str = "You";

/// A format-agnostic view of an announcement message. `who` is the actor
/// [`resolve_announcement`] chose, after the `ME` → `self_name`
/// (caller-supplied) substitution, or `None` for an event Messages shows
/// without one; `timestamp` is pre-formatted. `announcement` is the library's
/// [`Announcement`] verbatim.
///
/// `participant_name` exists only because the library reports
/// [`GroupAction::ParticipantAdded`] / [`GroupAction::ParticipantRemoved`] as
/// a `handle_id` (`i32`), and resolving that to a display name requires the
/// binary's [`Config`]. Pre-resolving here keeps the lookup out of the
/// per-format templates. Defaults to [`UNKNOWN_PARTICIPANT`] for every
/// non-participant variant (templates only read it on the participant arms).
pub struct ResolvedAnnouncement<'a> {
    pub timestamp: String,
    pub who: Option<&'a str>,
    pub announcement: Announcement<'a>,
    pub participant_name: &'a str,
}

/// Format-agnostic shape for the per-format announcement templates. `Action`
/// mirrors [`ResolvedAnnouncement`]'s fields so templates can destructure
/// directly; `Unknown` is the fallback emitted when [`resolve_announcement`]
/// returns `None`.
pub enum AnnouncementBody<'a> {
    Action {
        timestamp: String,
        /// Display name of the actor; `None` for an event Messages shows
        /// without one.
        who: Option<&'a str>,
        announcement: Announcement<'a>,
        /// Resolved display name for the participant in `ParticipantAdded`
        /// / `ParticipantRemoved`. Defaults to [`UNKNOWN_PARTICIPANT`] for
        /// non-participant variants (templates only read this on the
        /// participant arms).
        participant_name: &'a str,
    },
    Unknown,
}

impl<'a> From<ResolvedAnnouncement<'a>> for AnnouncementBody<'a> {
    fn from(r: ResolvedAnnouncement<'a>) -> Self {
        Self::Action {
            timestamp: r.timestamp,
            who: r.who,
            announcement: r.announcement,
            participant_name: r.participant_name,
        }
    }
}

/// Resolve a message's announcement into a [`ResolvedAnnouncement`].
/// Returns `None` when the message has no recognizable announcement.
/// `self_name` is the fallback when `who == ME` and `custom_name` is unset
/// (`"You"` in both current callers).
///
/// `who` is resolved in this order:
///
/// - `None` for an [`Announcement::UnknownEvent`]: no known producer writes
///   its codes, so nothing establishes an actor;
/// - `None` for an unattributed group event ([`Message::is_unattributed`]),
///   which Messages shows with no actor. [`GroupAction::ParticipantLeft`] and
///   [`GroupAction::PhoneNumberChanged`] ignore the flag: their actor is the
///   event's subject;
/// - the business for an RCS business's brand-logo update (see `brand_handle`);
/// - the message's sender.
pub fn resolve_announcement<'a>(
    msg: &'a Message,
    config: &'a Config,
    self_name: &'a str,
) -> Option<ResolvedAnnouncement<'a>> {
    let announcement = msg.get_announcement()?;

    let who = if matches!(announcement, Announcement::UnknownEvent(_))
        || is_unattributed(msg, &announcement)
    {
        None
    } else {
        let who = match brand_handle(msg, &announcement, config) {
            Some(handle) => config.who(Some(handle), false, &msg.destination_caller_id),
            None => config.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id),
        };
        Some(if who == ME {
            config.options.custom_name.as_deref().unwrap_or(self_name)
        } else {
            who
        })
    };

    let timestamp = format_message_date(msg, &config.options.timezone);

    let participant_name = match &announcement {
        Announcement::GroupAction(
            GroupAction::ParticipantAdded(handle) | GroupAction::ParticipantRemoved(handle),
        ) => config.who(Some(*handle), false, &msg.destination_caller_id),
        _ => UNKNOWN_PARTICIPANT,
    };

    Some(ResolvedAnnouncement {
        timestamp,
        who,
        announcement,
        participant_name,
    })
}

/// `true` when `announcement` is a group event `msg` marks as unattributed and
/// that has a form without an actor.
fn is_unattributed(msg: &Message, announcement: &Announcement) -> bool {
    msg.is_unattributed
        && matches!(
            announcement,
            Announcement::GroupAction(action)
                if !matches!(
                    action,
                    GroupAction::ParticipantLeft | GroupAction::PhoneNumberChanged(_)
                )
        )
}

/// Handle of the RCS business whose brand logo `msg` sets, or `None` when `msg`
/// is not a brand-logo update.
///
/// Messages stores a brand logo as a [`GroupAction::GroupIconChangedHidden`]
/// with no sender (`handle_id` `0` or `NULL`) and an [`Attachment::is_brand_logo`]
/// attachment; only the chat, whose sole participant is the business, names it.
/// The attachment query runs last; a query error yields `None`.
fn brand_handle(msg: &Message, announcement: &Announcement, config: &Config) -> Option<i32> {
    if !matches!(
        announcement,
        Announcement::GroupAction(GroupAction::GroupIconChangedHidden)
    ) || msg.handle_id.is_some_and(|id| id != 0)
    {
        return None;
    }
    let participants = config.chatroom_participants.get(&msg.chat_rowid()?)?;
    if participants.len() != 1 {
        return None;
    }
    let source = &config.data_source;
    let attachments = Attachment::from_message(source.db(), msg, &source.capabilities).ok()?;
    attachments
        .iter()
        .any(Attachment::is_brand_logo)
        .then_some(*participants.first()?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use imessage_database::{
        message_types::variants::{Announcement, KeptMessage},
        tables::messages::{Message, models::GroupAction},
    };

    use super::resolve_announcement;
    use crate::{
        Config, Options,
        app::{contacts::Name, export_type::ExportType},
    };

    fn make_config() -> Config {
        Config::fake_app(Options::fake_options(ExportType::Txt))
    }

    fn config_with_custom_name(name: &str) -> Config {
        let mut options = Options::fake_options(ExportType::Txt);
        options.custom_name = Some(name.to_string());
        Config::fake_app(options)
    }

    #[test]
    fn returns_none_when_message_is_not_an_announcement() {
        // Default fake_message has item_type=0 and no edited_parts, so
        // get_announcement() returns None.
        let config = make_config();
        let msg = Config::fake_message();
        assert!(resolve_announcement(&msg, &config, "You").is_none());
    }

    #[test]
    fn message_kept_has_no_participant_name() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.item_type = 5;
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::MessageKept(KeptMessage::Audio)
        ));
        assert_eq!(resolved.participant_name, super::UNKNOWN_PARTICIPANT);
    }

    #[test]
    fn from_me_without_custom_name_falls_back_to_self_name() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.is_from_me = true;
        msg.item_type = 5;
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("You"));
    }

    #[test]
    fn from_me_with_custom_name_uses_custom_name() {
        let config = config_with_custom_name("Chris");
        let mut msg = Config::fake_message();
        msg.is_from_me = true;
        msg.item_type = 5;
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Chris"));
    }

    #[test]
    fn participant_added_resolves_display_name() {
        let mut config = make_config();
        config.participants.insert(42, Name::fake_name("Alice"));
        config.real_participants.insert(42, 42);

        let mut msg = Config::fake_message();
        // ParticipantAdded: item_type=1, group_action_type=0,
        // other_handle=Some(target), handle_id != other_handle.
        msg.handle_id = Some(99);
        msg.item_type = 1;
        msg.group_action_type = 0;
        msg.other_handle = Some(42);

        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::ParticipantAdded(42))
        ));
        assert_eq!(resolved.participant_name, "Alice");
    }

    #[test]
    fn participant_removed_resolves_display_name() {
        let mut config = make_config();
        config.participants.insert(7, Name::fake_name("Bob"));
        config.real_participants.insert(7, 7);

        let mut msg = Config::fake_message();
        msg.handle_id = Some(99);
        msg.item_type = 1;
        msg.group_action_type = 1;
        msg.other_handle = Some(7);

        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::ParticipantRemoved(7))
        ));
        assert_eq!(resolved.participant_name, "Bob");
    }

    #[test]
    fn name_change_has_no_participant_name() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.item_type = 2;
        msg.group_title = Some("Trip 2026".to_string());
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::NameChange(_))
        ));
        assert_eq!(resolved.participant_name, super::UNKNOWN_PARTICIPANT);
    }

    /// Group event that `message_summary_info` marks unattributed.
    fn unattributed(item_type: i32, group_action_type: i32) -> Message {
        let mut msg = Config::fake_message();
        msg.item_type = item_type;
        msg.group_action_type = group_action_type;
        msg.handle_id = Some(0);
        msg.other_handle = Some(7);
        msg.group_title = Some("Trip 2026".to_string());
        msg.is_unattributed = true;
        msg
    }

    #[test]
    fn unattributed_group_events_have_no_actor() {
        let mut config = make_config();
        config.participants.insert(7, Name::fake_name("Bob"));
        config.real_participants.insert(7, 7);

        for (item_type, action) in [
            (1, 0),
            (1, 1),
            (2, 0),
            (3, 1),
            (3, 2),
            (3, 3),
            (3, 4),
            (3, 5),
            (3, 6),
            (3, 7),
        ] {
            for is_from_me in [false, true] {
                let mut msg = unattributed(item_type, action);
                msg.is_from_me = is_from_me;
                let resolved = resolve_announcement(&msg, &config, "You").unwrap();
                assert_eq!(resolved.who, None, "({item_type}, {action})");
            }
        }

        let msg = unattributed(1, 1);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.participant_name, "Bob");
    }

    #[test]
    fn unattributed_participant_left_keeps_actor() {
        let config = make_config();
        let mut msg = unattributed(3, 0);
        msg.is_from_me = true;
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::ParticipantLeft)
        ));
        assert_eq!(resolved.who, Some("You"));
    }

    #[test]
    fn unattributed_phone_number_change_keeps_actor() {
        let mut config = make_config();
        config.participants.insert(7, Name::fake_name("Bob"));
        config.real_participants.insert(7, 7);

        let mut msg = unattributed(1, 0);
        msg.handle_id = Some(7);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::PhoneNumberChanged(7))
        ));
        assert_eq!(resolved.who, Some("Bob"));
    }

    /// Config whose chat 761 has one participant, handle 5 (`Brand`), plus a
    /// non-participant handle 6 (`Alice`). TEMP tables shadow the read-only
    /// fixture's attachment tables to give message `rowid` 0 a
    /// `BrandLogoImage` attachment.
    fn brand_logo_config() -> Config {
        let mut config = make_config();
        config
            .data_source
            .db()
            .execute_batch(
                "CREATE TEMP TABLE attachment AS SELECT * FROM main.attachment WHERE 0;
                INSERT INTO attachment (ROWID, transfer_name) VALUES (1, 'BrandLogoImage');
                CREATE TEMP TABLE message_attachment_join AS
                    SELECT * FROM main.message_attachment_join WHERE 0;
                INSERT INTO message_attachment_join VALUES (0, 1);",
            )
            .unwrap();
        config
            .chatroom_participants
            .insert(761, BTreeSet::from([5]));
        for (handle, name) in [(5, "Brand"), (6, "Alice")] {
            config.participants.insert(handle, Name::fake_name(name));
            config.real_participants.insert(handle, handle);
        }
        config
    }

    /// Hidden group photo change in chat 761 with one attachment.
    fn hidden_group_photo(handle_id: i32) -> Message {
        let mut msg = Config::fake_message();
        msg.item_type = 3;
        msg.group_action_type = 3;
        msg.handle_id = Some(handle_id);
        msg.chat_id = Some(761);
        msg.num_attachments = 1;
        msg
    }

    #[test]
    fn brand_logo_names_sole_participant() {
        let config = brand_logo_config();
        let msg = hidden_group_photo(0);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Brand"));
    }

    #[test]
    fn hidden_group_photo_with_sender_keeps_sender() {
        let config = brand_logo_config();
        let msg = hidden_group_photo(6);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Alice"));
    }
}
