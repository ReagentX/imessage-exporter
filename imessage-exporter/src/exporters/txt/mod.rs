use std::{convert::identity, fs::File, io::BufWriter};

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter, PartBodyBuilder, RenderContext},
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
            time::{format_timestamp, message_time},
        },
    },
};

use imessage_database::{
    message_types::{edited::EditedMessage, sticker::StickerDecoration},
    tables::{
        attachment::Attachment,
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, SharedLocation},
        },
    },
};

mod balloons;
mod view_model;

use view_model::{
    AnnouncementVM, AttachmentVM, EditedRow, EditedVM, MessagePartVM, MessageVM, PartBody,
    RepliesVM, StickerVM, TapbackVM, TapbacksVM,
};

/// Indentation prepended to every line of a reply rendered inside its
/// parent message's body. Top-level messages render at zero indent.
const REPLY_INDENT: &str = "    ";

const ATTACHMENT_MISSING_TEXT: &str = "Attachment missing!";

pub struct TXT<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Shared per-export state (file cache, orphaned writer, progress bar).
    pub state: ExportState,
}

impl<'a> TXT<'a> {
    pub fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(TXT {
            config,
            state: ExportState::new(config, "txt")?,
        })
    }
}

// MARK: Driver hooks
impl<'a> MessageWriter<'a> for TXT<'a> {
    const LABEL: &'static str = "txt";
    const BUFFER_CAPACITY: usize = 1024;

    fn config(&self) -> &'a Config {
        self.config
    }

    fn state(&self) -> &ExportState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ExportState {
        &mut self.state
    }

    fn write_file_header(_file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn write_file_footer(_file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn footer_notice() -> Option<&'static str> {
        None
    }
}

