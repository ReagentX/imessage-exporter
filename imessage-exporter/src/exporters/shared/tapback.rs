use imessage_database::{
    message_types::variants::{Tapback, TapbackAction, Variant},
    tables::{attachment::Attachment, messages::Message},
};

use crate::app::{error::RuntimeError, runtime::Config};

/// Format-agnostic shape for tapback rendering. The `payload` carried by
/// [`Sticker`] is generic so each exporter can use its own pre-rendered type.
///
/// [`Sticker`]: TapbackKind::Sticker
pub enum TapbackKind<'a, S> {
    /// Standard reaction.
    Reaction { tapback: Tapback<'a>, who: &'a str },
    /// Sticker tapback whose attachment was found and rendered.
    Sticker { payload: S, who: &'a str },
    /// Sticker tapback whose attachment is missing.
    StickerMissing { who: &'a str },
}

/// Resolve a tapback message into the format-agnostic [`TapbackKind`].
/// Returns `Ok(None)` for [`TapbackAction::Removed`] so the caller can
/// render an empty string without re-deriving the variant.
///
/// `sticker_renderer` lifts a found sticker attachment into the format's
/// payload type. The closure captures the formatter `self`
/// and `msg` so its body can call back into [`format_sticker`].
///
/// [`format_sticker`]: crate::exporters::formatter::MessageFormatter::format_sticker
///
/// # Panics
///
/// Panics if `msg.variant()` is not [`Variant::Tapback`]. Calling code is
/// expected to dispatch off the variant before calling this helper.
pub(crate) fn resolve_tapback<'a, S>(
    msg: &'a Message,
    config: &'a Config,
    sticker_renderer: impl FnOnce(&mut Attachment) -> S,
) -> Result<Option<TapbackKind<'a, S>>, RuntimeError> {
    let Variant::Tapback(_, action, tapback) = msg.variant() else {
        unreachable!(
            "resolve_tapback called with non-Tapback variant: {:?}",
            msg.variant()
        )
    };
    if let TapbackAction::Removed = action {
        return Ok(None);
    }
    let who = config.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
    let kind = match tapback {
        Tapback::Sticker => {
            let mut paths = Attachment::from_message(
                config.data_source.db(),
                msg,
                &config.data_source.capabilities,
            )?;
            match paths.get_mut(0) {
                Some(sticker) => TapbackKind::Sticker {
                    payload: sticker_renderer(sticker),
                    who,
                },
                None => TapbackKind::StickerMissing { who },
            }
        }
        other => TapbackKind::Reaction {
            tapback: other,
            who,
        },
    };
    Ok(Some(kind))
}
