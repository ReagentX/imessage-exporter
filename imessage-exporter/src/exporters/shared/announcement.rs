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

/// Format-agnostic announcement data from [`resolve_announcement`]:
///
/// - `who`: the actor's display name after the `ME` → caller-supplied
///   `self_name` substitution, or `None` for an event displayed without an
///   actor in Messages;
/// - `timestamp`: the formatted timestamp, followed by ` (Failed)` for a
///   failed group event ([`Message::is_failed`]);
/// - `announcement`: the library's [`Announcement`], unchanged.
///
/// For [`GroupAction::ParticipantAdded`] / [`GroupAction::ParticipantRemoved`],
/// resolve the `handle_id` (`i32`) to a display name using the binary's
/// [`Config`] before formatting to avoid lookups in per-format templates.
/// Use [`UNKNOWN_PARTICIPANT`] for every other variant; read `participant_name`
/// only in the participant template arms.
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
        /// Actor's display name; `None` for an event displayed without an
        /// actor in Messages.
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
/// Return `None` for a message with no recognizable announcement.
/// `self_name` is the fallback when `who == ME` and `custom_name` is unset
/// (`"You"` in both current callers).
///
/// Resolve `who` in this order:
///
/// - `None` for an [`Announcement::UnknownEvent`]: the actor is unknown
///   because no producer is known for these codes;
/// - `None` for [`Announcement::EmergencyLocationSent`]: no actor in Apple's
///   only wording for this event;
/// - `None` for an unattributed group event ([`Message::is_unattributed`]),
///   displayed with no actor in Messages. For [`GroupAction::ParticipantLeft`]
///   and [`GroupAction::PhoneNumberChanged`], use the event's subject as its
///   actor regardless of the flag;
/// - the business for an RCS business's brand-logo update (see `brand_handle`);
/// - the database owner, with their usual message display name, for a
///   [`GroupAction::ParticipantLeft`] sent by an Apple Messages for Business
///   handle (see `is_business_leave`);
/// - the message's sender.
pub fn resolve_announcement<'a>(
    msg: &'a Message,
    config: &'a Config,
    self_name: &'a str,
) -> Option<ResolvedAnnouncement<'a>> {
    let announcement = msg.get_announcement()?;

    let who = if matches!(
        announcement,
        Announcement::UnknownEvent(_) | Announcement::EmergencyLocationSent
    ) || is_unattributed(msg, &announcement)
    {
        None
    } else {
        let who = match brand_handle(msg, &announcement, config) {
            Some(handle) => config.who(Some(handle), false, &msg.destination_caller_id),
            None => config.who(
                msg.handle_id,
                msg.is_from_me() || is_business_leave(msg, &announcement, config),
                &msg.destination_caller_id,
            ),
        };
        Some(if who == ME {
            config.options.custom_name.as_deref().unwrap_or(self_name)
        } else {
            who
        })
    };

    let mut timestamp = format_message_date(msg, &config.options.timezone);
    if msg.is_failed() {
        timestamp.push_str(" (Failed)");
    }

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

/// `true` for a group event marked as unattributed in `msg` and displayable
/// without an actor.
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

/// `true` for a [`GroupAction::ParticipantLeft`] whose sender is an Apple
/// Messages for Business handle.
///
/// For business chat session closures, the sender is the business conversation
/// ID. Use the database owner as the actor to match Messages.
///
/// Require every `handle.id` in the sender's group to be a business ID
/// ([`is_business_id`]). In [`Config`], we store addresses per group, so retain
/// the sender as the actor when grouped with any other address type. Such
/// mixed groups are possible only through a shared `person_centric_id`;
/// business IDs cannot be matched through Contacts.
fn is_business_leave(msg: &Message, announcement: &Announcement, config: &Config) -> bool {
    matches!(
        announcement,
        Announcement::GroupAction(GroupAction::ParticipantLeft)
    ) && msg
        .handle_id
        .and_then(|handle_id| config.resolve_participant(handle_id))
        .is_some_and(|name| {
            !name.addresses.is_empty() && name.addresses.iter().all(|id| is_business_id(id))
        })
}

