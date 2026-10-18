use imessage_database::{
    message_types::edited::{EditStatus, EditedMessage},
    tables::{
        messages::{Message, models::BubbleComponent},
        table::ME,
    },
    util::dates::{readable_diff, to_utc},
};

use crate::app::runtime::Config;

/// Format-agnostic shape for the per-format edit-history templates. `Edited`
/// carries a format-specific row type (`R`) so each exporter can compute the
/// exact fields its template needs.
pub enum Edit<'a, R> {
    Edited {
        rows: Vec<R>,
    },
    /// `elapsed` carries the human-readable duration between send and unsend
    /// (e.g. `"49 seconds"`) when both timestamps are available;
    /// `None` falls back to a duration-less phrasing.
    Unsent {
        who: &'a str,
        elapsed: Option<String>,
    },
}

impl<'a, R> Edit<'a, R> {
    /// Convert each row via `f`, preserving the variant shape. Used to map
    /// the format-neutral [`NormalizedEditEvent`]s produced by
    /// [`normalize_edited`] into a per-exporter row type.
    pub fn map_rows<R2>(self, f: impl FnMut(R) -> R2) -> Edit<'a, R2> {
        match self {
            Edit::Edited { rows } => Edit::Edited {
                rows: rows.into_iter().map(f).collect(),
            },
            Edit::Unsent { who, elapsed } => Edit::Unsent { who, elapsed },
        }
    }
}

/// Resolve the display name for the actor of an [`EditStatus::Unsent`] event.
pub fn resolve_unsent_actor<'a>(
    msg: &'a Message,
    config: &'a Config,
    self_name: &'a str,
) -> &'a str {
    let who = config.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
    if who == ME {
        config.options.custom_name.as_deref().unwrap_or(self_name)
    } else {
        who
    }
}

/// Relative position of an edit event in the edit history.
pub enum EditDiff {
    /// First event in the history.
    First,
    /// Diff couldn't be computed (invalid timestamps or `readable_diff`
    /// returned `None`). Renderers typically suppress the prefix in this case.
    Failed,
    /// Human-readable diff between this event and the preceding one
    /// (e.g. `"3 minutes"`).
    Computed(String),
}

/// Pre-computed view of one entry in an edit history. Carries both the raw
/// fields a renderer needs (`text`, `components`) and the derived values
/// callers would otherwise have to compute themselves (`diff_since_previous`,
/// `date`, `is_last`).
pub struct NormalizedEditEvent<'a> {
    /// `true` for the final emitted event in the history.
    /// Implementations may use this to switch trailing markup (e.g. table
    /// footer vs body row).
    pub is_last: bool,
    /// Reference to `event.text`.
    pub text: &'a str,
    /// `event.components` passed through for renderers that need to format
    /// the original text attributes.
    pub components: &'a [BubbleComponent],
    /// Position of this event relative to the preceding one.
    /// See [`EditDiff`].
    pub diff_since_previous: EditDiff,
    /// Raw iMessage timestamp for this event. Renderers that need the
    /// absolute time format it via
    /// [`format_timestamp`](super::time::format_timestamp); renderers that
    /// only consume `diff_since_previous` can ignore this field.
    pub date: i64,
}

/// Normalize one [`EditedMessagePart`](imessage_database::message_types::edited::EditedMessagePart) into the template-facing
/// [`Edit`], parameterized by [`NormalizedEditEvent`] so callers can
/// `.map_rows(...)` into their format-specific row type. Returns `None` for
/// [`EditStatus::Original`] (nothing to render). `self_name` is the fallback
/// label for the unsent actor when the message is from `ME` and no custom
/// name is set (`"You"` in both current callers).
pub fn normalize_edited<'a>(
    msg: &'a Message,
    edited: &'a EditedMessage,
    part_idx: usize,
    config: &'a Config,
    self_name: &'a str,
) -> Option<Edit<'a, NormalizedEditEvent<'a>>> {
    let part = edited.part(part_idx)?;
    let zone = &config.options.timezone;
    match part.status {
        EditStatus::Original => None,
        EditStatus::Edited => {
            let mut rows = Vec::with_capacity(part.edit_history.len());
            let mut previous_timestamp: Option<i64> = None;
            for event in &part.edit_history {
                let diff_since_previous = match previous_timestamp {
                    None => EditDiff::First,
                    Some(prev) => match (to_utc(prev), to_utc(event.date)) {
                        (Ok(start), Ok(end)) => {
                            readable_diff(&start.with_timezone(zone), &end.with_timezone(zone))
                                .map(EditDiff::Computed)
                                .unwrap_or(EditDiff::Failed)
                        }
                        _ => EditDiff::Failed,
                    },
                };
                previous_timestamp = Some(event.date);
                rows.push(NormalizedEditEvent {
                    is_last: false,
                    text: &event.text,
                    components: &event.components,
                    diff_since_previous,
                    date: event.date,
                });
            }
            if let Some(last) = rows.last_mut() {
                last.is_last = true;
            }
            Some(Edit::Edited { rows })
        }
        EditStatus::Unsent => {
            let elapsed = msg
                .date()
                .ok()
                .zip(msg.date_edited().ok())
                .and_then(|(s, e)| readable_diff(&s.with_timezone(zone), &e.with_timezone(zone)));
            let who = resolve_unsent_actor(msg, config, self_name);
            Some(Edit::Unsent { who, elapsed })
        }
    }
}

