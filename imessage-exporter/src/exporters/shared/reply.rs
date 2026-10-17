use imessage_database::tables::messages::Message;

use crate::{
    app::error::RuntimeError,
    exporters::formatter::{MessageFormatter, PartBodyBuilder, RenderContext},
};

/// One reply, as fed to the format's `replies` template. `body` is already a
/// fully-rendered, format-safe payload; its concrete type `S` is chosen by
/// the calling format. `guid` is exposed for templates that need it (e.g. as
/// a per-reply anchor id); implementations that don't need it may ignore the
/// field.
pub(crate) struct ReplyEntry<S> {
    pub guid: String,
    pub body: S,
}

/// Render the tapbacks attached to `message[idx]`. Returns `None` when the
/// message has no tapbacks for this part *or* every tapback rendered empty
/// (e.g. all were [`TapbackAction::Removed`](imessage_database::message_types::variants::TapbackAction::Removed)).
/// `wrap` lifts each per-tapback rendered string into the format's payload
/// type.
pub(crate) fn build_tapbacks<'a, F, T>(
    formatter: &'a F,
    message: &'a Message,
    idx: usize,
    wrap: impl Fn(String) -> T,
) -> Result<Option<Vec<T>>, RuntimeError>
where
    F: MessageFormatter<'a> + PartBodyBuilder,
{
    let Some(tapbacks) = formatter
        .config()
        .tapbacks
        .get(&message.guid)
        .and_then(|m| m.get(&idx))
    else {
        return Ok(None);
    };

    let mut rendered = Vec::new();
    for tapback in tapbacks {
        let f = formatter.format_tapback(tapback)?;
        if !f.is_empty() {
            rendered.push(wrap(f));
        }
    }
    if rendered.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rendered))
    }
}

/// Render the replies threaded under a message part. Tapbacks in the reply
/// list are skipped (they render alongside their parent via `build_tapbacks`).
/// `buffer_capacity` is the format's `MessageWriter::BUFFER_CAPACITY` (used
/// to pre-allocate the per-reply scratch buffer). `wrap_body` lifts the
/// rendered reply body into the format's payload type.
pub(crate) fn build_replies<'a, F, S>(
    formatter: &'a F,
    replies: Option<&'a mut Vec<Message>>,
    buffer_capacity: usize,
    wrap_body: impl Fn(String) -> S,
) -> Result<Option<Vec<ReplyEntry<S>>>, RuntimeError>
where
    F: MessageFormatter<'a> + PartBodyBuilder,
{
    let Some(replies) = replies else {
        return Ok(None);
    };

    let mut rendered = Vec::new();
    for reply in replies.iter() {
        if !reply.is_tapback() {
            let mut buf = String::with_capacity(buffer_capacity);
            formatter.format_message_into(reply, RenderContext::Reply, &mut buf)?;
            rendered.push(ReplyEntry {
                guid: reply.guid.clone(),
                body: wrap_body(buf),
            });
        }
    }
    if rendered.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rendered))
    }
}
