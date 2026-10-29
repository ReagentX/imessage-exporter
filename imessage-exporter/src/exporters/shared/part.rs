use std::collections::HashMap;

use imessage_database::tables::{
    attachment::Attachment,
    messages::{
        Message,
        models::{AttributedRange, BubbleComponent},
    },
};

use crate::exporters::formatter::{MessageFormatter, PartBodyBuilder};

/// Resolves a message's body attachment ranges to indices into its resolved
/// attachment list, preferring the file-transfer GUID.
///
/// The body lists attachment placeholders in display order, but
/// [`Attachment::from_message`](imessage_database::tables::attachment::Attachment::from_message)
/// returns rows in the join's (unspecified) order, so positional pairing can
/// mis-order a message with several attachments. Matching by GUID keeps every
/// placeholder bound to its own attachment regardless of join order, and the
/// resolved attachments always carry a GUID from the database column, so this is the
/// path for any typedstream-parsed body.
///
/// Ranges that carry no GUID (only the legacy (non-typedstream)
/// `parse_body_legacy` path, whose placeholders have no identity to match) fall
/// back to positional order. The resolver is built once per message and advanced
/// as those fallback ranges are consumed.
pub(crate) struct AttachmentResolver {
    /// `guid → index` into the message's resolved attachments.
    by_guid: HashMap<String, usize>,
    /// Cursor for the positional fallback (GUID-less legacy ranges only).
    next_positional: usize,
}

impl AttachmentResolver {
    pub(crate) fn new(attachments: &[Attachment]) -> Self {
        Self {
            by_guid: attachments
                .iter()
                .enumerate()
                .filter_map(|(i, a)| a.guid.clone().map(|g| (g, i)))
                .collect(),
            next_positional: 0,
        }
    }

    /// Resolve one attachment range to an index into the attachment list.
    /// Prefers the range's GUID; on a miss (or a GUID-less legacy range)
    /// consumes the next positional slot. Call exactly once per attachment
    /// range, in body order. The returned index may be out of bounds (a
    /// dangling placeholder); callers bounds-check via `attachments.get(idx)`.
    pub(crate) fn resolve(&mut self, range: &AttributedRange) -> usize {
        if let Some(idx) = range
            .attachment
            .as_ref()
            .and_then(|meta| meta.guid.as_deref())
            .and_then(|guid| self.by_guid.get(guid).copied())
        {
            return idx;
        }
        let idx = self.next_positional;
        self.next_positional += 1;
        idx
    }
}

/// Pair every range of a run with its resolved attachment index, in body order.
///
/// Text ranges (no attachment) yield `None`; attachment ranges yield
/// `Some(index)` into the message's resolved attachment list, advancing
/// `resolver` exactly once per attachment range.
pub(crate) fn resolve_run<'r>(
    ranges: &'r [AttributedRange],
    resolver: &mut AttachmentResolver,
) -> Vec<(&'r AttributedRange, Option<usize>)> {
    ranges
        .iter()
        .map(|range| {
            let idx = range.attachment.is_some().then(|| resolver.resolve(range));
            (range, idx)
        })
        .collect()
}

/// Walks `message_part` and produces the format's part-body. Owns the
/// format-agnostic control flow:
///
///  - run-vs-app-vs-retracted branching
///  - the part-edited / Retracted edit-history dance
///
/// A plain (non-edited) [`Run`](BubbleComponent::Run)–a bubble's worth of
/// attributed ranges–is delegated to the format's
/// [`MessageFormatter::render_run`], which interleaves text and inline
/// attachments, resolves them via `resolver`, and applies translation. App and
/// Retracted leaves are wrapped via the format's [`PartBodyBuilder`] impl.
pub(crate) fn dispatch_part_body<'a, F>(
    formatter: &'a F,
    message: &'a Message,
    idx: usize,
    message_part: &'a BubbleComponent,
    attachments: &'a [Attachment],
    resolver: &mut AttachmentResolver,
) -> F::Body
where
    F: MessageFormatter<'a> + PartBodyBuilder,
{
    match message_part {
        BubbleComponent::Run(ranges) => {
            // An edited part renders its edit history in place of the live body.
            if message.is_part_edited(idx) {
                return match &message.edited_parts {
                    Some(edited_parts) => match formatter.format_edited(
                        message,
                        edited_parts,
                        idx,
                        attachments,
                        resolver,
                    ) {
                        Some(rendered) => formatter.body_text_edited(rendered),
                        None => formatter.body_empty(),
                    },
                    None => formatter.body_empty(),
                };
            }
            formatter.render_run(message, ranges, attachments, resolver)
        }
        BubbleComponent::App => match formatter.format_app(message, attachments) {
            Ok(content) => formatter.body_app(content),
            Err(why) => formatter.body_app_error(message, why.to_string()),
        },
        BubbleComponent::Retracted => match &message.edited_parts {
            Some(edited_parts) => {
                match formatter.format_edited(message, edited_parts, idx, attachments, resolver) {
                    Some(content) => formatter.body_retracted(content),
                    None => formatter.body_empty(),
                }
            }
            None => formatter.body_empty(),
        },
    }
}