#[cfg(test)]
mod tests {
    use imessage_database::message_types::edited::{
        EditStatus, EditedEvent, EditedMessage, EditedMessagePart,
    };

    use super::{Edit, EditDiff, normalize_edited, resolve_unsent_actor};
    use crate::{
        Config, Options,
        app::{contacts::Name, export_type::ExportType},
    };

    // May 17, 2022  8:29:42 PM
    const DATE_A: i64 = 674526582885055488;
    // 1 hour, 49 seconds later
    const DATE_B: i64 = 674530231992568192;

    fn make_config() -> Config {
        Config::fake_app(Options::fake_options(ExportType::Txt))
    }

    fn event(date: i64, text: &str) -> EditedEvent {
        EditedEvent {
            date,
            text: text.to_string(),
            components: vec![],
            guid: None,
        }
    }

    #[test]
    fn normalize_returns_none_for_original_status() {
        let config = make_config();
        let msg = Config::fake_message();
        let edited = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Original,
                edit_history: vec![],
            }],
        };
        assert!(normalize_edited(&msg, &edited, 0, &config, "You").is_none());
    }

    #[test]
    fn normalize_returns_none_for_out_of_range_part_index() {
        let config = make_config();
        let msg = Config::fake_message();
        let edited = EditedMessage { parts: vec![] };
        assert!(normalize_edited(&msg, &edited, 0, &config, "You").is_none());
    }

    #[test]
    fn normalize_unsent_carries_readable_diff() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.date = DATE_A;
        msg.date_edited = DATE_B;
        let edited = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        };
        match normalize_edited(&msg, &edited, 0, &config, "You") {
            Some(Edit::Unsent {
                elapsed: Some(elapsed),
                ..
            }) => {
                assert_eq!(elapsed, "1 hour, 49 seconds");
            }
            Some(Edit::Unsent { elapsed: None, .. }) => {
                panic!("expected a computed elapsed duration, got None")
            }
            _ => panic!("expected Unsent"),
        }
    }

    #[test]
    fn normalize_unsent_elapsed_is_none_when_date_edited_missing() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.date = DATE_A;
        // date_edited defaults to 0; readable_diff can't compute a duration
        // against the iMessage epoch.
        let edited = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        };
        assert!(matches!(
            normalize_edited(&msg, &edited, 0, &config, "You"),
            Some(Edit::Unsent { elapsed: None, .. })
        ));
    }

    #[test]
    fn normalize_edited_single_event_is_first_and_last() {
        let config = make_config();
        let msg = Config::fake_message();
        let edited = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![event(DATE_A, "hi")],
            }],
        };
        match normalize_edited(&msg, &edited, 0, &config, "You") {
            Some(Edit::Edited { rows }) => {
                assert_eq!(rows.len(), 1);
                assert!(rows[0].is_last, "single event must be is_last");
                assert!(matches!(rows[0].diff_since_previous, EditDiff::First));
                assert_eq!(rows[0].text, "hi");
            }
            _ => panic!("expected Edited"),
        }
    }

    #[test]
    fn normalize_edited_multiple_events_diffs_against_previous() {
        let config = make_config();
        let msg = Config::fake_message();
        let edited = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![event(DATE_A, "first"), event(DATE_B, "second")],
            }],
        };
        match normalize_edited(&msg, &edited, 0, &config, "You") {
            Some(Edit::Edited { rows }) => {
                assert_eq!(rows.len(), 2);
                assert!(!rows[0].is_last);
                assert!(rows[1].is_last);
                assert!(matches!(rows[0].diff_since_previous, EditDiff::First));
                match &rows[1].diff_since_previous {
                    EditDiff::Computed(diff) => assert_eq!(diff, "1 hour, 49 seconds"),
                    _ => panic!("expected Computed diff for second event"),
                }
            }
            _ => panic!("expected Edited"),
        }
    }

    #[test]
    fn resolve_unsent_actor_self_default_falls_back_to_self_name() {
        let config = make_config();
        let mut msg = Config::fake_message();
        msg.is_from_me = true;
        assert_eq!(resolve_unsent_actor(&msg, &config, "You"), "You");
    }

    #[test]
    fn resolve_unsent_actor_self_with_custom_name_uses_custom_name() {
        let mut config = make_config();
        config.options.custom_name = Some("Chris".to_string());
        let mut msg = Config::fake_message();
        msg.is_from_me = true;
        assert_eq!(resolve_unsent_actor(&msg, &config, "You"), "Chris");
    }

    #[test]
    fn resolve_unsent_actor_self_with_use_caller_id_returns_caller_id() {
        let mut config = make_config();
        config.options.use_caller_id = true;
        let mut msg = Config::fake_message();
        msg.is_from_me = true;
        msg.destination_caller_id = Some("+15551234567".to_string());
        assert_eq!(resolve_unsent_actor(&msg, &config, "You"), "+15551234567");
    }

    #[test]
    fn resolve_unsent_actor_other_resolves_participant() {
        let mut config = make_config();
        config.participants.insert(7, Name::fake_name("Alice"));
        config.real_participants.insert(7, 7);
        let mut msg = Config::fake_message();
        msg.is_from_me = false;
        msg.handle_id = Some(7);
        assert_eq!(resolve_unsent_actor(&msg, &config, "You"), "Alice");
    }
}