/// `true` for an Apple Messages for Business ID: a `handle.id` value with an
/// `urn:biz:` or `biz:` prefix, ignoring ASCII case.
fn is_business_id(id: &str) -> bool {
    ["urn:biz:", "biz:"].iter().any(|prefix| {
        id.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

/// Return the RCS business's handle for a brand-logo update in `msg`, or
/// `None` for other messages.
///
/// In Messages, brand logos are stored as [`GroupAction::GroupIconChangedHidden`]
/// events with no sender (`handle_id` `0` or `NULL`) and an
/// [`Attachment::is_brand_logo`] attachment. Identify the business from the
/// chat's sole participant. Query attachments last; return `None` on query errors.
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

    use super::{is_business_id, resolve_announcement};
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

    /// Group event marked as unattributed in `message_summary_info`.
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

    /// Config with handle 5 (`Brand`) as chat 761's sole participant and
    /// handle 6 (`Alice`) as a non-participant. Use TEMP tables in place of
    /// the read-only fixture's attachment tables to associate a
    /// `BrandLogoImage` attachment with message `rowid` 0.
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

    #[test]
    fn business_id_prefixes() {
        for id in [
            "urn:biz:0000",
            "biz:0000",
            "URN:BIZ:0000",
            "Urn:Biz:0000",
            "BIZ:0000",
        ] {
            assert!(is_business_id(id), "{id}");
        }
    }

    #[test]
    fn non_business_ids() {
        for id in [
            "",
            "biz",
            "urn:biz",
            "+15555550100",
            "person@biz:example.com",
            "x-urn:biz:0000",
            " biz:0000",
            "urn:bi\u{0130}z:0000",
        ] {
            assert!(!is_business_id(id), "{id}");
        }
    }

    /// Config with handle 5 as an Apple Messages for Business handle named
    /// `Business` and handle 6 as `Alice`.
    fn business_config(options: Options) -> Config {
        let mut config = Config::fake_app(options);
        let mut business = Name::fake_name("Business");
        business.addresses = BTreeSet::from(["urn:biz:0000".to_string()]);
        config.participants.insert(5, business);
        config.real_participants.insert(5, 5);
        let mut alice = Name::fake_name("Alice");
        alice.addresses = BTreeSet::from(["+15555550100".to_string()]);
        config.participants.insert(6, alice);
        config.real_participants.insert(6, 6);
        config
    }

    /// `ParticipantLeft` row sent by `handle_id`.
    fn left(handle_id: i32) -> Message {
        let mut msg = Config::fake_message();
        msg.item_type = 3;
        msg.group_action_type = 0;
        msg.handle_id = Some(handle_id);
        msg
    }

    #[test]
    fn business_leave_resolves_to_database_owner() {
        let config = business_config(Options::fake_options(ExportType::Txt));
        let msg = left(5);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("You"));

        let mut options = Options::fake_options(ExportType::Txt);
        options.custom_name = Some("Chris".to_string());
        let config = business_config(options);
        let msg = left(5);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Chris"));
    }

    #[test]
    fn business_leave_needs_every_group_address_to_be_business() {
        let mut config = business_config(Options::fake_options(ExportType::Txt));
        config
            .participants
            .get_mut(&5)
            .unwrap()
            .addresses
            .insert("+15555550101".to_string());
        let msg = left(5);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Business"));
    }

    #[test]
    fn non_business_leave_keeps_sender() {
        let config = business_config(Options::fake_options(ExportType::Txt));
        let msg = left(6);
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert_eq!(resolved.who, Some("Alice"));
    }

    #[test]
    fn business_sender_on_other_group_action_keeps_sender() {
        let config = business_config(Options::fake_options(ExportType::Txt));
        let mut msg = left(5);
        msg.group_action_type = 2;
        let resolved = resolve_announcement(&msg, &config, "You").unwrap();
        assert!(matches!(
            resolved.announcement,
            Announcement::GroupAction(GroupAction::GroupIconRemoved)
        ));
        assert_eq!(resolved.who, Some("Business"));
    }
}