// MARK: Writer
impl<'a> MessageFormatter<'a> for TXT<'a> {
    fn format_attachment(
        &self,
        attachment: &'a Attachment,
        message: &'a Message,
        metadata: &AttachmentMeta,
    ) -> AttachmentRender {
        match prepare_attachment(self.config, &self.state, attachment, message) {
            Ok(file) => AttachmentRender::Embedded(self.render_attachment(&file, metadata)),
            Err(why) => why.into(),
        }
    }

    fn format_sticker(&self, sticker: &'a Attachment, message: &'a Message) -> String {
        match prepare_attachment(self.config, &self.state, sticker, message) {
            Ok(file) => self.render_sticker(sticker, &file, message),
            // Without a prepared file, omit the decoration and use only the
            // failure's display name for the attachment.
            Err(why) => render_template(&StickerVM {
                effect_prefix: None,
                who: self.config.who(
                    message.handle_id,
                    message.is_from_me(),
                    &message.destination_caller_id,
                ),
                path: match why {
                    AttachmentFailure::MissingFilename => String::new(),
                    AttachmentFailure::NamedFile(name) => name,
                },
                suffix: None,
            }),
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
            self.format_sticker(sticker, msg)
        })?
        else {
            return Ok(String::new());
        };
        Ok(render_template(&TapbackVM { kind }))
    }

    fn format_announcement(&self, msg: &Message, out: &mut String) {
        let kind = resolve_announcement(msg, self.config, YOU)
            .map_or(AnnouncementBody::Unknown, AnnouncementBody::from);
        render_template_into(&AnnouncementVM { kind }, out);
    }

    fn format_shareplay(&self) -> &'static str {
        "SharePlay Message\nEnded"
    }

    fn format_shared_location(&self, kind: SharedLocation) -> &'static str {
        match kind {
            SharedLocation::Started => "Started sharing location!",
            SharedLocation::Stopped => "Stopped sharing location!",
        }
    }

    fn format_edited(
        &'a self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        // Plain text can't embed media, so the edit history keeps the parsed
        // `\u{FFFC}` placeholder rather than inlining the sticker image.
        _attachments: &'a [Attachment],
        _resolver: &mut AttachmentResolver,
    ) -> Option<String> {
        let kind = normalize_edited(msg, edited_message, message_part_idx, self.config, YOU)?
            .map_rows(|event| {
                let timestamp_prefix = match event.diff_since_previous {
                    EditDiff::First => {
                        let mut s = format_timestamp(event.date, &self.config.options.timezone);
                        s.push(' ');
                        s
                    }
                    // Diff calculation failed; suppress the prefix.
                    EditDiff::Failed => String::new(),
                    EditDiff::Computed(diff) => format!("Edited {diff} later: "),
                };
                EditedRow {
                    timestamp_prefix,
                    text: event.text,
                }
            });

        Some(render_template(&EditedVM { kind }))
    }

    fn format_attributes(&self, text: &str, ranges: &[AttributedRange]) -> String {
        let mut formatted_text = String::with_capacity(text.len());
        let mut prev_start = 0;
        let mut prev_end = 0;

        for range in ranges {
            // Attachment ranges carry no text of their own.
            if range.attachment.is_some() {
                continue;
            }
            if prev_start == range.start && prev_end == range.end {
                continue;
            }
            if let Some(message_content) = text.get(range.start..range.end) {
                prev_start = range.start;
                prev_end = range.end;
                // There isn't really a way to represent formatted text in a plain text export
                formatted_text.push_str(message_content);
            }
        }
        formatted_text
    }

    fn render_run(
        &'a self,
        message: &'a Message,
        ranges: &'a [AttributedRange],
        attachments: &'a [Attachment],
        resolver: &mut AttachmentResolver,
    ) -> <Self as PartBodyBuilder>::Body {
        let text = message.text.as_deref().unwrap_or_default();

        // A run with no attachment ranges is a plain text bubble, rendered
        // exactly as the pre-refactor text component was, translation included.
        if ranges.iter().all(|range| range.attachment.is_none()) {
            let formatted = {
                let attr_text = self.format_attributes(text, ranges);
                if attr_text.is_empty() {
                    self.body_escape(text)
                } else {
                    attr_text
                }
            };
            return self.body_text_with_translation(message, formatted);
        }

        // Otherwise the run mixes text and/or attachments. Resolve attachment
        // ranges to their attachments (GUID-first, positional fallback) and
        // render each range as its own line, joined one-per-range, surfacing the
        // translation (if any) through the same shared helper as the pure-text
        // path so a translated sticker-bearing message keeps its translation.
        let mut lines: Vec<String> = Vec::with_capacity(ranges.len());
        for (range, idx) in resolve_run(ranges, resolver) {
            if let (Some(meta), Some(idx)) = (range.attachment.as_ref(), idx) {
                let line = match attachments.get(idx) {
                    Some(attachment) if attachment.is_sticker => {
                        self.format_sticker(attachment, message)
                    }
                    Some(attachment) => match self.format_attachment(attachment, message, meta) {
                        AttachmentRender::Embedded(content) => content,
                        AttachmentRender::MissingFilename => ATTACHMENT_MISSING_TEXT.to_string(),
                        AttachmentRender::NamedFile(name) => name,
                    },
                    None => ATTACHMENT_MISSING_TEXT.to_string(),
                };
                lines.push(line);
            } else {
                let segment = self.format_attributes(text, std::slice::from_ref(range));
                if !segment.is_empty() {
                    lines.push(segment);
                }
            }
        }
        self.body_text_with_translation(message, lines.join("\n"))
    }

    fn format_message_into(
        &self,
        message: &Message,
        context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError> {
        let mut ctx = MessageContext::resolve(
            message,
            self.config.data_source.db(),
            &self.config.data_source.capabilities,
        )?;
        let mut resolver = AttachmentResolver::new(&ctx.attachments);

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
            parts.push(MessagePartVM {
                body,
                expressive: ctx.expressive,
                tapbacks: build_tapbacks(self, message, idx, identity)?
                    .map(|tapbacks| TapbacksVM { tapbacks }),
                replies: build_replies(
                    self,
                    ctx.replies_map.get_mut(&idx),
                    Self::BUFFER_CAPACITY,
                    identity,
                )?
                .map(|replies| RepliesVM { replies }),
            });
        }

        let vm = MessageVM {
            timestamp: self.get_time(message),
            sender: self.config.who(
                message.handle_id,
                message.is_from_me(),
                &message.destination_caller_id,
            ),
            is_deleted: message.is_deleted(),
            subject: message.subject.as_deref(),
            shareplay: message.is_shareplay().then(|| self.format_shareplay()),
            shared_location: message
                .shared_location_kind()
                .map(|kind| self.format_shared_location(kind)),
            parts,
            is_reply: message.is_reply(),
            context,
        };

        match context {
            RenderContext::TopLevel => {
                render_template_into(&vm, out);
            }
            RenderContext::Reply => {
                // Render to a scratch buffer, then prefix every non-blank
                // line with REPLY_INDENT on the way out
                let mut buf = String::with_capacity(Self::BUFFER_CAPACITY);
                render_template_into(&vm, &mut buf);
                Self::push_indented(out, &buf, REPLY_INDENT);
            }
        }
        Ok(())
    }
}

