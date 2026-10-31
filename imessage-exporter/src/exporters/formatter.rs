use std::borrow::Cow;

use imessage_database::{
    message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        business_chat::BusinessMessage,
        collaboration::CollaborationMessage,
        digital_touch::DigitalTouchMessage,
        edited::EditedMessage,
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        polls::Poll,
        text_effects::{
            animation::Animation,
            detected::{
                address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
                shipment_tracking::ShipmentTracking, unit::Unit,
            },
            style::Style,
            text_effect::TextEffect,
        },
        url::URLMessage,
    },
    tables::{
        attachment::Attachment,
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, SharedLocation},
        },
    },
};

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::shared::{
        attachment::AttachmentFailure, balloon::rewrite_fitness_receiver, part::AttachmentResolver,
    },
};

/// Where a message sits in the rendered conversation hierarchy. Each exporter
/// applies its own decoration for [`Reply`](Self::Reply) (e.g. line prefixing,
/// anchor variants, suppression of top-level decorations).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderContext {
    /// Standalone message in the export.
    ///
    /// Emitted by [`crate::exporters::shared::driver::run_export`].
    TopLevel,
    /// Reply nested inside its parent message's body.
    ///
    /// Emitted from [`crate::exporters::shared::reply::build_replies`].
    Reply,
}

/// Outcome of [`MessageFormatter::format_attachment`]; each variant routes
/// to a different [`PartBodyBuilder`] hook.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AttachmentRender {
    /// Attachment was located and processed into an embed-ready string.
    Embedded(String),
    /// Attachment is present in the message but has no filename metadata.
    /// Caller renders via [`PartBodyBuilder::body_attachment_missing`].
    MissingFilename,
    /// Attachment manager couldn't process the file, but its filename is
    /// known. Caller renders via [`PartBodyBuilder::body_attachment_error`].
    NamedFile(String),
}

impl From<AttachmentFailure> for AttachmentRender {
    fn from(failure: AttachmentFailure) -> Self {
        match failure {
            AttachmentFailure::MissingFilename => AttachmentRender::MissingFilename,
            AttachmentFailure::NamedFile(name) => AttachmentRender::NamedFile(name),
        }
    }
}

// MARK: Message
/// Formatting hooks used by each exporter to render a message.
pub(crate) trait MessageFormatter<'a> {
    /// Format an attachment, possibly by reading the disk. The returned
    /// [`AttachmentRender`] tells the caller which body hook to invoke.
    fn format_attachment(
        &self,
        attachment: &'a Attachment,
        msg: &'a Message,
        metadata: &AttachmentMeta,
    ) -> AttachmentRender;
    /// Format a sticker, possibly by reading the disk.
    fn format_sticker(&self, attachment: &'a Attachment, msg: &'a Message) -> String;
    /// Format an app message from its payload and attachments.
    fn format_app(
        &self,
        msg: &'a Message,
        attachments: &[Attachment],
    ) -> Result<String, RuntimeError>;
    /// Format a tapback displayed under a message.
    fn format_tapback(&self, msg: &Message) -> Result<String, RuntimeError>;
    /// Render an announcement message directly into `out`. Permits reuse of
    /// the same buffer that [`format_message_into`](Self::format_message_into)
    /// uses, so the per-message hot path doesn't allocate per call.
    fn format_announcement(&self, msg: &Message, out: &mut String);
    /// Format a FaceTime or SharePlay row ([`Message::is_shareplay`]). Messages
    /// titles it SharePlay only when its
    /// [`conversation_av_mode`](Message::conversation_av_mode) is
    /// [`AVLess`](imessage_database::tables::messages::models::ConversationAVMode::AVLess),
    /// and FaceTime otherwise, including when the mode is absent.
    fn format_shareplay(&self, msg: &Message) -> &'static str;
    /// Format a legacy shared-location message.
    fn format_shared_location(&self, kind: SharedLocation) -> &'static str;
    /// Format an edited message by applying the edit's
    /// [`AttributedRange`]s to the original message text
    /// and interleaving inline attachments as in
    /// [`render_run`](Self::render_run).
    fn format_edited(
        &'a self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        attachments: &'a [Attachment],
        resolver: &mut AttachmentResolver,
    ) -> Option<String>;
    /// Format the text of a set of [`AttributedRange`]s applied to `text`.
    /// Attachment ranges are ignored; only text ranges contribute.
    fn format_attributes(&self, text: &str, ranges: &[AttributedRange]) -> String;
    /// Render one plain (non-edited) [`Run`](imessage_database::tables::messages::models::BubbleComponent::Run)
    /// (a bubble's worth of attributed ranges) into this format's part body.
    /// Interleaves text ranges with inline-attachment ranges, pairing each
    /// attachment to its row via `resolver` (GUID-first, positional fallback).
    /// Translation of the whole run is handled here so the dispatch stays
    /// format-agnostic.
    fn render_run(
        &'a self,
        message: &'a Message,
        ranges: &'a [AttributedRange],
        attachments: &'a [Attachment],
        resolver: &mut AttachmentResolver,
    ) -> <Self as PartBodyBuilder>::Body
    where
        Self: PartBodyBuilder;
    /// Render `message` directly into `out`. Permits reuse of a single buffer to
    /// avoid allocating per-message. `context` distinguishes the top-level
    /// driver pass from a nested-reply recursion (see [`RenderContext`]).
    fn format_message_into(
        &self,
        message: &Message,
        context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError>;
}

