use std::{
    borrow::Cow,
    cmp::{
        Ordering::{Equal, Greater, Less},
        min,
    },
    fs::File,
    io::{BufWriter, Write},
};

use crate::{
    app::{error::RuntimeError, runtime::Config, sanitizers::sanitize_html},
    exporters::{
        formatter::{
            AttachmentRender, MessageFormatter, PartBodyBuilder, RenderContext, TextEffectFormatter,
        },
        shared::{
            announcement::{AnnouncementBody, resolve_announcement},
            attachment::prepare_attachment,
            balloon::dispatch_app_balloon,
            driver::{ExportState, MessageWriter},
            edited::{Edit, EditDiff, normalize_edited},
            message::MessageContext,
            part::{AttachmentResolver, dispatch_part_body, resolve_run},
            render::{render_template, render_template_into},
            reply::{build_replies, build_tapbacks},
            tapback::resolve_tapback,
            time::message_time,
        },
    },
};

use imessage_database::{
    message_types::{
        edited::EditedMessage, sticker::StickerDecoration, text_effects::text_effect::TextEffect,
        variants::Announcement,
    },
    tables::{
        attachment::{Attachment, MediaType},
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, BubbleComponent, SharedLocation},
        },
        table::YOU,
    },
};

mod balloons;
mod jumbomoji;
mod safe;
mod text_effects;
mod view_model;

use safe::Html;
use view_model::{
    AnnouncementInnerVM, AttachmentVM, AttachmentVariant, EditedRow, EditedVM, GlyphSize,
    InlineSegment, MessagePartVM, MessageVM, PartBody, RepliesVM, ReplyAnchorKind, StickerInlineVM,
    StickerSuffixVM, TapbackVM, TapbacksVM,
};

// MARK: HTML
const HEADER: &str = "<html>\n<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
const FOOTER: &str = "</body></html>";
const STYLE: &str = include_str!("resources/style.css");
/// Inline placeholder for an attachment range whose row is absent
const MISSING_INLINE_ATTACHMENT: &str =
    "<span class=\"attachment_error\">Attachment does not exist!</span>";

#[derive(Debug, Clone)]
/// [`EventType`] is used to track the start and end of HTML text attributes
/// so we can render them correctly in the HTML output.
enum EventType<'a> {
    /// Start event for text attributes, contains the index of the attribute
    Start(usize, &'a [TextEffect]),
    /// End event for text attributes, contains the index of the attribute
    End(usize),
}

pub struct HTML<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Shared per-export state (file cache, orphaned writer, progress bar).
    pub state: ExportState,
}

impl<'a> HTML<'a> {
    pub fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(HTML {
            config,
            state: ExportState::new(config, "html")?,
        })
    }
}

// MARK: Driver hooks
impl<'a> MessageWriter<'a> for HTML<'a> {
    const LABEL: &'static str = "html";
    const BUFFER_CAPACITY: usize = 2048;

    fn config(&self) -> &'a Config {
        self.config
    }

    fn state(&self) -> &ExportState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ExportState {
        &mut self.state
    }

    fn write_file_header(file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        HTML::write_headers(file)
    }

    fn write_file_footer(file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        file.write_all(FOOTER.as_bytes())?;
        Ok(())
    }

    fn footer_notice() -> Option<&'static str> {
        Some("Writing HTML footers...")
    }
}

// MARK: Writer
impl<'a> MessageFormatter<'a> for HTML<'a> {
    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        message: &Message,
        metadata: &AttachmentMeta,
    ) -> AttachmentRender {
        if let Err(render) = prepare_attachment(self.config, &self.state, attachment, message) {
            return render;
        }

        let embed_path = self.config.message_attachment_path(attachment);

        let variant = match attachment.mime_type() {
            MediaType::Image(_) => AttachmentVariant::Image,
            // Video duplicates the source tag intentionally; see
            // https://github.com/ReagentX/imessage-exporter/issues/73
            MediaType::Video(media_type) => AttachmentVariant::Video { media_type },
            MediaType::Audio(media_type) => match metadata.transcription.as_deref() {
                Some(transcription) => AttachmentVariant::AudioTranscription {
                    media_type,
                    transcription,
                },
                None => AttachmentVariant::Audio { media_type },
            },
            MediaType::Text(_) | MediaType::Application(_) => {
                let Some(filename) = attachment.filename() else {
                    return AttachmentRender::MissingFilename;
                };
                AttachmentVariant::Download {
                    filename,
                    file_size: attachment.file_size(),
                }
            }
            MediaType::Unknown => {
                if attachment
                    .copied_path
                    .as_ref()
                    .is_some_and(|path| path.is_dir())
                {
                    let Some(filename) = attachment.filename() else {
                        return AttachmentRender::MissingFilename;
                    };
                    AttachmentVariant::UnknownFolder {
                        filename,
                        file_size: attachment.file_size(),
                    }
                } else {
                    AttachmentVariant::UnknownOther {
                        file_size: attachment.file_size(),
                    }
                }
            }
            MediaType::Other(media_type) => AttachmentVariant::Other { media_type },
        };

        AttachmentRender::Embedded(render_template(&AttachmentVM {
            lazy: !self.config.options.no_lazy,
            embed_path,
            variant,
        }))
    }

    fn format_sticker(&self, sticker: &'a mut Attachment, message: &Message) -> String {
        let mut sticker_embed =
            match self.format_attachment(sticker, message, &AttachmentMeta::default()) {
                AttachmentRender::Embedded(html) => html,
                AttachmentRender::MissingFilename => return String::new(),
                AttachmentRender::NamedFile(name) => return sanitize_html(&name).into_owned(),
            };

        if let Some(kind) = sticker.get_sticker_decoration(
            self.config.data_source.db(),
            &self.config.options.platform,
            &self.config.options.db_path,
            self.config.options.attachment_root.as_deref(),
        ) {
            let suffix_html = render_template(&StickerSuffixVM {
                class: sticker_decoration_class(&kind),
                label: sticker_decoration_label(&kind),
            });
            sticker_embed.push_str(&suffix_html);
        }

        sticker_embed
    }

    fn format_app(
        &self,
        message: &'a Message,
        attachments: &mut Vec<Attachment>,
    ) -> Result<String, RuntimeError> {
        dispatch_app_balloon(self, message, attachments, self.config)
    }

    fn format_tapback(&self, msg: &Message) -> Result<String, RuntimeError> {
        let Some(kind) = resolve_tapback(msg, self.config, |sticker| {
            Html::trust(self.format_sticker(sticker, msg))
        })?
        else {
            return Ok(String::new());
        };
        Ok(render_template(&TapbackVM { kind }))
    }

    fn format_announcement(&self, msg: &Message, out: &mut String) {
        let (kind, wrap_newlines) = match resolve_announcement(msg, self.config, YOU) {
            None => (AnnouncementBody::Unknown, true),
            Some(resolved) => {
                let wrap = !matches!(resolved.announcement, Announcement::FullyUnsent);
                (resolved.into(), wrap)
            }
        };

        if wrap_newlines {
            out.push('\n');
        }
        render_template_into(&AnnouncementInnerVM { kind }, out);
        if wrap_newlines {
            out.push('\n');
        }
    }

    fn format_shareplay(&self) -> &'static str {
        "<hr>SharePlay Message Ended"
    }

    fn format_shared_location(&self, kind: SharedLocation) -> &'static str {
        match kind {
            SharedLocation::Started => "<hr>Started sharing location!",
            SharedLocation::Stopped => "<hr>Stopped sharing location!",
        }
    }

    fn format_edited(
        &'a self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        attachments: &'a mut Vec<Attachment>,
        resolver: &mut AttachmentResolver,
    ) -> Option<String> {
        let normalized = normalize_edited(msg, edited_message, message_part_idx, self.config, YOU)?;
        // Build rows in a direct loop rather than `map_rows`: rendering an inline
        // sticker needs a `&'a mut Attachment`, which a closure capture can't
        // hold across iterations.
        let kind = match normalized {
            Edit::Unsent { who, elapsed } => Edit::Unsent { who, elapsed },
            Edit::Edited { rows } => {
                let mut out = Vec::with_capacity(rows.len());
                for event in rows {
                    let rendered_text = match event.components.first() {
                        // A run carrying an inline sticker interleaves text ranges
                        // with glyph-sized `<img>`s, exactly as `render_run` does
                        // for live messages so an edited Memoji/genmoji renders
                        // as its image, not a bare `\u{FFFC}` placeholder.
                        Some(BubbleComponent::Run(ranges))
                            if ranges
                                .iter()
                                .any(|range| range.attachment.is_some() && range.emoji_image) =>
                        {
                            // Edit-history components always come from
                            // `parse_body_typedstream`, so each attachment range
                            // carries a file-transfer GUID and `resolve` is
                            // idempotent (a GUID lookup, not a positional-cursor
                            // advance). That keeps this per-row resolve safe
                            // despite the resolver's "once per range" contract: a
                            // GUID-less range repeated across rows would otherwise
                            // drift the positional cursor.
                            inline_segments_to_html(self.interleave_segments(
                                event.text,
                                ranges,
                                msg,
                                attachments,
                                resolver,
                            ))
                        }
                        Some(BubbleComponent::Run(ranges)) => {
                            self.format_attributes(event.text, ranges)
                        }
                        _ => sanitize_html(event.text).into_owned(),
                    };
                    let timestamp = match event.diff_since_previous {
                        EditDiff::First => String::new(),
                        EditDiff::Failed => "Edited later".to_string(),
                        EditDiff::Computed(diff) => format!("Edited {diff} later"),
                    };
                    out.push(EditedRow {
                        is_last: event.is_last,
                        timestamp,
                        text_html: Html::trust(rendered_text),
                    });
                }
                Edit::Edited { rows: out }
            }
        };

        Some(render_template(&EditedVM { kind }))
    }

    fn format_attributes(&self, text: &str, ranges: &[AttributedRange]) -> String {
        if ranges.is_empty() {
            return sanitize_html(text).into_owned();
        }

        let mut events = Vec::new();

        // Text ranges become start/end events. Attachment ranges carry no text
        // of their own, so they are rendered separately.
        for (attr_id, attr) in ranges.iter().enumerate() {
            if attr.attachment.is_some() {
                continue;
            }
            events.push((attr.start, EventType::Start(attr_id, &attr.effects)));
            events.push((attr.end, EventType::End(attr_id)));
        }

        // End events run before start events at the same byte position.
        events.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| match (&a.1, &b.1) {
                (EventType::End(_), EventType::Start(_, _)) => Less,
                (EventType::Start(_, _), EventType::End(_)) => Greater,
                _ => Equal,
            })
        });

        let mut result = String::new();
        // Active attribute IDs and their effects.
        let mut active_attrs = Vec::new();
        let mut last_pos = events.first().map_or(0, |(pos, _)| *pos);

        for (pos, event) in events {
            // Add text before this event with current active attributes
            if pos > last_pos && last_pos < text.len() {
                // Get the text slice from last position to current position
                let end_pos = min(pos, text.len());
                let text_slice = &text[last_pos..end_pos];
                // Sanitize the text slice
                let sanitized_text = sanitize_html(text_slice);
                result.push_str(&self.apply_active_attributes(&sanitized_text, &active_attrs));
            }

            // Update active attributes based on the event
            match event {
                EventType::Start(attr_id, attr) => {
                    // Add the attribute that starts
                    active_attrs.push((attr_id, attr));
                }
                EventType::End(attr_id) => {
                    // Remove the attribute that ends
                    active_attrs.retain(|(id, _)| *id != attr_id);
                }
            }

            last_pos = pos;
        }
        result
    }

    fn render_run(
        &'a self,
        message: &'a Message,
        ranges: &'a [AttributedRange],
        attachments: &'a mut Vec<Attachment>,
        resolver: &mut AttachmentResolver,
    ) -> <Self as PartBodyBuilder>::Body {
        let text = message.text.as_deref().unwrap_or_default();
        let is_translated = self.config.is_translated(message);

        // Does this run carry an inline-rendered attachment? Apple's
        // `emoji_image` hint is the signal.
        let has_inline_sticker = ranges
            .iter()
            .any(|range| range.attachment.is_some() && range.emoji_image);

        // Inline path: text interleaved with glyph-sized static stickers in a
        // single bubble. Suppressed for translated messages so a translated
        // bubble and an inline sticker don't render side by side incoherently.
        if has_inline_sticker && !is_translated {
            let segments = self.interleave_segments(text, ranges, message, attachments, resolver);
            return PartBody::InlineBubble {
                // Patched with the per-message jumbomoji class by the caller.
                bubble_class: GlyphSize::Normal.bubble_class(),
                segments,
            };
        }

        // Translated run carrying an inline sticker. The inline path above is
        // suppressed for translated messages, but the lone-attachment block path
        // below would drop the text *and* the translation, keeping only the last
        // attachment. Render the text interleaved with the inline sticker(s) as
        // the original and pair it with the translation, so none of the three is
        // lost. (A sticker-only translated run has no text to preserve and falls
        // through to the block path).
        let has_text = ranges.iter().any(|range| range.attachment.is_none());
        if has_inline_sticker && is_translated && has_text {
            let original = inline_segments_to_html(self.interleave_segments(
                text,
                ranges,
                message,
                attachments,
                resolver,
            ));
            return match self.config.translation_for(message) {
                Ok(Some(translation)) => {
                    let safe_translated = self.body_escape(&translation.translated_text);
                    self.body_text_translated(safe_translated, original)
                }
                _ => self.body_text_bubble(original),
            };
        }

        // Block path: a run whose attachments each render as their own balloon: a
        // regular file, an animated (block) sticker, or a sticker-only translated
        // message.
        if ranges.iter().any(AttributedRange::is_attachment) {
            let mut body = self.body_attachment_missing();
            for (range, idx) in resolve_run(ranges, resolver) {
                let (Some(meta), Some(idx)) = (range.attachment.as_ref(), idx) else {
                    continue;
                };
                body = match attachments.get_mut(idx) {
                    Some(attachment) if attachment.is_sticker => {
                        let content = self.format_sticker(attachment, message);
                        self.body_sticker(content)
                    }
                    Some(attachment) => match self.format_attachment(attachment, message, meta) {
                        AttachmentRender::Embedded(content) => self.body_attachment(content),
                        AttachmentRender::MissingFilename => self.body_attachment_missing(),
                        AttachmentRender::NamedFile(name) => self.body_attachment_error(&name),
                    },
                    None => self.body_attachment_missing(),
                };
            }
            return body;
        }

        // Pure-text run.
        let formatted = {
            let attr_text = self.format_attributes(text, ranges);
            if attr_text.is_empty() {
                self.body_escape(text)
            } else {
                attr_text
            }
        };
        self.body_text_with_translation(message, formatted)
    }

    fn format_message_into(
        &self,
        message: &Message,
        context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError> {
        let is_reply = matches!(context, RenderContext::Reply);
        let mut ctx = MessageContext::resolve(
            message,
            self.config.data_source.db(),
            &self.config.data_source.capabilities,
        )?;
        let parts = self.build_message_parts(message, &mut ctx)?;

        let (date, read_after) = self.get_time(message);
        let reply_anchor = if message.is_reply() {
            Some(if is_reply {
                ReplyAnchorKind::InThread
            } else {
                ReplyAnchorKind::TopLevel
            })
        } else {
            None
        };

        let vm = MessageVM {
            guid: &message.guid,
            anchor_id: message.is_reply() && !is_reply,
            is_from_me: message.is_from_me(),
            service: message.service(),
            digital_touch: message.is_digital_touch(),
            date,
            read_after,
            reply_anchor,
            sender: self.config.who(
                message.handle_id,
                message.is_from_me(),
                &message.destination_caller_id,
            ),
            is_deleted: message.is_deleted(),
            subject: message.subject.as_deref(),
            shareplay: message
                .is_shareplay()
                .then(|| Html::trust(self.format_shareplay())),
            shared_location: message
                .shared_location_kind()
                .map(|kind| Html::trust(self.format_shared_location(kind))),
            parts,
            trailing_reply_context: message.is_reply() && !is_reply,
        };
        render_template_into(&vm, out);
        Ok(())
    }
}

