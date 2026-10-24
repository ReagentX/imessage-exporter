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
            announcement::{AnnouncementBody, YOU, resolve_announcement},
            attachment::{AttachmentFailure, ExportFile, prepare_attachment},
            balloon::dispatch_app_balloon,
            driver::{ExportState, MessageWriter},
            edited::{EditDiff, normalize_edited},
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
        attachment: &'a Attachment,
        message: &'a Message,
        metadata: &AttachmentMeta,
    ) -> AttachmentRender {
        match prepare_attachment(self.config, &self.state, attachment, message) {
            Ok(file) => {
                AttachmentRender::Embedded(self.render_attachment(attachment, &file, metadata))
            }
            Err(why) => why.into(),
        }
    }

    fn format_sticker(&self, sticker: &'a Attachment, message: &'a Message) -> String {
        match prepare_attachment(self.config, &self.state, sticker, message) {
            Ok(file) => self.render_sticker(sticker, &file),
            Err(AttachmentFailure::MissingFilename) => String::new(),
            Err(AttachmentFailure::NamedFile(name)) => sanitize_html(&name).into_owned(),
        }
    }

    fn format_app(
        &self,
        message: &'a Message,
        attachments: &[Attachment],
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
        attachments: &'a [Attachment],
        resolver: &mut AttachmentResolver,
    ) -> Option<String> {
        let kind = normalize_edited(msg, edited_message, message_part_idx, self.config, YOU)?
            .map_rows(|event| {
                let rendered_text = match event.components.first() {
                    // Interleave text ranges with glyph-sized `<img>`s as in
                    // `render_run` for live messages, so an edited Memoji/genmoji
                    // is displayed as its image instead of a bare `\u{FFFC}` placeholder.
                    Some(BubbleComponent::Run(ranges))
                        if ranges
                            .iter()
                            .any(|range| range.attachment.is_some() && range.emoji_image) =>
                    {
                        // Edit-history components are always parsed with
                        // `parse_body_typedstream`, with a file-transfer GUID
                        // on each attachment range. Resolving the same GUID
                        // repeatedly is safe: look it up without advancing the
                        // positional cursor. For GUID-less ranges, resolve only
                        // once per range to avoid advancing the cursor across rows.
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
                EditedRow {
                    is_last: event.is_last,
                    timestamp,
                    text_html: Html::trust(rendered_text),
                }
            });

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
        attachments: &'a [Attachment],
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
                body = match attachments.get(idx) {
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
        self.state.errors.set(self.state.errors.get() + 1);
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
        self.state.errors.set(self.state.errors.get() + 1);
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
                &ctx.attachments,
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
        attachments: &[Attachment],
        resolver: &mut AttachmentResolver,
    ) -> Vec<InlineSegment> {
        let resolved = resolve_run(ranges, resolver);
        let mut segments = Vec::with_capacity(resolved.len());
        for (range, idx) in resolved {
            match idx {
                Some(idx) => match attachments.get(idx) {
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
    fn format_sticker_inline(&self, sticker: &Attachment, message: &Message) -> String {
        let lazy = !self.config.options.no_lazy;
        match prepare_attachment(self.config, &self.state, sticker, message) {
            Ok(file) => self.render_sticker_inline(sticker, &file),
            Err(AttachmentFailure::MissingFilename) => render_template(&StickerInlineVM {
                lazy,
                embed_path: None,
                label: None,
            }),
            Err(AttachmentFailure::NamedFile(name)) => render_template(&StickerInlineVM {
                lazy,
                embed_path: Some(name),
                label: None,
            }),
        }
    }

    /// Render a prepared attachment as embed markup. Use the media type and
    /// embed path from `file` and the displayed size from the row.
    fn render_attachment(
        &self,
        attachment: &Attachment,
        file: &ExportFile<'_>,
        metadata: &AttachmentMeta,
    ) -> String {
        let embed_path = self.config.relative_path(&file.path);

        let variant = match &file.media_type {
            MediaType::Image(_) => AttachmentVariant::Image,
            // Duplicate the video source tag intentionally; see
            // https://github.com/ReagentX/imessage-exporter/issues/73
            MediaType::Video(media_type) => AttachmentVariant::Video { media_type },
            MediaType::Audio(media_type) => match metadata.transcription.as_deref() {
                Some(transcription) => AttachmentVariant::AudioTranscription {
                    media_type,
                    transcription,
                },
                None => AttachmentVariant::Audio { media_type },
            },
            MediaType::Text(_) | MediaType::Application(_) => AttachmentVariant::Download {
                filename: file.name,
                file_size: attachment.file_size(),
            },
            MediaType::Unknown => {
                if file.path.is_dir() {
                    AttachmentVariant::UnknownFolder {
                        filename: file.name,
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

        render_template(&AttachmentVM {
            lazy: !self.config.options.no_lazy,
            embed_path,
            variant,
        })
    }

    /// Render a prepared sticker as a block attachment, followed by a
    /// decoration suffix when available in the row's file.
    fn render_sticker(&self, sticker: &Attachment, file: &ExportFile<'_>) -> String {
        let mut sticker_embed = self.render_attachment(sticker, file, &AttachmentMeta::default());

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

    /// Render a prepared static sticker as a glyph-sized inline `<img>` with
    /// the decoration label in `title=`.
    fn render_sticker_inline(&self, sticker: &Attachment, file: &ExportFile<'_>) -> String {
        let label = sticker
            .get_sticker_decoration(
                self.config.data_source.db(),
                &self.config.options.platform,
                &self.config.options.db_path,
                self.config.options.attachment_root.as_deref(),
            )
            .map(|kind| sticker_decoration_label(&kind));

        render_template(&StickerInlineVM {
            lazy: !self.config.options.no_lazy,
            embed_path: Some(self.config.relative_path(&file.path)),
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
        file.write_all(br#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; connect-src 'none'; img-src 'self' file: data:; media-src 'self' file:; style-src 'self' 'unsafe-inline'; base-uri 'none'; form-action 'none'; object-src 'none'; frame-src 'none'">"#)?;
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
mod tests;