// MARK: Balloon
/// Formatting hooks for custom app balloons.
pub(crate) trait BalloonFormatter {
    /// Format a URL message.
    fn format_url(&self, msg: &Message, balloon: &URLMessage) -> String;
    /// Format an Apple Music message.
    fn format_music(&self, balloon: &MusicMessage) -> String;
    /// Format a Rich Collaboration message.
    fn format_collaboration(&self, balloon: &CollaborationMessage) -> String;
    /// Format an App Store link.
    fn format_app_store(&self, balloon: &AppStoreMessage) -> String;
    /// Format a shared location message.
    fn format_placemark(&self, balloon: &PlacemarkMessage) -> String;
    /// Format a handwritten note message.
    fn format_handwriting(&self, msg: &Message, balloon: &HandwrittenMessage) -> String;
    /// Format a digital touch message.
    fn format_digital_touch(&self, msg: &Message, balloon: &DigitalTouchMessage) -> String;
    /// Format an Apple Pay message.
    fn format_apple_pay(&self, balloon: &AppMessage) -> String;
    /// Format a Fitness message.
    fn format_fitness(&self, balloon: &AppMessage) -> String;
    /// Format a Photo Slideshow message.
    fn format_slideshow(&self, balloon: &AppMessage) -> String;
    /// Format a Find My message.
    fn format_find_my(&self, balloon: &AppMessage) -> String;
    /// Format a Check In message.
    fn format_check_in(&self, balloon: &AppMessage) -> String;
    /// Format a poll message.
    fn format_poll(&self, poll: &Poll) -> String;
    /// Format an Apple Business Chat message.
    fn format_business(&self, balloon: &BusinessMessage) -> String;
    /// Format an app message without a specialized renderer.
    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &[Attachment],
        msg: &Message,
    ) -> String;
}

// MARK: Part Body
/// Constructs the per-format part-body variant emitted by
/// [`shared::part::dispatch_part_body`](crate::exporters::shared::part::dispatch_part_body).
/// The dispatch owns the format-agnostic control flow (text vs attachment vs
/// app vs retracted, edit-check, attachment-index plumbing) and delegates leaf
/// wrapping to these constructors.
///
/// Method inputs that are `String` are already format-safe (either produced by
/// [`MessageFormatter::format_attributes`] / [`MessageFormatter::format_edited`]
/// / [`MessageFormatter::format_attachment`] etc., or pre-escaped via
/// [`Self::body_escape`]). Inputs that are `&str` are raw (e.g. attachment
/// error filenames) and each impl decides how to escape.
pub(crate) trait PartBodyBuilder {
    type Body;
    /// Empty body (no text, edited content missing, etc.)
    fn body_empty(&self) -> Self::Body;
    /// Text content with no special formatting (e.g. a non-edited text part).
    fn body_text_bubble(&self, content: String) -> Self::Body;
    /// Translated text content
    fn body_text_translated(&self, translated: String, original: String) -> Self::Body;
    /// Render `original` as a text bubble, pairing it with the message's
    /// translation when one applies.
    fn body_text_with_translation(&self, message: &Message, original: String) -> Self::Body {
        if let Ok(Some(translation)) = self.config().translation_for(message) {
            let safe_translated = self.body_escape(&translation.translated_text);
            return self.body_text_translated(safe_translated, original);
        }
        self.body_text_bubble(rewrite_fitness_receiver(original))
    }
    /// Edited text content
    fn body_text_edited(&self, content: String) -> Self::Body;
    /// Attachment content, generally by reference to an external file
    fn body_attachment(&self, content: String) -> Self::Body;
    /// Attachment that failed to export due to an error
    fn body_attachment_error(&self, error: &str) -> Self::Body;
    /// Attachment with missing filename metadata
    fn body_attachment_missing(&self) -> Self::Body;
    /// Sticker content, generally by reference to an external file
    fn body_sticker(&self, content: String) -> Self::Body;
    /// App message content
    fn body_app(&self, content: String) -> Self::Body;
    /// App message that failed to export due to an error. `why` is the
    /// already-rendered error message; implementations are responsible for
    /// any format-specific escaping.
    fn body_app_error(&self, message: &Message, why: String) -> Self::Body;
    /// Retracted message content
    fn body_retracted(&self, content: String) -> Self::Body;
    /// Escape raw user text for this format. Implementations decide what
    /// escaping (if any) is required. Used by the dispatch on the fallback
    /// and translation paths before handing strings to the variant constructors.
    fn body_escape(&self, text: &str) -> String;
    /// Surface the runtime [`Config`] so the dispatch can consult the translation
    /// set and open a DB connection without taking config as a free parameter.
    fn config(&self) -> &Config;
}

// MARK: Text Effects
/// Formatting hooks for text effects.
pub(crate) trait TextEffectFormatter<'a> {
    /// Format one [`TextEffect`].
    fn format_effect(&'a self, text: &'a str, effect: &'a TextEffect) -> Cow<'a, str>;
    /// Format a mention range.
    fn format_mention(&self, text: &str, mentioned: &str) -> String;
    /// Format a link range.
    fn format_link(&self, text: &str, url: &str) -> String;
    /// Format a one-time-password range.
    fn format_otp(&self, text: &str) -> String;
    /// Format a detected postal address range.
    fn format_address(&self, text: &str, address: &DetectedAddress) -> String;
    /// Format a detected unit-conversion range.
    fn format_conversion(&self, text: &str, unit: &Unit) -> String;
    /// Format a detected monetary amount range.
    fn format_currency(&self, text: &str, currency: &DetectedCurrency) -> String;
    /// Format a detected package tracking range.
    fn format_tracking(&self, text: &str, tracking: &ShipmentTracking) -> String;
    /// Format a detected flight reference range.
    fn format_flight(&self, text: &str, flight: &Flight) -> String;
    /// Format a styled text range.
    fn format_styles(&self, text: &str, styles: &[Style]) -> String;
    /// Format an animated text range.
    fn format_animated(&self, text: &str, animation: &Animation) -> String;
}