// MARK: Part Body
impl PartBodyBuilder for HTML<'_> {
    type Body = PartBody;

    fn body_empty(&self) -> Self::Body {
        PartBody::Empty
    }

    fn body_text_bubble(&self, content: String) -> Self::Body {
        PartBody::TextBubble {
            bubble_class: GlyphSize::Normal.bubble_class(),
            html: Html::trust(content),
        }
    }

    fn body_text_translated(&self, translated: String, original: String) -> Self::Body {
        PartBody::TextTranslated {
            translated: Html::trust(translated),
            original: Html::trust(original),
        }
    }

    fn body_text_edited(&self, content: String) -> Self::Body {
        PartBody::TextEdited {
            html: Html::trust(content),
        }
    }

    fn body_attachment(&self, content: String) -> Self::Body {
        PartBody::Attachment {
            html: Html::trust(content),
        }
    }

    fn body_attachment_error(&self, error: &str) -> Self::Body {
        PartBody::AttachmentError {
            error: Html::trust(sanitize_html(error).into_owned()),
        }
    }

    fn body_attachment_missing(&self) -> Self::Body {
        PartBody::AttachmentMissing
    }

    fn body_sticker(&self, content: String) -> Self::Body {
        PartBody::Sticker {
            html: Html::trust(content),
        }
    }

    fn body_app(&self, content: String) -> Self::Body {
        PartBody::App {
            html: Html::trust(content),
        }
    }

    fn body_app_error(&self, message: &Message, why: String) -> Self::Body {
        PartBody::AppError {
            html: Html::trust(
                sanitize_html(&format!(
                    "Unable to format {:?} message: {why}",
                    message.variant()
                ))
                .into_owned(),
            ),
        }
    }

    fn body_retracted(&self, content: String) -> Self::Body {
        PartBody::Retracted {
            html: Html::trust(content),
        }
    }

    fn body_escape(&self, text: &str) -> String {
        sanitize_html(text).into_owned()
    }

    fn config(&self) -> &Config {
        self.config
    }
}

// MARK: Impl
impl<'a> HTML<'a> {
    /// Build the per-message [`MessagePartVM`] list. The body parser has
    /// already grouped attributed ranges into one
    /// [`Run`](BubbleComponent::Run) per bubble, so this walker is a thin pass:
    /// dispatch each component to its part body, stamp the per-message
    /// jumbomoji size class onto bubble bodies, and attach that part's tapbacks
    /// and replies. Reactions key on the component index, which equals the
    /// message-part index (runs are emitted in contiguous part order), so no
    /// cross-component aggregation is needed.
    ///
    /// Extracted from [`format_message_into`](Self::format_message_into) so
    /// tests can drive the walker with a hand-built [`MessageContext`].
    fn build_message_parts(
        &'a self,
        message: &'a Message,
        ctx: &mut MessageContext<'a>,
    ) -> Result<Vec<MessagePartVM<'a>>, RuntimeError> {
        // Pair body attachment placeholders to resolved attachments by GUID
        // (positional fallback for legacy bodies); built once for the message.
        let mut resolver = AttachmentResolver::new(&ctx.attachments);

        // Classify the whole message once for jumbomoji sizing; the result is
        // applied to whichever bubble body the dispatch produces.
        let glyph_class = jumbomoji::classify_message(&message.components, message.text.as_deref());

        let mut parts = Vec::with_capacity(message.components.len());
        for (idx, message_part) in message.components.iter().enumerate() {
            let body = dispatch_part_body(
                self,
                message,
                idx,
                message_part,
                &mut ctx.attachments,
                &mut resolver,
            );
            // Plumb the per-message glyph class into bubble bodies so pure-glyph
            // messages (emoji and/or inline stickers) pick up jumbomoji sizing.
            let body = match body {
                PartBody::TextBubble { html, .. } => PartBody::TextBubble {
                    bubble_class: glyph_class.bubble_class(),
                    html,
                },
                PartBody::InlineBubble { segments, .. } => PartBody::InlineBubble {
                    bubble_class: glyph_class.bubble_class(),
                    segments,
                },
                // Translated / edited / attachment / app bodies carry no
                // `bubble_class`, so they intentionally never receive jumbomoji
                // sizing.
                other => other,
            };

            parts.push(MessagePartVM {
                body,
                expressive: ctx.expressive,
                tapbacks: build_tapbacks(self, message, idx, Html::trust)?
                    .map(|tapbacks| TapbacksVM { tapbacks }),
                replies: build_replies(
                    self,
                    ctx.replies_map.get_mut(&idx),
                    Self::BUFFER_CAPACITY,
                    Html::trust,
                )?
                .map(|replies| RepliesVM { replies }),
            });
        }

        Ok(parts)
    }

    /// Render a single text range with its effects, falling back to the range's
    /// own (sanitized) text slice when
    /// [`format_attributes`](Self::format_attributes) yields nothing. Using
    /// the whole message text would smear sibling ranges into this segment.
    fn render_text_range(&self, text: &str, range: &AttributedRange) -> String {
        let formatted = self.format_attributes(text, std::slice::from_ref(range));
        if !formatted.is_empty() {
            return formatted;
        }
        let len = text.len();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        sanitize_html(&text[start..end]).into_owned()
    }

    /// Walk a run's ranges in body order, producing one [`InlineSegment`] per
    /// range: an inline sticker `<img>` for a resolved attachment range, the
    /// missing-attachment placeholder for a dangling one, and the text (with
    /// effects applied) for a text range. Shared by `render_run`'s inline and
    /// translated-inline paths and by `format_edited`'s inline-sticker branch,
    /// so the resolve-then-interleave logic lives in exactly one place.
    fn interleave_segments(
        &self,
        text: &str,
        ranges: &[AttributedRange],
        message: &Message,
        attachments: &mut [Attachment],
        resolver: &mut AttachmentResolver,
    ) -> Vec<InlineSegment> {
        let resolved = resolve_run(ranges, resolver);
        let mut segments = Vec::with_capacity(resolved.len());
        for (range, idx) in resolved {
            match idx {
                Some(idx) => match attachments.get_mut(idx) {
                    Some(attachment) => {
                        let img = self.format_sticker_inline(attachment, message);
                        segments.push(InlineSegment::Sticker(Html::trust(img)));
                    }
                    None => segments.push(InlineSegment::Text(Html::trust(
                        MISSING_INLINE_ATTACHMENT.to_string(),
                    ))),
                },
                None => segments.push(InlineSegment::Text(Html::trust(
                    self.render_text_range(text, range),
                ))),
            }
        }
        segments
    }

    /// Render a static sticker as a glyph-sized inline `<img>`. The block-style
    /// "Sent with … effect" suffix is dropped; the same text rides along on a
    /// `title=` attribute (and `alt=`) so it stays reachable on hover and for
    /// screen readers. On a prepare failure the segment still emits an
    /// `<img>` (with no or unreachable `src`) so the browser shows its
    /// broken-image glyph.
    fn format_sticker_inline(&self, sticker: &mut Attachment, message: &Message) -> String {
        let (embed_path, label) =
            match prepare_attachment(self.config, &self.state, sticker, message) {
                Ok(()) => {
                    let path = self.config.message_attachment_path(sticker);
                    let label = sticker
                        .get_sticker_decoration(
                            self.config.data_source.db(),
                            &self.config.options.platform,
                            &self.config.options.db_path,
                            self.config.options.attachment_root.as_deref(),
                        )
                        .map(|kind| sticker_decoration_label(&kind));
                    (Some(path), label)
                }
                Err(AttachmentRender::MissingFilename) => (None, None),
                Err(AttachmentRender::NamedFile(name)) => (Some(name), None),
                Err(AttachmentRender::Embedded(_)) => {
                    unreachable!("prepare_attachment never returns Embedded as an Err variant")
                }
            };

        render_template(&StickerInlineVM {
            lazy: !self.config.options.no_lazy,
            embed_path,
            label,
        })
    }
}

/// Flatten a sequence of inline segments into one HTML-safe string by
/// concatenating their inner markup, exactly as `message_part.html` renders an
/// [`InlineBubble`](view_model::PartBody::InlineBubble) (each segment emitted
/// back-to-back inside one bubble span). Used where the inline content must live
/// in a `String` rather than a bubble: the translated original and edit-history
/// rows.
fn inline_segments_to_html(segments: Vec<InlineSegment>) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            InlineSegment::Text(html) | InlineSegment::Sticker(html) => {
                out.push_str(&html.into_inner());
            }
        }
    }
    out
}

/// Single source of truth for the plain-text label of a [`StickerDecoration`].
/// The inline form drops it into `alt=` / `title=`; the block form renders it
/// inside `<div class="{class}">{label}</div>` (via
/// [`sticker_decoration_class`]). Routing both forms through this function
/// keeps the wording from drifting between the two render paths.
fn sticker_decoration_label(kind: &StickerDecoration) -> String {
    match kind {
        StickerDecoration::GenmojiPrompt(prompt) => format!("Genmoji prompt: {prompt}"),
        StickerDecoration::Memoji => "App: Memoji".to_string(),
        StickerDecoration::Effect(effect) => format!("Sent with {effect} effect"),
        StickerDecoration::AppName(name) => format!("App: {name}"),
    }
}

/// CSS class for the block-style sticker decoration container. Paired with
/// [`sticker_decoration_label`] in `templates/attachments/sticker_suffix.html`.
fn sticker_decoration_class(kind: &StickerDecoration) -> &'static str {
    match kind {
        StickerDecoration::GenmojiPrompt(_) => "genmoji_prompt",
        StickerDecoration::Memoji | StickerDecoration::AppName(_) => "sticker_name",
        StickerDecoration::Effect(_) => "sticker_effect",
    }
}

impl HTML<'_> {
    fn get_time(&self, message: &Message) -> (String, String) {
        message_time(self.config, message)
    }

    fn write_headers(file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        file.write_all(HEADER.as_bytes())?;
        file.write_all(b"<style>\n")?;
        file.write_all(STYLE.as_bytes())?;
        file.write_all(b"\n</style>")?;
        file.write_all(b"<link rel=\"stylesheet\" href=\"style.css\">")?;
        file.write_all(b"\n</head>\n<body>\n")?;
        Ok(())
    }

    fn apply_active_attributes<'a>(
        &'a self,
        text: &'a str,
        active_attrs: &'a [(usize, &[TextEffect])],
    ) -> Cow<'a, str> {
        // The first non-`Default` effect flips us into the owned path; from
        // that point on every iteration reads from `owned` and writes the
        // next render back into it.
        let mut owned: Option<String> = None;
        for (_, effects) in active_attrs {
            for effect in *effects {
                if matches!(effect, TextEffect::Default) {
                    continue;
                }
                let current = owned.as_deref().unwrap_or(text);
                owned = Some(self.format_effect(current, effect).into_owned());
            }
        }

        match owned {
            Some(s) => Cow::Owned(s),
            None => Cow::Borrowed(text),
        }
    }
}