// MARK: Part Body
impl PartBodyBuilder for TXT<'_> {
    type Body = PartBody;

    fn body_empty(&self) -> Self::Body {
        PartBody::Empty
    }

    fn body_text_bubble(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_text_translated(&self, translated: String, original: String) -> Self::Body {
        PartBody::Translated {
            translated,
            original,
        }
    }

    fn body_text_edited(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_attachment(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_attachment_error(&self, error: &str) -> Self::Body {
        self.state.errors.set(self.state.errors.get() + 1);
        PartBody::Line {
            text: error.to_string(),
        }
    }

    fn body_attachment_missing(&self) -> Self::Body {
        PartBody::Line {
            text: ATTACHMENT_MISSING_TEXT.to_string(),
        }
    }

    fn body_sticker(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_app(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_app_error(&self, _message: &Message, why: String) -> Self::Body {
        self.state.errors.set(self.state.errors.get() + 1);
        PartBody::Line {
            text: format!("Unable to format app message: {why}"),
        }
    }

    fn body_retracted(&self, content: String) -> Self::Body {
        PartBody::Line { text: content }
    }

    fn body_escape(&self, text: &str) -> String {
        text.to_string()
    }

    fn config(&self) -> &Config {
        self.config
    }
}

// MARK: Impl
impl TXT<'_> {
    /// Render a prepared attachment as its export-relative path, followed by
    /// the transcription line when available in `metadata`.
    fn render_attachment(&self, file: &ExportFile<'_>, metadata: &AttachmentMeta) -> String {
        render_template(&AttachmentVM {
            embed_path: self.config.relative_path(&file.path),
            transcription: metadata.transcription.as_deref(),
        })
    }

    /// Render a prepared sticker as its sender, path, and the decoration read
    /// from the row's file.
    fn render_sticker(
        &self,
        sticker: &Attachment,
        file: &ExportFile<'_>,
        message: &Message,
    ) -> String {
        let who = self.config.who(
            message.handle_id,
            message.is_from_me(),
            &message.destination_caller_id,
        );
        let path = self.render_attachment(file, &AttachmentMeta::default());

        let decoration = sticker.get_sticker_decoration(
            self.config.data_source.db(),
            &self.config.options.platform,
            &self.config.options.db_path,
            self.config.options.attachment_root.as_deref(),
        );

        let (effect_prefix, suffix) = match decoration {
            Some(StickerDecoration::GenmojiPrompt(prompt)) => {
                (None, Some(format!(" (Genmoji prompt: {prompt})")))
            }
            Some(StickerDecoration::Memoji) => (None, Some(" (App: Memoji)".to_string())),
            Some(StickerDecoration::Effect(effect)) => (Some(format!("{effect} ")), None),
            Some(StickerDecoration::AppName(name)) => (None, Some(format!(" (App: {name})"))),
            None => (None, None),
        };

        render_template(&StickerVM {
            effect_prefix,
            who,
            path,
            suffix,
        })
    }

    fn get_time(&self, message: &Message) -> String {
        let (mut date, read_receipt) = message_time(self.config, message);
        if read_receipt.is_empty() {
            date
        } else {
            date.push(' ');
            date.push_str(&read_receipt);
            date
        }
    }

    /// Append `source` to `out`, prefixing every non-blank line with `prefix`.
    /// Blank lines (`"\n"` only) pass through unprefixed so the output has
    /// no trailing-whitespace artifacts.
    fn push_indented(out: &mut String, source: &str, prefix: &str) {
        out.reserve(source.len() + prefix.len() * 4);
        for line in source.split_inclusive('\n') {
            if !line.trim_end_matches('\n').is_empty() {
                out.push_str(prefix);
            }
            out.push_str(line);
        }
    }
}

// MARK: Tests

#[cfg(test)]
mod tests;