// MARK: Tests

#[cfg(test)]
mod tests {
    use std::{env::current_dir, path::PathBuf};

    use crate::{
        Config, HTML, Options,
        app::{
            compatibility::attachment_manager::AttachmentManagerMode, contacts::Name,
            export_type::ExportType,
        },
        exporters::formatter::{AttachmentRender, MessageFormatter, RenderContext},
    };

    use imessage_database::{
        message_types::text_effects::text_effect::TextEffect,
        tables::{
            messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
            table::{FITNESS_RECEIVER, ME},
        },
        util::{dirs::home, platform::Platform},
    };

    #[test]
    fn can_create() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();
        assert_eq!(exporter.state.files.len(), 0);
    }

    #[test]
    fn can_get_time_valid() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        // let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;

        assert_eq!(
            exporter.get_time(&message),
            (
                "May 17, 2022  5:29:42 PM".to_string(),
                "(Read by you after 1 hour, 49 seconds)".to_string()
            )
        );
    }

    #[test]
    fn can_get_time_invalid() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  9:30:31 PM
        message.date = 674530231992568192;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        // Wed May 18 2022 02:36:24 GMT+0000
        message.date_read = 674526582885055488;
        assert_eq!(
            exporter.get_time(&message),
            ("May 17, 2022  6:30:31 PM".to_string(), String::new())
        );
    }

    #[test]
    fn can_format_html_from_me_normal() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_fitness_receiver_rewrite() {
        // A Fitness transcript message whose body begins with the
        // `FITNESS_RECEIVER` sentinel must render as "You".
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some(format!("{FITNESS_RECEIVER} closed all three rings"));
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">You closed all three rings</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_message_with_html() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("<table></table>".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">&lt;table&gt;&lt;/table&gt;</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_deleted() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.text = Some("Hello world".to_string());
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.deleted_from = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        <span class=\"deleted\">This message was deleted from the conversation!</span>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_me_normal_read() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_delivered = 674530231992568192;
        message.is_from_me = true;
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by them after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.handle_id = Some(999999);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_normal_read() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = Some(999999);
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by you after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_from_them_custom_name_read() {
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = Some(999999);
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        // May 17, 2022  8:29:42 PM
        message.date_delivered = 674526582885055488;
        // May 17, 2022  9:30:31 PM
        message.date_read = 674530231992568192;
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                (Read by Name after 1 hour, 49 seconds)\n            </span>\n            \n            <span class=\"sender\">Sample Contact</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_shareplay() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.item_type = 6;

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        <span class=\"shareplay\"><hr>SharePlay Message Ended</span>\n        \n        \n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 2;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You named the conversation <b>Hello world</b></p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_announcement_custom_name() {
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.item_type = 2;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Name named the conversation <b>Hello world</b></p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn html_announcement_who_is_escaped_once() {
        // Regression: `who` is rendered with the default escaper in the
        // template, so the formatter must not pre-escape it. Pre-escaping
        // would produce `&amp;amp;` for an `&` in the name.
        let mut options = Options::fake_options(ExportType::Html);
        options.custom_name = Some("Bob & <Alice>".to_string());
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.item_type = 3; // ParticipantLeft

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Bob &amp; &lt;Alice&gt; left the conversation.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_reply_top_level() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.guid = "TOP-GUID".to_string();
        message.text = Some("hello".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.thread_originator_guid = Some("ORIG-GUID".to_string());
        message.thread_originator_part = Some("0:0:0".to_string());
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\" id=\"r-TOP-GUID\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=TOP-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            \n            <span class=\"reply_anchor\"><a title=\"View in thread\" href=\"#TOP-GUID\">⇱</a></span>\n            \n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n        <span class=\"reply_context\">This message responded to an earlier message.</span>\n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_reply_in_thread() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.guid = "INNER-GUID".to_string();
        message.text = Some("hello".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.thread_originator_guid = Some("ORIG-GUID".to_string());
        message.thread_originator_part = Some("0:0:0".to_string());
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut buf = String::with_capacity(2048);
        exporter
            .format_message_into(&message, RenderContext::Reply, &mut buf)
            .unwrap();
        let actual = buf;
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=INNER-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            \n            <span class=\"reply_anchor\"><a title=\"View in context\" href=\"#r-INNER-GUID\">⇲</a></span>\n            \n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_non_reply_has_no_anchor() {
        // Sanity check: a regular message has no reply anchor and no anchor id.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.guid = "PLAIN-GUID".to_string();
        message.text = Some("hello".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=PLAIN-GUID\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">hello</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_part_body_attachment_missing_standalone() {
        // An attachment-range Run with no matching Attachment row →
        // PartBody::AttachmentMissing → "<span class=\"attachment_error\">Attachment does not exist!</span>"
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.rowid = i32::MAX; // unlikely to exist in fixture db
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
            0,
            3,
            AttachmentMeta::default(),
        )])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_url_message_without_payload_uses_text_fallback() {
        // Defensive path in dispatch_app_balloon: when a URL-balloon message
        // has no payload row but does carry `text`, the normal `format_url`
        // pipeline still produces a clickable link via its msg.text fallback.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.rowid = i32::MAX; // not in fixture db, so payload_data returns None
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
        message.text = Some("https://example.com".to_string());
        message.components = vec![BubbleComponent::App];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://example.com\"><div class=\"app_header\"><div class=\"name\">https://example.com</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_url_message_without_payload_escapes_text() {
        // The fallback flows msg.text through `format_url` and the Askama
        // template's auto-escaper; raw HTML in `text` must not survive.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.rowid = i32::MAX;
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.balloon_bundle_id = Some("com.apple.messages.URLBalloonProvider".to_string());
        message.text = Some("https://x.test/?q=<script>".to_string());
        message.components = vec![BubbleComponent::App];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://x.test/?q=&lt;script&gt;\"><div class=\"app_header\"><div class=\"name\">https://x.test/?q=&lt;script&gt;</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn expressive_renders_via_display_impl() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.expressive_send_style_id =
            Some("com.apple.messages.effect.CKConfettiEffect".to_string());
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hello world</span>\n    </div>\n<span class=\"expressive\">Sent with Confetti</span>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn expressive_empty_unknown_renders_like_none() {
        // expressive_send_style_id = Some("") rows must render identically to
        // expressive_send_style_id = None: no stray empty `<span class="expressive">`
        // from the empty Unknown variant passing through the template's `Some` guard.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let build = |expressive: Option<String>| {
            let mut m = Config::fake_message();
            m.date = 674526582885055488;
            m.text = Some("Hello world".to_string());
            m.is_from_me = true;
            m.chat_id = Some(0);
            m.expressive_send_style_id = expressive;
            m.generate_text_legacy(config.data_source.db()).unwrap();
            m
        };

        let mut baseline = String::new();
        exporter
            .format_message_into(&build(None), RenderContext::TopLevel, &mut baseline)
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(
                &build(Some(String::new())),
                RenderContext::TopLevel,
                &mut actual,
            )
            .unwrap();

        assert_eq!(actual, baseline);
    }

    #[test]
    fn can_format_html_part_body_app_error_on_normal_variant() {
        // BubbleComponent::App on a Variant::Normal message → format_app
        // returns WrongMessageType → PartBody::AppError, escaped via sanitize_html.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.rowid = i32::MAX;
        message.is_from_me = true;
        message.chat_id = Some(0);
        // Default fake_message is Variant::Normal (no balloon_bundle_id, AMT=0).
        // Adding a BubbleComponent::App forces format_app's else-branch.
        message.components = vec![BubbleComponent::App];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app_error\">Unable to format Normal message: Failed to parse property list: Message is not an app message!</div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn format_message_into_appends_to_existing_buffer() {
        // Mirrors the production hot path in `run_export`, which reuses a
        // single `String` across messages via `clear()` + `format_message_into`.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;
        message.text = Some("hello".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut standalone = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut standalone)
            .unwrap();

        // Pre-fill the buffer with content the writer would have left in
        // (e.g. the previous message). format_message_into should leave that
        // content alone and append the new render after it.
        let prefix = "<!-- previous message -->\n";
        let mut buf = String::with_capacity(2048);
        buf.push_str(prefix);
        let cap_before = buf.capacity();

        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut buf)
            .unwrap();

        assert!(
            buf.starts_with(prefix),
            "format_message_into must not overwrite existing buffer content"
        );
        assert_eq!(&buf[prefix.len()..], standalone);
        // Capacity should not have shrunk; if anything it grows to fit the
        // new content.
        assert!(buf.capacity() >= cap_before);
    }

    #[test]
    fn can_format_html_announcement_unknown() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.date = 674526582885055488;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected =
            "\n<div class=\"announcement\">\n    <p>Unable to format announcement!</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_removed() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 1;
        message.other_handle = Some(1);

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed Other from the conversation.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_removed_other() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.participants.insert(2, Name::fake_name("Second"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);
        config.real_participants.insert(2, 2);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = false;
        message.handle_id = Some(1);
        message.item_type = 1;
        message.group_action_type = 1;
        message.other_handle = Some(2);

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Other removed Second from the conversation.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_changed_number() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = false;
        message.handle_id = Some(1);
        message.item_type = 1;
        message.group_action_type = 0;
        message.other_handle = Some(1);

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> Other changed their phone number.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_added() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.participants.insert(1, Name::fake_name("Other"));
        config.real_participants.insert(0, 0);
        config.real_participants.insert(1, 1);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 1;
        message.group_action_type = 0;
        message.other_handle = Some(1);

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You added Other to the conversation.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_left() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You left the conversation.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_icon_removed() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 2;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the group photo.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_group_icon_added() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 1;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the group photo.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_chat_background_removed() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 6;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You removed the chat background.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_chat_background_added() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.group_title = Some("Hello world".to_string());
        message.is_from_me = true;
        message.item_type = 3;
        message.group_action_type = 4;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You changed the chat background.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_audio_message_kept() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.is_from_me = true;
        message.item_type = 5;

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "\n<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You kept an audio message.</p>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_me() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.participants.insert(0, Name::fake_name(ME));
        config.real_participants.insert(0, 0);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\"><b>Loved</b> by Me</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_them() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2000);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\"><b>Loved</b> by Sample Contact</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_emoji() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2006);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.associated_message_emoji = Some("☕️".to_string());

        let actual = exporter.format_tapback(&message).unwrap();
        // The result contains `&nbsp;`
        let expected = "<span class=\"tapback\"><b>☕\u{fe0f}</b> by Sample Contact</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2007);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.num_attachments = 1;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "<span class=\"tapback\">Sticker from Sample Contact not found!</span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker_exists() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(2007);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.num_attachments = 1;
        message.rowid = 452567;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = format!(
            "<img src=\"{}/Library/Messages/StickerCache/8e682c381ab52ec2-289D9E83-33EE-4153-AF13-43DB31792C6F/289D9E83-33EE-4153-AF13-43DB31792C6F.heic\" loading=\"lazy\"><div class=\"sticker_name\">App: Free People</div><div class=\"sticker_tapback\">&nbsp;by Sample Contact</div>",
            home()
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tapback_custom_sticker_removed() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config
            .participants
            .insert(999999, Name::fake_name("Sample Contact"));
        config.real_participants.insert(999999, 999999);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.associated_message_type = Some(3007);
        message.associated_message_guid = Some("fake_guid".to_string());
        message.handle_id = Some(999999);
        message.num_attachments = 1;
        message.rowid = 452567;

        let actual = exporter.format_tapback(&message).unwrap();
        let expected = "";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_me() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = Some(2);
        message.share_status = false;
        message.share_direction = Some(false);
        message.item_type = 4;

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Started sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_me() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = false;
        message.other_handle = Some(2);
        message.share_status = true;
        message.share_direction = Some(false);
        message.item_type = 4;

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Stopped sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_started_sharing_location_them() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = Some(0);
        message.share_status = false;
        message.share_direction = Some(false);
        message.item_type = 4;

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Started sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_stopped_sharing_location_them() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.handle_id = None;
        message.is_from_me = false;
        message.other_handle = Some(0);
        message.share_status = true;
        message.share_direction = Some(false);
        message.item_type = 4;

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">Dec 31, 2000  4:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        <span class=\"shared_location\"><hr>Stopped sharing location!</span>\n        \n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_attachment_macos() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(
            actual,
            AttachmentRender::Embedded("<img src=\"a/b/c/d.jpg\" loading=\"lazy\">".to_string())
        );
    }

    #[test]
    fn can_format_html_attachment_macos_invalid_disabled() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, AttachmentRender::MissingFilename);
    }

    #[test]
    fn can_format_html_attachment_macos_invalid_clone() {
        let mut options = Options::fake_options(ExportType::Html);
        options.attachment_manager.mode = AttachmentManagerMode::Clone;

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, AttachmentRender::MissingFilename);
    }

    #[test]
    fn can_format_html_attachment_ios() {
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        config.options.no_lazy = true;
        config.options.platform = Platform::iOS;
        let exporter = HTML::new(&config).unwrap();
        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();

        let AttachmentRender::Embedded(actual) =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default())
        else {
            panic!("expected AttachmentRender::Embedded");
        };

        assert!(actual.ends_with("33/33c81da8ae3194fc5a0ea993ef6ffe0b048baedb\">"));
    }

    #[test]
    fn can_format_html_attachment_ios_invalid_disabled() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, AttachmentRender::MissingFilename);
    }

    #[test]
    fn can_format_html_attachment_ios_invalid_clone() {
        let mut options = Options::fake_options(ExportType::Html);
        options.attachment_manager.mode = AttachmentManagerMode::Clone;

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.filename = None;
        attachment.transfer_name = None;

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(actual, AttachmentRender::MissingFilename);
    }

    #[test]
    fn can_format_html_attachment_folder() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        let folder_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/");
        attachment.mime_type = None;
        attachment.transfer_name = Some("test_data".to_string());
        attachment.copied_path = Some(folder_path);

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        let abs_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/");
        let expected = format!(
            "<p>\n    Folder: <i>test_data</i> (100.00 B)\n    <a href=\"{}\">Click to open</a>\n</p>",
            abs_path.display()
        );

        assert_eq!(actual, AttachmentRender::Embedded(expected));
    }

    #[test]
    fn can_format_html_attachment_text_download() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        // text/* → MediaType::Text(_) → AttachmentVariant::Download
        attachment.mime_type = Some("text/plain".to_string());
        attachment.filename = Some("notes.txt".to_string());
        attachment.transfer_name = Some("notes.txt".to_string());

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(
            actual,
            AttachmentRender::Embedded(
                "<a href=\"notes.txt\">Click to download notes.txt (100.00 B)</a>".to_string()
            )
        );
    }

    #[test]
    fn can_format_html_attachment_application_download() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        // application/* → MediaType::Application(_) → AttachmentVariant::Download
        attachment.mime_type = Some("application/pdf".to_string());
        attachment.filename = Some("doc.pdf".to_string());
        attachment.transfer_name = Some("doc.pdf".to_string());

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(
            actual,
            AttachmentRender::Embedded(
                "<a href=\"doc.pdf\">Click to download doc.pdf (100.00 B)</a>".to_string()
            )
        );
    }

    #[test]
    fn can_format_html_attachment_other_media_type() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        // mime_type without a recognized prefix maps to MediaType::Other(full).
        attachment.mime_type = Some("model/gltf-binary".to_string());
        attachment.filename = Some("scene.glb".to_string());

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(
            actual,
            AttachmentRender::Embedded(
                "<p>Unable to embed model/gltf-binary attachments: scene.glb</p>".to_string()
            )
        );
    }

    #[test]
    fn can_format_html_attachment_unknown() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        let folder_path = "Fake";
        attachment.mime_type = None;
        attachment.transfer_name = Some("test_data".to_string());
        attachment.copied_path = Some(PathBuf::from(folder_path));

        let actual =
            exporter.format_attachment(&mut attachment, &message, &AttachmentMeta::default());

        assert_eq!(
            actual,
            AttachmentRender::Embedded(
                "<p>Unknown attachment type: Fake</p>\n<a href=\"Fake\">Download (100.00 B)</a>"
                    .to_string()
            )
        );
    }

    #[test]
    fn can_format_html_attachment_sticker() {
        let options = Options::fake_options(ExportType::Html);

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 3;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"sticker_effect\">Sent with Outline effect</div>"
        );
    }

    #[test]
    fn can_format_html_attachment_sticker_genmoji() {
        let options = Options::fake_options(ExportType::Html);

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 2;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );
        attachment.emoji_description = Some("pink poodle".to_string());

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"genmoji_prompt\">Genmoji prompt: pink poodle</div>"
        );
    }

    #[test]
    fn can_format_html_attachment_sticker_app() {
        let options = Options::fake_options(ExportType::Html);

        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 1;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );

        let actual = exporter.format_sticker(&mut attachment, &message);

        assert_eq!(
            actual,
            "<img src=\"imessage-database/test_data/stickers/outline.heic\" loading=\"lazy\"><div class=\"sticker_name\">App: Free People</div>"
        );
    }

    #[test]
    fn format_sticker_inline_renders_with_effect_title() {
        // Inline static stickers emit a bare `<img class="inline_sticker">`
        // with the decoration text carried on `title=` (rather than the
        // block-style `<div class="sticker_effect">`).
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 3;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );

        let actual = exporter.format_sticker_inline(&mut attachment, &message);
        let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"Sent with Outline effect\" title=\"Sent with Outline effect\" loading=\"lazy\">";
        assert_eq!(actual, expected);
    }

    #[test]
    fn format_sticker_inline_renders_with_genmoji_title() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 2;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );
        attachment.emoji_description = Some("pink poodle".to_string());

        let actual = exporter.format_sticker_inline(&mut attachment, &message);
        let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"Genmoji prompt: pink poodle\" title=\"Genmoji prompt: pink poodle\" loading=\"lazy\">";
        assert_eq!(actual, expected);
    }

    #[test]
    fn format_sticker_inline_renders_with_app_title() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.rowid = 1;
        attachment.is_sticker = true;
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        attachment.filename = Some(sticker_path.to_string_lossy().to_string());
        attachment.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );

        let actual = exporter.format_sticker_inline(&mut attachment, &message);
        let expected = "<img class=\"inline_sticker\" src=\"imessage-database/test_data/stickers/outline.heic\" alt=\"App: Free People\" title=\"App: Free People\" loading=\"lazy\">";
        assert_eq!(actual, expected);
    }

    // MARK: Walker / jumbomoji integration

    fn render_parts<'a>(
        exporter: &'a HTML<'a>,
        message: &'a imessage_database::tables::messages::Message,
        ctx: &mut crate::exporters::shared::message::MessageContext<'a>,
    ) -> String {
        use crate::exporters::shared::render::render_template;
        let parts = exporter.build_message_parts(message, ctx).unwrap();
        parts
            .iter()
            .map(render_template)
            .collect::<Vec<_>>()
            .join("")
    }

    fn empty_ctx<'a>() -> crate::exporters::shared::message::MessageContext<'a> {
        use std::collections::HashMap;
        crate::exporters::shared::message::MessageContext {
            attachments: vec![],
            replies_map: HashMap::new(),
            expressive: None,
        }
    }

    fn make_static_sticker(config: &Config) -> imessage_database::tables::attachment::Attachment {
        let sticker_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/stickers/outline.heic");
        let mut sticker = Config::fake_attachment();
        sticker.rowid = 3;
        sticker.is_sticker = true;
        sticker.mime_type = Some("image/heic".to_string());
        sticker.filename = Some(sticker_path.to_string_lossy().to_string());
        sticker.copied_path = Some(
            config
                .options
                .export_path
                .join("imessage-database/test_data/stickers/outline.heic"),
        );
        sticker
    }

    #[test]
    fn inline_stickers_resolve_by_guid_in_body_order() {
        // The body lists stickers in display order via their file-transfer
        // GUIDs, but `Attachment::from_message` returns them in an unspecified
        // join order. The walker must pair each placeholder to its own
        // attachment by GUID so the images keep body order, not DB order.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // A static sticker with a given GUID and a distinct relative `src`.
        let make = |guid: &str, file: &str| {
            let mut s = Config::fake_attachment();
            s.is_sticker = true;
            s.mime_type = Some("image/heic".to_string());
            s.guid = Some(guid.to_string());
            s.filename = Some(file.to_string());
            s.copied_path = Some(config.options.export_path.join(file));
            s
        };
        // An inline-sticker range carrying its file-transfer GUID.
        let range = |start: usize, guid: &str| AttributedRange {
            start,
            end: start + 3,
            effects: vec![],
            attachment: Some(AttachmentMeta {
                guid: Some(guid.to_string()),
                ..Default::default()
            }),
            emoji_image: true,
        };

        // Body order: A, B, C.
        let mut message = Config::fake_message();
        message.text = Some("\u{FFFC}\u{FFFC}\u{FFFC}".to_string());
        message.components = vec![BubbleComponent::Run(vec![
            range(0, "A"),
            range(3, "B"),
            range(6, "C"),
        ])];

        // Resolved attachments deliberately in a *different* (join) order: C, A, B.
        let mut ctx = empty_ctx();
        ctx.attachments = vec![
            make("C", "c.heic"),
            make("A", "a.heic"),
            make("B", "b.heic"),
        ];

        let actual = render_parts(&exporter, &message, &mut ctx);
        let pa = actual.find("a.heic").expect("a.heic in output");
        let pb = actual.find("b.heic").expect("b.heic in output");
        let pc = actual.find("c.heic").expect("c.heic in output");
        // Body order a < b < c, not the resolved order c, a, b (which positional
        // matching would have produced).
        assert!(
            pa < pb && pb < pc,
            "expected inline stickers in body order a<b<c, got: {actual}"
        );
    }

    #[test]
    fn text_only_single_emoji_renders_jumbo_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("🎉".to_string());
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            "🎉".len(),
            vec![TextEffect::Default],
        )])];

        let mut ctx = empty_ctx();
        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains(r#"<span class="bubble jumbo">🎉</span>"#),
            "expected jumbo class on single-emoji bubble, got: {actual}"
        );
    }

    #[test]
    fn text_only_three_emoji_renders_medium_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        let text = "🎉🎊🎁".to_string();
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        message.text = Some(text);

        let mut ctx = empty_ctx();
        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains(r#"class="bubble medium""#),
            "expected medium class on three-emoji bubble, got: {actual}"
        );
    }

    #[test]
    fn text_only_four_emoji_renders_normal_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        let text = "🎉🎊🎁🎀".to_string();
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        message.text = Some(text);

        let mut ctx = empty_ctx();
        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains(r#"class="bubble""#),
            "expected plain bubble class for 4+ emoji, got: {actual}"
        );
        assert!(
            !actual.contains("jumbo") && !actual.contains("medium"),
            "expected no size class for 4+ emoji, got: {actual}"
        );
    }

    #[test]
    fn text_with_emoji_renders_normal_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        let text = "Hello 👋".to_string();
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        message.text = Some(text);

        let mut ctx = empty_ctx();
        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            !actual.contains("jumbo") && !actual.contains("medium"),
            "non-pure-emoji text must not get a size class, got: {actual}"
        );
    }

    #[test]
    fn inline_sticker_between_text_renders_one_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // "Hello " = 6 bytes, "\u{FFFC}" = 3 bytes, " world" = 6 bytes
        message.text = Some("Hello \u{FFFC} world".to_string());
        // Inline sticker shares the text's part: one Run, ranges in order.
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 6, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(6, 9, AttachmentMeta::default()),
            AttributedRange::text(9, 15, vec![TextEffect::Default]),
        ])];

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];

        let actual = render_parts(&exporter, &message, &mut ctx);

        // The bubble wraps all three segments in one span.
        assert!(
            actual.contains("<span class=\"bubble\">Hello <img class=\"inline_sticker\""),
            "expected text + inline_sticker in the same bubble, got: {actual}"
        );
        // No block-level sticker div or effect suffix.
        assert!(
            !actual.contains("class=\"sticker\""),
            "inline sticker should not emit <div class=\"sticker\">, got: {actual}"
        );
        assert!(
            !actual.contains("sticker_effect"),
            "inline sticker should drop the effect suffix, got: {actual}"
        );
        // The decoration is preserved via title=.
        assert!(
            actual.contains("title=\"Sent with Outline effect\""),
            "inline sticker should carry decoration in title=, got: {actual}"
        );
    }

    #[test]
    fn inline_memoji_among_text_renders_inline_and_keeps_text() {
        // A non-edited run with text, an inline (image/heic) Memoji carrying the
        // `emoji_image` hint, and a trailing emoji: `render_run` must interleave
        // the sticker as an inline <img> and keep the surrounding text/emoji,
        // given the Memoji is resolved in `ctx.attachments`.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
        let mut message = Config::fake_message();
        message.text = Some("Check this out: \u{FFFC} 😀".to_string());
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 16, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(
                16,
                19,
                AttachmentMeta {
                    guid: Some(guid.to_string()),
                    ..Default::default()
                },
            ),
            AttributedRange::text(19, 24, vec![TextEffect::Default]),
        ])];

        // A static image/heic Memoji, matching the real attachment row.
        let mut memoji = make_static_sticker(&config);
        memoji.guid = Some(guid.to_string());
        let mut ctx = empty_ctx();
        ctx.attachments = vec![memoji];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("<img class=\"inline_sticker\""),
            "inline Memoji should render an inline <img>, got: {actual}"
        );
        assert!(
            actual.contains("😀"),
            "surrounding text/emoji must survive alongside the sticker, got: {actual}"
        );
        assert!(
            !actual.contains("class=\"sticker\""),
            "Memoji must render inline, not as a block <div class=\"sticker\">, got: {actual}"
        );
    }

    #[test]
    fn edited_message_inlines_memoji_sticker() {
        // Regression for the `MemojiEdited` fixture: an *edited* message with an
        // inline Memoji renders through `format_edited`, not `render_run`. It must
        // still inline the sticker as an <img> in every edit-history row rather
        // than leaking the bare `\u{FFFC}` placeholder.
        use imessage_database::message_types::edited::{
            EditStatus, EditedEvent, EditedMessage, EditedMessagePart,
        };

        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
        let mk_meta = || AttachmentMeta {
            guid: Some(guid.to_string()),
            ..Default::default()
        };

        // v1 ends with the Memoji; v2 adds " 😀" after it. Both carry the hint.
        let v1 = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 16, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(16, 19, mk_meta()),
        ])];
        let v2 = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 16, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(16, 19, mk_meta()),
            AttributedRange::text(19, 24, vec![TextEffect::Default]),
        ])];

        let mut message = Config::fake_message();
        message.text = Some("Check this out: \u{FFFC} 😀".to_string());
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            1,
            vec![TextEffect::Default],
        )])];
        message.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 758573156000000000,
                        text: "Check this out: \u{FFFC}".to_string(),
                        components: v1,
                        guid: None,
                    },
                    EditedEvent {
                        date: 758573166000000000,
                        text: "Check this out: \u{FFFC} 😀".to_string(),
                        components: v2,
                        guid: None,
                    },
                ],
            }],
        });

        let mut memoji = make_static_sticker(&config);
        memoji.guid = Some(guid.to_string());
        let mut ctx = empty_ctx();
        ctx.attachments = vec![memoji];

        let actual = render_parts(&exporter, &message, &mut ctx);

        assert_eq!(
            actual.matches("<img class=\"inline_sticker\"").count(),
            2,
            "both edit versions should inline the Memoji as an <img>, got: {actual}"
        );
        assert!(
            actual.contains('😀'),
            "the trailing emoji must survive, got: {actual}"
        );
        assert!(
            !actual.contains('\u{FFFC}'),
            "no bare object-replacement placeholder should leak, got: {actual}"
        );
    }

    #[test]
    fn missing_inline_sticker_row_renders_placeholder() {
        // An inline-sticker range that resolves to an absent attachment row (a
        // dangling placeholder / orphaned join → out-of-bounds index) must show
        // the same "missing" marker as the block path, not vanish silently.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("\u{FFFC}".to_string());
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
        ])];

        // No resolved attachments → the range's index is out of bounds.
        let mut ctx = empty_ctx();
        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("Attachment does not exist!"),
            "missing inline sticker must render a placeholder, got: {actual}"
        );
    }

    #[test]
    fn single_inline_sticker_renders_jumbo_bubble() {
        // A pure-sticker message (one static sticker, no text) should render
        // as a jumbo-sized inline bubble.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("\u{FFFC}".to_string());
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
        ])];

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("class=\"bubble jumbo\""),
            "single static sticker should be jumbo, got: {actual}"
        );
        assert!(
            actual.contains("<img class=\"inline_sticker\""),
            "expected inline_sticker img, got: {actual}"
        );
    }

    #[test]
    fn multiple_inline_stickers_render_in_one_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // 6 stickers separated by spaces, no surrounding text. Each \u{FFFC}
        // is 3 bytes; spaces are 1 byte. Components must reference the spaces.
        let text = "\u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC} \u{FFFC}".to_string();
        message.text = Some(text);
        // All six inline stickers and the spaces between them share one part.
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
            AttributedRange::text(3, 4, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(4, 7, AttachmentMeta::default()),
            AttributedRange::text(7, 8, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(8, 11, AttachmentMeta::default()),
            AttributedRange::text(11, 12, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(12, 15, AttachmentMeta::default()),
            AttributedRange::text(15, 16, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(16, 19, AttachmentMeta::default()),
            AttributedRange::text(19, 20, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(20, 23, AttachmentMeta::default()),
        ])];

        let mut ctx = empty_ctx();
        ctx.attachments = (0..6).map(|_| make_static_sticker(&config)).collect();

        let actual = render_parts(&exporter, &message, &mut ctx);
        let sticker_count = actual.matches("<img class=\"inline_sticker\"").count();
        assert_eq!(
            sticker_count, 6,
            "expected 6 inline stickers in a single bubble, got {sticker_count}: {actual}"
        );
        let bubble_open_count = actual.matches("<span class=\"bubble").count();
        assert_eq!(
            bubble_open_count, 1,
            "all stickers should share one bubble, found {bubble_open_count} bubbles: {actual}"
        );
        assert!(
            !actual.contains("class=\"sticker\""),
            "no block-level sticker divs expected: {actual}"
        );
    }

    #[test]
    fn animated_sticker_renders_as_block() {
        // HEIC sequence stickers stay on the existing block path (their own
        // <div class="sticker"> with the "Sent with … effect" suffix), even
        // when surrounded by text.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("Hi \u{FFFC} bye".to_string());
        // An animated sticker is its own part, so the text on either side stays
        // in its own Run: three parts in all.
        message.components = vec![
            BubbleComponent::Run(vec![AttributedRange::text(0, 3, vec![TextEffect::Default])]),
            BubbleComponent::Run(vec![AttributedRange::attachment(
                3,
                6,
                AttachmentMeta::default(),
            )]),
            BubbleComponent::Run(vec![AttributedRange::text(
                6,
                10,
                vec![TextEffect::Default],
            )]),
        ];

        let mut animated = make_static_sticker(&config);
        animated.mime_type = Some("image/heic-sequence".to_string());
        let mut ctx = empty_ctx();
        ctx.attachments = vec![animated];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("<div class=\"sticker\">"),
            "animated sticker should keep block rendering, got: {actual}"
        );
        // Surrounding text should NOT have merged into an inline bubble.
        assert!(
            !actual.contains("class=\"inline_sticker\""),
            "animated sticker must not emit inline_sticker img: {actual}"
        );
    }

    #[test]
    fn translated_message_with_inline_sticker_uses_block_path() {
        // When a message is translated, every component falls back to the
        // block-style rendering–including stickers–so the bubble's
        // semantics stay consistent (a translated text bubble alongside an
        // inline-style sticker would look incoherent).
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        let test_guid = "TRANSLATED-STICKER-GUID-0001".to_string();
        config.translated_messages.insert(test_guid.clone());
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = test_guid;
        message.text = Some("\u{FFFC}".to_string());
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
            0,
            3,
            AttachmentMeta::default(),
        )])];

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("<div class=\"sticker\">"),
            "translated message must keep sticker on the block path: {actual}"
        );
        assert!(
            !actual.contains("class=\"inline_sticker\""),
            "no inline_sticker img expected for a translated message: {actual}"
        );
    }

    #[test]
    fn translated_message_with_inline_sticker_keeps_text_and_sticker() {
        // For a translated run of [text, inline Memoji, text], the text and the
        // sticker must both survive (interleaved as the translation's "original").
        // The translation itself is fetched from the DB via `get_translation`, so
        // with no translation row this renders as a plain bubble: the regression
        // being guarded is the lost text/sticker, not the translation lookup.
        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        let test_guid = "TRANSLATED-INLINE-TEXT-0001".to_string();
        config.translated_messages.insert(test_guid.clone());
        let exporter = HTML::new(&config).unwrap();

        let sticker_guid = "F2C223DB-0140-4D49-B38A-C1A3553B4CBA";
        let mut message = Config::fake_message();
        message.guid = test_guid;
        message.text = Some("Look at this \u{FFFC} now".to_string());
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 13, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(
                13,
                16,
                AttachmentMeta {
                    guid: Some(sticker_guid.to_string()),
                    ..Default::default()
                },
            ),
            AttributedRange::text(16, 20, vec![TextEffect::Default]),
        ])];

        let mut memoji = make_static_sticker(&config);
        memoji.guid = Some(sticker_guid.to_string());
        let mut ctx = empty_ctx();
        ctx.attachments = vec![memoji];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("Look at this"),
            "leading text must survive on the translated path: {actual}"
        );
        assert!(
            actual.contains("now"),
            "trailing text must survive on the translated path: {actual}"
        );
        assert!(
            actual.contains("<img class=\"inline_sticker\""),
            "the inline sticker must still render: {actual}"
        );
        assert!(
            !actual.contains("class=\"sticker\""),
            "must not collapse to a lone block sticker: {actual}"
        );
    }

    #[test]
    fn edited_text_part_flushes_block_but_sibling_sticker_inlines() {
        // Per-part edit semantics: an edited text part block-flushes via
        // `<div class="edited">`, but an adjacent attachment whose own part
        // index isn't marked Edited still goes through the inline path.
        // Edits to text never bleed into sibling stickers.
        use imessage_database::message_types::edited::{
            EditStatus, EditedMessage, EditedMessagePart,
        };

        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("hi \u{FFFC}".to_string());
        // An edit applies to a whole Run/part, so the edited text and the
        // sibling sticker live in separate parts; the edit can't bleed across.
        message.components = vec![
            BubbleComponent::Run(vec![AttributedRange::text(0, 3, vec![TextEffect::Default])]),
            BubbleComponent::Run(vec![AttributedRange::inline_attachment(
                3,
                6,
                AttachmentMeta::default(),
            )]),
        ];
        // Mark only the Text part (idx 0) as edited.
        message.edited_parts = Some(EditedMessage {
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
        message.date_edited = 674526582885055488;

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];

        let actual = render_parts(&exporter, &message, &mut ctx);
        // Edited text part block-flushes through dispatch_part_body's
        // PartBody::TextEdited arm.
        assert!(
            actual.contains("class=\"edited\"") || actual.contains("Edited"),
            "edited text part should not be inlined: {actual}"
        );
        // Sticker's own idx isn't Edited → still on the inline path.
        assert!(
            actual.contains("class=\"inline_sticker\""),
            "non-edited sticker should still go inline: {actual}"
        );
    }

    #[test]
    fn missing_inline_sticker_renders_as_broken_image() {
        // A sticker with no filename should still produce a visible inline
        // `<img>` (which the browser renders as its broken-image glyph)
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("hi \u{FFFC}".to_string());
        // Inline sticker shares the text's part: one Run.
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 3, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
        ])];

        let mut sticker = Config::fake_attachment();
        sticker.rowid = 3;
        sticker.is_sticker = true;
        sticker.mime_type = Some("image/heic".to_string());
        // No filename and no copied_path → AttachmentRender::MissingFilename.
        sticker.filename = None;
        sticker.transfer_name = None;
        sticker.copied_path = None;

        let mut ctx = empty_ctx();
        ctx.attachments = vec![sticker];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            actual.contains("<img class=\"inline_sticker\" loading=\"lazy\">"),
            "missing sticker should emit a bare inline <img> (no src, broken-image glyph): {actual}"
        );
        assert!(
            !actual.contains("attachment_error"),
            "must not splice error text into a bubble: {actual}"
        );
    }

    #[test]
    fn inline_bubble_carries_part_tapbacks() {
        // One inline Run (text + sticker) is a single part; both tapbacks
        // registered on that part index render on the one merged bubble.
        use std::collections::HashMap;

        let options = Options::fake_options(ExportType::Html);
        let mut config = Config::fake_app(options);
        let parent_guid = "INLINE-TAPBACK-PARENT-0001".to_string();

        let mut tapback_a = Config::fake_message();
        tapback_a.associated_message_type = Some(2000); // Loved
        tapback_a.associated_message_guid = Some(parent_guid.clone());
        let mut tapback_b = Config::fake_message();
        tapback_b.associated_message_type = Some(2001); // Liked
        tapback_b.associated_message_guid = Some(parent_guid.clone());

        let mut by_idx: HashMap<usize, Vec<imessage_database::tables::messages::Message>> =
            HashMap::new();
        // Both tapbacks belong to the single inline part (index 0).
        by_idx.insert(0, vec![tapback_a, tapback_b]);
        config.tapbacks.insert(parent_guid.clone(), by_idx);

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = parent_guid;
        message.text = Some("hi \u{FFFC}".to_string());
        // Text and inline sticker share one part (one Run).
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 3, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
        ])];

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];

        let actual = render_parts(&exporter, &message, &mut ctx);
        let bubble_count = actual.matches("<span class=\"bubble\"").count();
        assert_eq!(bubble_count, 1, "expected one merged bubble: {actual}");
        let tapback_count = actual.matches("<span class=\"tapback\">").count();
        assert_eq!(
            tapback_count, 2,
            "expected both of the part's tapbacks on the merged bubble, got {tapback_count}: {actual}"
        );
    }

    #[test]
    fn inline_bubble_carries_part_replies() {
        // One inline Run (text + sticker) is a single part; both reply threads
        // registered on that part index appear under the one merged bubble.
        use std::collections::HashMap;

        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("hi \u{FFFC}".to_string());
        // Text and inline sticker share one part (one Run).
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 3, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
        ])];

        let mut reply_a = Config::fake_message();
        reply_a.guid = "REPLY-A-GUID".to_string();
        reply_a.text = Some("re: text part".to_string());
        reply_a.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            13,
            vec![TextEffect::Default],
        )])];

        let mut reply_b = Config::fake_message();
        reply_b.guid = "REPLY-B-GUID".to_string();
        reply_b.text = Some("re: sticker".to_string());
        reply_b.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            11,
            vec![TextEffect::Default],
        )])];

        let mut replies_map: HashMap<usize, Vec<imessage_database::tables::messages::Message>> =
            HashMap::new();
        // Both replies belong to the single inline part (index 0).
        replies_map.insert(0, vec![reply_a, reply_b]);

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];
        ctx.replies_map = replies_map;

        let actual = render_parts(&exporter, &message, &mut ctx);
        let bubble_count = actual.matches("<span class=\"bubble\"").count();
        // Replies render their own bubbles too. The outer merged bubble plus
        // one bubble per reply means at least 3. What we're verifying is
        // that the two reply bodies both appear in the merged output.
        assert!(
            bubble_count >= 3,
            "expected merged bubble + at least 2 reply bubbles, got {bubble_count}: {actual}"
        );
        assert!(
            actual.contains("re: text part"),
            "reply A missing from merged bubble: {actual}"
        );
        assert!(
            actual.contains("re: sticker"),
            "reply B missing from merged bubble: {actual}"
        );
    }

    #[test]
    fn expressive_renders_once_on_merged_inline_bubble() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("hi \u{FFFC}".to_string());
        // Text and inline sticker share one part (one Run).
        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 3, vec![TextEffect::Default]),
            AttributedRange::inline_attachment(3, 6, AttachmentMeta::default()),
        ])];
        message.expressive_send_style_id =
            Some("com.apple.MobileSMS.expressivesend.confetti".to_string());

        let mut ctx = empty_ctx();
        ctx.attachments = vec![make_static_sticker(&config)];
        ctx.expressive = match message.get_expressive() {
            imessage_database::message_types::expressives::Expressive::None
            | imessage_database::message_types::expressives::Expressive::Unknown("") => None,
            other => Some(other),
        };

        let actual = render_parts(&exporter, &message, &mut ctx);
        let expressive_count = actual.matches("<span class=\"expressive\">").count();
        assert_eq!(
            expressive_count, 1,
            "merged inline bubble should fire the expressive marker once, got {expressive_count}: {actual}"
        );
    }

    #[test]
    fn animated_sticker_alone_is_not_jumbo() {
        // A single animated sticker stays as a block-style sticker; the
        // jumbomoji bucketing only applies to inline-eligible content.
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("\u{FFFC}".to_string());
        message.components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
            0,
            3,
            AttachmentMeta::default(),
        )])];

        let mut animated = make_static_sticker(&config);
        animated.mime_type = Some("image/heic-sequence".to_string());
        let mut ctx = empty_ctx();
        ctx.attachments = vec![animated];

        let actual = render_parts(&exporter, &message, &mut ctx);
        assert!(
            !actual.contains("jumbo") && !actual.contains("medium"),
            "animated sticker must not trigger jumbomoji sizing: {actual}"
        );
        assert!(
            actual.contains("<div class=\"sticker\">"),
            "expected block sticker rendering: {actual}"
        );
    }

    #[test]
    fn can_format_html_attachment_audio_transcript() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let message = Config::fake_message();

        let mut attachment = Config::fake_attachment();
        attachment.uti = Some("com.apple.coreaudio-format".to_string());
        attachment.transfer_name = Some("Audio Message.caf".to_string());
        attachment.filename = Some("Audio Message.caf".to_string());
        attachment.mime_type = None;

        let meta = AttachmentMeta {
            transcription: Some("Test".to_string()),
            ..Default::default()
        };

        let actual = exporter.format_attachment(&mut attachment, &message, &meta);

        assert_eq!(
            actual,
            AttachmentRender::Embedded(
                "<div>\n    <audio controls src=\"Audio Message.caf\" type=\"x-caf; codecs=opus\"> </audio>\n</div>\n<hr>\n<span class=\"transcription\">Transcription: Test</span>".to_string()
            )
        );
    }

    #[test]
    fn can_format_html_single_url_no_bundle_id() {
        let options = Options::fake_options(ExportType::Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();

        // Use test message payload from test database
        message.guid = "FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203".to_string();
        message.rowid = 123445;

        message.date = 674526582885055488;
        // Set the message components to a single url
        message.text = Some("https://example.com".to_string());
        message.components = vec![BubbleComponent::Run(vec![
                AttributedRange::text(
                    0,
                    84,
                    vec![
                        TextEffect::Link("https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/".to_string()),
                    ]
                ),
            ]),];

        let body = message.parse_body(config.data_source.db()).unwrap();
        message.apply_body(body);

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();

        assert_eq!(
            actual,
            "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=FAKEGUID-D0C8-4212-AA87-DD8AE4FD1203\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"app\"><a href=\"https://www.ghacks.net/2020/01/23/lastpass-no-longer-listed-on-the-chrome-web-store/\"><div class=\"app_header\"><img src=\"https://www.ghacks.net/wp-content/uploads/2020/01/lastpass-chrome-extension.png\" loading=\"lazy\" onerror=\"this.style.display='none'\"><div class=\"name\">gHacks Technology News</div></div><div class=\"app_footer\"><div class=\"caption\">LastPass no longer listed on the Chrome Web Store - gHacks Tech News</div><div class=\"subcaption\">LastPass customers and new users searching for password managers on Google&apos;s Chrome Web Store may have noticed that the LastPass extension for Google Chrome is currently no longer listed on the store.</div></div></a></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n"
        );
    }

    #[test]
    fn can_format_html_translated_message() {
        let mut options = Options::fake_options(ExportType::Html);
        options.attachment_manager.mode = AttachmentManagerMode::Clone;

        let mut config = Config::fake_app(options);
        config
            .translated_messages
            .insert("56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string());

        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "56FE94B9-2345-4A3C-A57F-949BDDDDF9FF".to_string();
        message.rowid = 548216;
        message
            .generate_text_legacy(config.data_source.db())
            .unwrap();

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"received\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=56FE94B9-2345-4A3C-A57F-949BDDDDF9FF\">Dec 31, 2000  4:00:00 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Unknown</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Oh, il a traduit ce que j&apos;ai envoyé !</span>\n    <div class=\"translated\"><span class=\"bubble\">Oh it translated what I sent!</span></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod balloon_format_tests {
    use std::{collections::HashMap, env::current_dir, fs::File, io::Read};

    use crate::{
        Config, HTML, Options, app::export_type::ExportType::Html,
        exporters::formatter::BalloonFormatter,
    };
    use imessage_database::message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        collaboration::CollaborationMessage,
        digital_touch::DigitalTouchMessage,
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::{Placemark, PlacemarkMessage},
        polls::{Poll, PollOption, PollOptionID, PollVote},
        url::URLMessage,
    };

    #[test]
    fn can_format_html_url() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: Some("title"),
            summary: Some("summary"),
            url: Some("url"),
            original_url: Some("original_url"),
            item_type: Some("item_type"),
            images: vec!["images"],
            icons: vec!["icons"],
            site_name: Some("site_name"),
            placeholder: false,
        };

        let actual = exporter.format_url(&Config::fake_message(), &balloon);
        let expected = "<a href=\"url\"><div class=\"app_header\"><img src=\"images\" loading=\"lazy\" onerror=\"this.style.display='none'\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_url_no_lazy() {
        let mut options = Options::fake_options(Html);
        options.no_lazy = true;
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: Some("title"),
            summary: Some("summary"),
            url: Some("url"),
            original_url: Some("original_url"),
            item_type: Some("item_type"),
            images: vec!["images"],
            icons: vec!["icons"],
            site_name: Some("site_name"),
            placeholder: false,
        };

        let actual = exporter.format_url(&Config::fake_message(), &balloon);
        let expected = "<a href=\"url\"><div class=\"app_header\"><img src=\"images\" onerror=\"this.style.display='none'\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_music() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: Some("preview"),
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
            lyrics: None,
        };

        let actual = exporter.format_music(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">track_name</div><audio controls src=\"preview\"> </audio></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_music_lyrics() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: None,
            artist: Some("artist"),
            album: Some("album"),
            track_name: Some("track_name"),
            lyrics: Some(vec!["a", "b"]),
        };

        let actual = exporter.format_music(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">track_name</div><div class=\"ldtext\"><p>a</p><p>b</p></div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn music_balloon_skips_empty_string_fields() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = MusicMessage {
            url: Some("url"),
            preview: None,
            artist: Some(""),
            album: Some(""),
            track_name: Some("track_name"),
            lyrics: None,
        };

        let actual = exporter.format_music(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">track_name</div></div><a href=\"url\"></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_collaboration() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = CollaborationMessage {
            original_url: Some("original_url"),
            url: Some("url"),
            title: Some("title"),
            creation_date: Some(0.),
            bundle_id: Some("bundle_id"),
            app_name: Some("app_name"),
        };

        let actual = exporter.format_collaboration(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">url</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_apple_pay() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let actual = exporter.format_apple_pay(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">app_name</div>\n</div><div class=\"app_footer\">\n    <div class=\"caption\">ldtext</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn apple_pay_balloon_emits_nothing_when_both_fields_missing() {
        // Apple Pay balloons with no `app_name` and no `ldtext` must render
        // nothing. `.app_footer` has a grey background + borders in style.css,
        // so an empty wrapper would render as a visible bordered strip.
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: None,
            title: None,
            subtitle: None,
            caption: None,
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: None,
            ldtext: None,
        };

        let actual = exporter.format_apple_pay(&balloon);
        assert_eq!(actual, "");
    }

    #[test]
    fn can_format_html_fitness() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let actual = exporter.format_fitness(&balloon);
        let expected = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_slideshow() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let actual = exporter.format_slideshow(&balloon);
        let expected = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_find_my() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let actual = exporter.format_find_my(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">app_name</div>\n</div><div class=\"app_footer\">\n    <div class=\"caption\">ldtext</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn find_my_balloon_emits_nothing_when_both_fields_missing() {
        // An empty Find My payload must not render bare `.app_header` /
        // `.app_footer` wrappers, which would show as a styled grey strip
        // with no content.
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: None,
            title: None,
            subtitle: None,
            caption: None,
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: None,
            ldtext: None,
        };

        let actual = exporter.format_find_my(&balloon);
        assert_eq!(actual, "");
    }

    #[test]
    fn can_format_html_check_in_timer() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_check_in_timer_late() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Has not checked in when expected, location shared"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Has not checked in when expected, location shared"),
        };

        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Has not checked in when expected, location shared</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_accepted_check_in() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Fake Location"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Fake Location"),
        };

        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Fake Location</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_app_store() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppStoreMessage {
            url: Some("url"),
            app_name: Some("app_name"),
            original_url: Some("original_url"),
            description: Some("description"),
            platform: Some("platform"),
            genre: Some("genre"),
        };

        let actual = exporter.format_app_store(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">description</div><div class=\"subcaption\">platform</div><div class=\"trailing_subcaption\">genre</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_app_store_no_url_with_original_url() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppStoreMessage {
            url: None,
            app_name: Some("app_name"),
            original_url: Some("original_url"),
            description: Some("description"),
            platform: Some("platform"),
            genre: Some("genre"),
        };

        let actual = exporter.format_app_store(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"original_url\"><div class=\"app_footer\"><div class=\"caption\">description</div><div class=\"subcaption\">platform</div><div class=\"trailing_subcaption\">genre</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_placemark() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = PlacemarkMessage {
            url: Some("url"),
            original_url: Some("original_url"),
            place_name: Some("Name"),
            placemark: Placemark {
                name: Some("name"),
                address: Some("address"),
                state: Some("state"),
                city: Some("city"),
                iso_country_code: Some("iso_country_code"),
                postal_code: Some("postal_code"),
                country: Some("country"),
                street: Some("street"),
                sub_administrative_area: Some("sub_administrative_area"),
                sub_locality: Some("sub_locality"),
            },
        };

        let actual = exporter.format_placemark(&balloon);
        let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">Name</div><div class=\"image_title\">name</div></div><div class=\"app_footer\"><div class=\"caption\">address</div><div class=\"trailing_caption\">postal_code</div><div class=\"subcaption\">country</div><div class=\"trailing_subcaption\">sub_administrative_area</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_poll() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut poll_options: HashMap<PollOptionID, PollOption> = HashMap::new();

        let id1: PollOptionID = "1".to_string();
        let id2: PollOptionID = "2".to_string();
        let id3: PollOptionID = "3".to_string();

        poll_options.insert(
            id1.clone(),
            PollOption {
                text: "Rust".to_string(),
                creator: "alice".to_string(),
                votes: vec![PollVote {
                    voter: "carol".to_string(),
                    option_id: id1.clone(),
                }],
            },
        );

        poll_options.insert(
            id2.clone(),
            PollOption {
                text: "Go".to_string(),
                creator: "bob".to_string(),
                votes: vec![
                    PollVote {
                        voter: "alice".to_string(),
                        option_id: id2.clone(),
                    },
                    PollVote {
                        voter: "bob".to_string(),
                        option_id: id2.clone(),
                    },
                ],
            },
        );

        poll_options.insert(
            id3.clone(),
            PollOption {
                text: "Python".to_string(),
                creator: "carol".to_string(),
                votes: vec![PollVote {
                    voter: "dave".to_string(),
                    option_id: id3.clone(),
                }],
            },
        );

        let poll = Poll {
            options: poll_options,
            order: vec![id1, id2, id3],
        };

        let actual = exporter.format_poll(&poll);
        let expected = "<div class=\"poll-container\"><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Rust</span><span class=\"vote-count\">1</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 50%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">carol</span></div></div><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Go</span><span class=\"vote-count\">2</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 100%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">alice</span><span class=\"voter\">bob</span></div></div><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Python</span><span class=\"vote-count\">1</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 50%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">dave</span></div></div></div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_quick_reply_prompt() {
        use imessage_database::message_types::business_chat::{
            BusinessMessage, QuickReply, QuickReplyOption,
        };

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::QuickReply(QuickReply {
            summary: Some("Choose an option".to_string()),
            options: vec![
                QuickReplyOption {
                    title: "Yes".to_string(),
                },
                QuickReplyOption {
                    title: "No".to_string(),
                },
            ],
            selected_index: None,
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Choose an option\n    </div>\n    <ul class=\"business-options\"><li\n            class=\"business-option\">Yes</li><li\n            class=\"business-option\">No</li>\n    </ul>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_quick_reply_selected() {
        use imessage_database::message_types::business_chat::{
            BusinessMessage, QuickReply, QuickReplyOption,
        };

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::QuickReply(QuickReply {
            summary: Some("Replied to a question".to_string()),
            options: vec![
                QuickReplyOption {
                    title: "Yes".to_string(),
                },
                QuickReplyOption {
                    title: "No".to_string(),
                },
            ],
            selected_index: Some(0),
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Replied to a question\n    </div>\n    <ul class=\"business-options\"><li\n            class=\"business-option selected\">Yes</li><li\n            class=\"business-option\">No</li>\n    </ul>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_form_request() {
        use imessage_database::message_types::business_chat::{BusinessMessage, FormRequest};

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::FormRequest(FormRequest {
            title: Some("Report an Issue".to_string()),
            subtitle: Some("Tap to get started".to_string()),
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Report an Issue</div><div class=\"business-subtitle\">Tap to get started</div></div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_form_response() {
        use imessage_database::message_types::business_chat::{
            BusinessMessage, FormAnswer, FormResponse,
        };

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::FormResponse(FormResponse {
            summary: Some("Here's my completed form".to_string()),
            answers: vec![
                FormAnswer {
                    question: "Which option best describes your request?".to_string(),
                    answers: vec!["The first example option".to_string()],
                },
                FormAnswer {
                    question: "When did this happen?".to_string(),
                    answers: vec!["01/01/2024".to_string()],
                },
                FormAnswer {
                    question: "Anything else to add?".to_string(),
                    answers: vec!["Example free-text response.".to_string()],
                },
            ],
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Here&apos;s my completed form\n    </div><dl class=\"business-answers\"><dt class=\"business-question\">Which option best describes your request?</dt>\n        <dd class=\"business-answer\">The first example option</dd><dt class=\"business-question\">When did this happen?</dt>\n        <dd class=\"business-answer\">01/01/2024</dd><dt class=\"business-question\">Anything else to add?</dt>\n        <dd class=\"business-answer\">Example free-text response.</dd>\n    </dl>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_list_picker_prompt() {
        use imessage_database::message_types::business_chat::{
            BusinessMessage, ListPicker, ListPickerItem,
        };

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::ListPicker(ListPicker {
            summary: Some("Select a Product".to_string()),
            items: vec![
                ListPickerItem {
                    title: "iPhone".to_string(),
                    subtitle: None,
                    selected: false,
                },
                ListPickerItem {
                    title: "AirPods".to_string(),
                    subtitle: Some("Wireless".to_string()),
                    selected: false,
                },
                ListPickerItem {
                    title: "Apple Watch".to_string(),
                    subtitle: None,
                    selected: false,
                },
            ],
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Select a Product\n    </div><ul class=\"business-options\"><li\n            class=\"business-option\">iPhone</li><li\n            class=\"business-option\">AirPods <span class=\"business-option-detail\">Wireless</span></li><li\n            class=\"business-option\">Apple Watch</li></ul>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_business_list_picker_reply() {
        use imessage_database::message_types::business_chat::{
            BusinessMessage, ListPicker, ListPickerItem,
        };

        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = BusinessMessage::ListPicker(ListPicker {
            summary: Some("Select a Product".to_string()),
            items: vec![
                ListPickerItem {
                    title: "iPhone".to_string(),
                    subtitle: None,
                    selected: true,
                },
                ListPickerItem {
                    title: "AirPods".to_string(),
                    subtitle: Some("Wireless".to_string()),
                    selected: false,
                },
                ListPickerItem {
                    title: "Apple Watch".to_string(),
                    subtitle: None,
                    selected: false,
                },
            ],
        });

        let actual = exporter.format_business(&balloon);
        let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Select a Product\n    </div><ul class=\"business-options\"><li\n            class=\"business-option selected\">iPhone</li><li\n            class=\"business-option\">AirPods <span class=\"business-option-detail\">Wireless</span></li><li\n            class=\"business-option\">Apple Watch</li></ul>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_generic_app() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: Some("image"),
            url: Some("url"),
            title: Some("title"),
            subtitle: Some("subtitle"),
            caption: Some("caption"),
            subcaption: Some("subcaption"),
            trailing_caption: Some("trailing_caption"),
            trailing_subcaption: Some("trailing_subcaption"),
            app_name: Some("app_name"),
            ldtext: Some("ldtext"),
        };

        let actual = exporter.format_generic_app(
            &balloon,
            "bundle_id",
            &mut vec![],
            &Config::fake_message(),
        );
        let expected = "<a href=\"url\"><div class=\"app_header\"><img src=\"image\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_digital_touch_kiss() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let payload_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/digital_touch_message/kiss.bin");
        let mut payload = vec![];
        File::open(payload_path)
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
        let balloon = DigitalTouchMessage::from_payload(&payload).unwrap();

        let msg = Config::fake_message();
        let actual = exporter.format_digital_touch(&msg, &balloon);

        assert!(actual.starts_with("<div class=\"digital_touch\">"));
        assert!(actual.contains("<svg"));
        assert!(actual.contains("<title>Digital Touch Kiss (1 kiss)</title>"));
        assert!(actual.contains("fill=\"red\""));
    }

    #[test]
    fn can_format_html_digital_touch_from_payload() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let payload_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/digital_touch_message/sketch.bin");
        let mut payload = vec![];
        File::open(payload_path)
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
        let touch = DigitalTouchMessage::from_payload(&payload).unwrap();

        let msg = Config::fake_message();
        let actual = exporter.format_digital_touch(&msg, &touch);

        assert!(actual.starts_with("<div class=\"digital_touch\">"));
        assert!(actual.contains("<polyline"));
        assert!(actual.contains("rgba(255, 0, 252, 1)"));
    }

    #[test]
    fn can_format_html_digital_touch_video_without_attachment() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let payload_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/digital_touch_message/video.bin");
        let mut payload = vec![];
        File::open(payload_path)
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
        let balloon = DigitalTouchMessage::from_payload(&payload).unwrap();

        let msg = Config::fake_message();
        let actual = exporter.format_digital_touch(&msg, &balloon);

        assert!(actual.starts_with("<div class=\"digital_touch\">"));
        assert!(actual.contains("<title>Digital Touch Video</title>"));
        assert!(actual.contains(">Video</text>"));
    }

    #[test]
    fn can_format_html_handwriting() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let payload_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/handwritten_message/handwriting.bin");
        let mut payload = vec![];
        File::open(payload_path)
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
        let balloon = HandwrittenMessage::from_payload(&payload).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/handwritten_message/handwriting.svg");
        File::open(expected_path)
            .unwrap()
            .read_to_string(&mut expected)
            .unwrap();

        let msg = Config::fake_message();
        let actual = exporter.format_handwriting(&msg, &balloon);

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_check_in_estimated_end_time() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&estimatedEndTime=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Expected at Oct 14, 2023  1:54:29 PM</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_check_in_trigger_time() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&triggerTime=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some("Check In: Timer Started"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In: Timer Started"),
        };

        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Was expected at Oct 14, 2023  1:54:29 PM</div>\n</div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_check_in_no_recognized_metadata() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1"),
            title: None,
            subtitle: None,
            caption: Some("Check In"),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check In"),
            ldtext: Some("Check In"),
        };

        // Without any of the three recognized timestamp keys the footer is
        // omitted entirely (CheckInVM.footer = None → check_in.html drops the
        // `<div class="app_footer">` block).
        let actual = exporter.format_check_in(&balloon);
        let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In</div></div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_poll_empty_options() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // Empty poll: `order` is empty, so max_votes = 0 (the unwrap_or guard
        // protects bar_width's `checked_div(0)` from panicking even though the
        // for-loop never executes).
        let poll = Poll {
            options: HashMap::new(),
            order: vec![],
        };

        let actual = exporter.format_poll(&poll);
        let expected = "<div class=\"poll-container\"></div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_poll_option_with_zero_votes() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut poll_options: HashMap<PollOptionID, PollOption> = HashMap::new();
        let id: PollOptionID = "1".to_string();
        poll_options.insert(
            id.clone(),
            PollOption {
                text: "Rust".to_string(),
                creator: "alice".to_string(),
                votes: vec![],
            },
        );

        let poll = Poll {
            options: poll_options,
            order: vec![id],
        };

        let actual = exporter.format_poll(&poll);
        let expected = "<div class=\"poll-container\"><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Rust</span><span class=\"vote-count\">0</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 0%;\"></div>\n        </div></div></div>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_url_no_site_name_falls_back_to_url() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: None,
            summary: None,
            url: Some("https://example.com"),
            original_url: None,
            item_type: None,
            images: vec![],
            icons: vec![],
            site_name: None,
            placeholder: false,
        };

        // No images → no <img>; no site_name → name falls back to balloon.url.
        // No title or summary → <div class="app_footer"> block is dropped.
        let actual = exporter.format_url(&Config::fake_message(), &balloon);
        let expected = "<a href=\"https://example.com\"><div class=\"app_header\"><div class=\"name\">https://example.com</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_url_no_url_falls_back_to_msg_text() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let balloon = URLMessage {
            title: None,
            summary: None,
            url: None,
            original_url: None,
            item_type: None,
            images: vec![],
            icons: vec![],
            site_name: None,
            placeholder: false,
        };

        let mut msg = Config::fake_message();
        msg.text = Some("https://example.com/from-text".to_string());

        // No balloon URL → wrapper_url and name both fall back to msg.text.
        let actual = exporter.format_url(&msg, &balloon);
        let expected = "<a href=\"https://example.com/from-text\"><div class=\"app_header\"><div class=\"name\">https://example.com/from-text</div></div></a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_collaboration_no_url_with_original_url() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        // wrapper_url is gated on balloon.url; footer_url uses get_url() which
        // falls back to original_url. With url=None + original_url=Some, the
        // <a> wrapper is dropped but the footer subcaption still appears.
        let balloon = CollaborationMessage {
            original_url: Some("https://example.com/original"),
            url: None,
            title: Some("Doc title"),
            creation_date: None,
            bundle_id: Some("bundle"),
            app_name: Some("App"),
        };

        let actual = exporter.format_collaboration(&balloon);
        let expected = "<div class=\"app_header\"><div class=\"name\">App</div></div><div class=\"app_footer\"><div class=\"caption\">Doc title</div><div class=\"subcaption\">https://example.com/original</div></div>";

        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod text_effect_tests {
    use std::borrow::Cow;

    use imessage_database::{
        message_types::text_effects::{
            animation::Animation,
            detected::{
                address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
                shipment_tracking::ShipmentTracking, unit::Unit,
            },
            style::Style,
            text_effect::TextEffect,
        },
        tables::messages::models::{AttributedRange, BubbleComponent},
    };

    use crate::{
        Config, HTML, Options,
        app::export_type::ExportType::Html,
        exporters::formatter::{MessageFormatter, RenderContext, TextEffectFormatter},
    };

    #[test]
    fn can_format_html_default() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_effect("Chris", &TextEffect::Default);
        let expected = "Chris";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_mention() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_mention("Chris", "+15558675309");
        let expected = "<span title=\"+15558675309\"><b>Chris</b></span>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_link() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_link("chrissardegna.com", "https://chrissardegna.com");
        let expected = "<a href=\"https://chrissardegna.com\">chrissardegna.com</a>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_otp() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_otp("123456");
        let expected = "<u>123456</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_style_single() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_styles("Bold", &[Style::Bold]);
        let expected = "<b>Bold</b>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_style_multiple() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_styles("Bold", &[Style::Bold, Style::Strikethrough]);
        let expected = "<s><b>Bold</b></s>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_style_all() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_styles(
            "Bold",
            &[
                Style::Bold,
                Style::Strikethrough,
                Style::Italic,
                Style::Underline,
            ],
        );
        let expected = "<u><i><s><b>Bold</b></s></i></u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_conversion("100 Miles", &Unit::Distance);
        let expected = "<u>100 Miles</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_address() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let address = DetectedAddress {
            full: "1 Apple Park Way, Cupertino, CA 95014".to_string(),
            street: Some("1 Apple Park Way".to_string()),
            street_number: Some("1".to_string()),
            street_name: Some("Apple Park Way".to_string()),
            city: Some("Cupertino".to_string()),
            state: Some("CA".to_string()),
            zip: Some("95014".to_string()),
            country: None,
            country_code: None,
        };
        let actual = exporter.format_address("1 Apple Park Way, Cupertino, CA 95014", &address);
        let expected = "<u>1 Apple Park Way, Cupertino, CA 95014</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_currency() {
        // Create exporter
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let currency = DetectedCurrency {
            symbol: "$".to_string(),
            amount: "16".to_string(),
        };
        let actual = exporter.format_currency("$16", &currency);
        let expected = "<u>$16</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_tracking() {
        // Create exporter
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let tracking = ShipmentTracking {
            carrier: Some("UPS".to_string()),
            number: "1Z999AA10123456784".to_string(),
        };
        let actual = exporter.format_tracking("1Z999AA10123456784", &tracking);
        let expected = "<u>1Z999AA10123456784</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_flight() {
        // Create exporter
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let flight = Flight {
            airline: Some("AS".to_string()),
            number: "1111".to_string(),
        };
        let actual = exporter.format_flight("AS 1111", &flight);
        let expected = "<u>AS 1111</u>";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_animated() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_animated("party", &Animation::Big);
        assert_eq!(actual, "<span class=\"animationBig\">party</span>");

        // Unknown(i64) round-trips its integer in the Debug form.
        let actual = exporter.format_animated("oops", &Animation::Unknown(42));
        assert_eq!(actual, "<span class=\"animationUnknown(42)\">oops</span>");
    }

    #[test]
    fn format_effect_default_is_borrowed() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let owned_text = String::from("hello");
        let result = exporter.format_effect(&owned_text, &TextEffect::Default);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "Default arm must not allocate"
        );

        let owned_url = String::from("https://example.com");
        let link = TextEffect::Link(owned_url);
        let result = exporter.format_effect(&owned_text, &link);
        assert!(
            matches!(result, Cow::Owned(_)),
            "Link arm wraps in <a> and must own"
        );
    }

    #[test]
    fn format_mention_escapes_name_to_prevent_attribute_injection() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_mention("Chris", "\"><script>alert(1)</script>");
        assert_eq!(
            actual,
            "<span title=\"&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;\"><b>Chris</b></span>"
        );
        assert!(
            !actual.contains("<script>"),
            "raw <script> must not survive"
        );
    }

    #[test]
    fn format_link_escapes_url_to_prevent_attribute_injection() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let actual = exporter.format_link("click me", "https://x.test/?q=\"><script>");
        assert_eq!(
            actual,
            "<a href=\"https://x.test/?q=&quot;&gt;&lt;script&gt;\">click me</a>"
        );
        assert!(
            !actual.contains("<script>"),
            "raw <script> must not survive"
        );
    }

    #[test]
    fn can_format_html_mention_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Test Dad ".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 5, vec![TextEffect::Default]),
            AttributedRange::text(5, 8, vec![TextEffect::Mention("+15558675309".to_string())]),
            AttributedRange::text(8, 9, vec![TextEffect::Default]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Test <span title=\"+15558675309\"><b>Dad</b></span> </span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_otp_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("000123 is your security code. Don't share your code.".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 6, vec![TextEffect::OTP]),
            AttributedRange::text(6, 52, vec![TextEffect::Default]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><u>000123</u> is your security code. Don&apos;t share your code.</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_link_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            56,
            vec![TextEffect::Link(
                "https://twitter.com/xxxxxxxxx/status/0000223300009216128".to_string(),
            )],
        )])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><a href=\"https://twitter.com/xxxxxxxxx/status/0000223300009216128\">https://twitter.com/xxxxxxxxx/status/0000223300009216128</a></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Hi. Right now or tomorrow?".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 17, vec![TextEffect::Default]),
            AttributedRange::text(17, 25, vec![TextEffect::Conversion(Unit::Timezone)]),
            AttributedRange::text(25, 26, vec![TextEffect::Default]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">Hi. Right now or <u>tomorrow</u>?</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_effect_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Big small shake nod explode ripple bloom jitter".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 3, vec![TextEffect::Animated(Animation::Big)]),
            AttributedRange::text(3, 4, vec![TextEffect::Default]),
            AttributedRange::text(4, 10, vec![TextEffect::Animated(Animation::Small)]),
            AttributedRange::text(10, 15, vec![TextEffect::Animated(Animation::Shake)]),
            AttributedRange::text(15, 16, vec![TextEffect::Animated(Animation::Small)]),
            AttributedRange::text(16, 19, vec![TextEffect::Animated(Animation::Nod)]),
            AttributedRange::text(19, 20, vec![TextEffect::Animated(Animation::Small)]),
            AttributedRange::text(20, 28, vec![TextEffect::Animated(Animation::Explode)]),
            AttributedRange::text(28, 34, vec![TextEffect::Animated(Animation::Ripple)]),
            AttributedRange::text(34, 35, vec![TextEffect::Animated(Animation::Explode)]),
            AttributedRange::text(35, 40, vec![TextEffect::Animated(Animation::Bloom)]),
            AttributedRange::text(40, 41, vec![TextEffect::Animated(Animation::Explode)]),
            AttributedRange::text(41, 47, vec![TextEffect::Animated(Animation::Jitter)]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><span class=\"animationBig\">Big</span> <span class=\"animationSmall\">small </span><span class=\"animationShake\">shake</span><span class=\"animationSmall\"> </span><span class=\"animationNod\">nod</span><span class=\"animationSmall\"> </span><span class=\"animationExplode\">explode </span><span class=\"animationRipple\">ripple</span><span class=\"animationExplode\"> </span><span class=\"animationBloom\">bloom</span><span class=\"animationExplode\"> </span><span class=\"animationJitter\">jitter</span></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Bold underline italic strikethrough all four".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 4, vec![TextEffect::Styles(vec![Style::Bold])]),
            AttributedRange::text(4, 5, vec![TextEffect::Default]),
            AttributedRange::text(5, 14, vec![TextEffect::Styles(vec![Style::Underline])]),
            AttributedRange::text(14, 15, vec![TextEffect::Default]),
            AttributedRange::text(15, 21, vec![TextEffect::Styles(vec![Style::Italic])]),
            AttributedRange::text(21, 22, vec![TextEffect::Default]),
            AttributedRange::text(22, 35, vec![TextEffect::Styles(vec![Style::Strikethrough])]),
            AttributedRange::text(35, 40, vec![TextEffect::Default]),
            AttributedRange::text(
                40,
                44,
                vec![TextEffect::Styles(vec![
                    Style::Bold,
                    Style::Strikethrough,
                    Style::Underline,
                    Style::Italic,
                ])],
            ),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><b>Bold</b> <u>underline</u> <i>italic</i> <s>strikethrough</s> all <i><u><s><b>four</b></s></u></i></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_single_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Everything".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            10,
            vec![TextEffect::Styles(vec![
                Style::Bold,
                Style::Strikethrough,
                Style::Underline,
                Style::Italic,
            ])],
        )])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><i><u><s><b>Everything</b></s></u></i></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styles_mixed_end_to_end() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("Underline normal jitter normal".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 9, vec![TextEffect::Styles(vec![Style::Underline])]),
            AttributedRange::text(9, 17, vec![TextEffect::Default]),
            AttributedRange::text(17, 23, vec![TextEffect::Animated(Animation::Jitter)]),
            AttributedRange::text(23, 30, vec![TextEffect::Default]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><u>Underline</u> normal <span class=\"animationJitter\">jitter</span> normal</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_plain_link() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text =
            Some("https://github.com/ReagentX/imessage-exporter/discussions/553".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            61,
            vec![
                TextEffect::Animated(Animation::Big),
                TextEffect::Link(
                    "https://github.com/ReagentX/imessage-exporter/discussions/553".to_string(),
                ),
            ],
        )])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><a href=\"https://github.com/ReagentX/imessage-exporter/discussions/553\"><span class=\"animationBig\">https://github.com/ReagentX/imessage-exporter/discussions/553</span></a></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_emoji_bold_underline() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("🅱️Bold_Underline".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, 7, vec![TextEffect::Default]),
            AttributedRange::text(7, 11, vec![TextEffect::Styles(vec![Style::Bold])]),
            AttributedRange::text(11, 12, vec![TextEffect::Default]),
            AttributedRange::text(12, 21, vec![TextEffect::Styles(vec![Style::Underline])]),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">🅱\u{fe0f}<b>Bold</b>_<u>Underline</u></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_text_styled_overlapping_ranges() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some("8:00 pm".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(
                0,
                1,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Bold]),
                ],
            ),
            AttributedRange::text(1, 2, vec![TextEffect::Conversion(Unit::Timezone)]),
            AttributedRange::text(
                2,
                4,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Underline]),
                ],
            ),
            AttributedRange::text(4, 5, vec![TextEffect::Conversion(Unit::Timezone)]),
            AttributedRange::text(
                5,
                7,
                vec![
                    TextEffect::Conversion(Unit::Timezone),
                    TextEffect::Styles(vec![Style::Italic]),
                ],
            ),
        ])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\"><b><u>8</u></b><u>:</u><u><u>00</u></u><u> </u><i><u>pm</u></i></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod edited_tests {
    use std::{env::current_dir, fs::File, io::Read};

    use crate::{
        Config, HTML, Options,
        app::export_type::ExportType::Html,
        exporters::formatter::{MessageFormatter, RenderContext},
    };
    use imessage_database::{
        message_types::{
            edited::{EditStatus, EditedEvent, EditedMessage, EditedMessagePart},
            text_effects::{style::Style, text_effect::TextEffect},
        },
        tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
    };

    #[test]
    fn can_format_html_edited_with_formatting() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let edited_message = EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Edited,
                edit_history: vec![
                    EditedEvent {
                        date: 758573156000000000,
                        text: "Test".to_string(),
                        components: vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Default],
                        )])],
                        guid: None,
                    },
                    EditedEvent {
                        date: 758573166000000000,
                        text: "Test".to_string(),
                        components: vec![BubbleComponent::Run(vec![AttributedRange::text(
                            0,
                            4,
                            vec![TextEffect::Styles(vec![Style::Strikethrough])],
                        )])],
                        guid: Some("76A466B8-D21E-4A20-AF62-FF2D3A20D31C".to_string()),
                    },
                ],
            }],
        };

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some("Test".to_string());
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(edited_message);

        let typedstream_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/typedstream/EditedWithFormatting");
        let mut file = File::open(typedstream_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        message.components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            4,
            vec![TextEffect::Styles(vec![Style::Strikethrough])],
        )])];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <div class=\"edited\"><table><tbody>\n        <tr>\n            <td><span class=\"timestamp\"></span></td>\n            <td>Test</td>\n        </tr>\n    </tbody><tfoot>\n        <tr>\n            <td><span class=\"timestamp\">Edited 10 seconds later</span></td>\n            <td><s>Test</s></td>\n        </tr>\n    </tfoot></table></div>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_final_unsent() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Original,
                    edit_history: vec![],
                },
                EditedMessagePart {
                    status: EditStatus::Unsent,
                    edit_history: vec![],
                },
            ],
        });

        message.components = vec![
            BubbleComponent::Run(vec![AttributedRange::text(
                0,
                28,
                vec![TextEffect::Default],
            )]),
            BubbleComponent::Run(vec![AttributedRange::attachment(
                28,
                31,
                AttachmentMeta {
                    guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None,
                },
            )]),
            BubbleComponent::Run(vec![AttributedRange::text(
                31,
                63,
                vec![TextEffect::Default],
            )]),
            BubbleComponent::Retracted,
        ];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">From arbitrary byte stream:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">To native Rust data structures:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"unsent\"><span class=\"unsent\">You unsent this message part 1 hour, 49 seconds after sending!</span></span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_no_edits() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.text = Some(
            "From arbitrary byte stream:\r\u{FFFC}To native Rust data structures:\r".to_string(),
        );
        message.is_from_me = true;
        message.chat_id = Some(0);

        message.components = vec![
            BubbleComponent::Run(vec![AttributedRange::text(
                0,
                28,
                vec![TextEffect::Default],
            )]),
            BubbleComponent::Run(vec![AttributedRange::attachment(
                28,
                31,
                AttachmentMeta {
                    guid: Some("D0551D89-4E11-43D0-9A0E-06F19704E97B".to_string()),
                    transcription: None,
                    height: None,
                    width: None,
                    name: None,
                },
            )]),
            BubbleComponent::Run(vec![AttributedRange::text(
                31,
                63,
                vec![TextEffect::Default],
            )]),
        ];

        let mut actual = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut actual)
            .unwrap();
        let expected = "<div class=\"message\">\n    <div class=\"sent iMessage\">\n        <p>\n            <span class=\"timestamp\">\n                <a title=\"Reveal in Messages app\" href=\"sms://open?message-guid=\">May 17, 2022  5:29:42 PM</a>\n                \n            </span>\n            \n            <span class=\"sender\">Me</span>\n        </p>\n        \n        \n        \n        \n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">From arbitrary byte stream:\r</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"attachment_error\">Attachment does not exist!</span>\n    </div>\n\n        \n        <hr>\n<div class=\"message_part\">\n    <span class=\"bubble\">To native Rust data structures:\r</span>\n    </div>\n\n        \n        \n    </div>\n</div>\n";

        assert_eq!(actual, expected);
    }

    #[test]
    fn can_format_html_conversion_fully_unsent() {
        let options = Options::fake_options(Html);
        let config = Config::fake_app(options);
        let exporter = HTML::new(&config).unwrap();

        let mut message = Config::fake_message();
        // May 17, 2022  8:29:42 PM
        message.date = 674526582885055488;
        message.date_edited = 674530231992568192;
        message.text = None;
        message.is_from_me = true;
        message.chat_id = Some(0);
        message.edited_parts = Some(EditedMessage {
            parts: vec![EditedMessagePart {
                status: EditStatus::Unsent,
                edit_history: vec![],
            }],
        });

        message.components = vec![];

        let mut actual = String::new();
        exporter.format_announcement(&message, &mut actual);
        let expected = "<div class=\"announcement\">\n    <p><span class=\"timestamp\">May 17, 2022  5:29:42 PM</span> You unsent a message.</p>\n</div>";

        assert_eq!(actual, expected);
    }
}
