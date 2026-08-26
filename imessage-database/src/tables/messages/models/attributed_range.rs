/*!
 Attribute runs of a message body's `NSAttributedString`.
*/

use crate::{
    message_types::text_effects::text_effect::TextEffect,
    tables::messages::models::attachment_meta::AttachmentMeta,
};

/// One attribute run of a message's [`NSAttributedString`](crate::util::typedstream)
/// body: a byte range into the [`Message`](crate::tables::messages::Message)'s [`text`](crate::tables::messages::Message::text)
/// plus every attribute applied to it.
///
/// A range is a *text* range when [`attachment`](Self::attachment) is `None` and
/// an *attachment* range (a `\u{FFFC}` placeholder for an inline attachment)
/// when it is `Some`. Effects, styles, and the inline-emoji hint apply to either
/// kind. The [`typedstream`](crate::util::typedstream) attribute dictionary is a flat bag, so an attachment
/// range can also carry, say, an [`Animated`](TextEffect::Animated) effect.
///
/// Ranges that share a `__kIMMessagePartAttributeName` index are grouped into one
/// [`BubbleComponent::Run`](crate::tables::messages::models::BubbleComponent::Run). For example, message text with a
/// [`Mention`](TextEffect::Mention) like:
///
/// ```
/// let message_text = "What's up, Christopher?";
/// ```
///
/// parses into a single run of 3 ranges:
///
/// ```
/// use imessage_database::message_types::text_effects::text_effect::TextEffect;
/// use imessage_database::tables::messages::models::{AttributedRange, BubbleComponent};
///
/// let result = vec![BubbleComponent::Run(vec![
///     AttributedRange::text(0, 11, vec![TextEffect::Default]),  // `What's up, `
///     AttributedRange::text(11, 22, vec![TextEffect::Mention("+5558675309".to_string())]), // `Christopher`
///     AttributedRange::text(22, 23, vec![TextEffect::Default])  // `?`
/// ])];
/// ```
#[derive(Debug, PartialEq, Clone)]
pub struct AttributedRange {
    /// Start byte index in the message text.
    pub start: usize,
    /// End byte index in the message text.
    pub end: usize,
    /// Effects applied to this range.
    pub effects: Vec<TextEffect>,
    /// `Some` when this range is a `\u{FFFC}` placeholder for an attachment.
    /// The attachment's metadata travels here; effects still apply alongside.
    pub attachment: Option<AttachmentMeta>,
    /// `true` when the range carries `__kIMEmojiImageAttributeName`–Apple's
    /// hint to render the attachment inline–like an emoji (observed on
    /// genmoji, Memoji, and custom sticker ranges).
    pub emoji_image: bool,
}

impl AttributedRange {
    /// Build a text range (no attachment, no inline-emoji hint) with the
    /// specified start index, end index, and text effects.
    #[must_use]
    pub fn text(start: usize, end: usize, effects: Vec<TextEffect>) -> Self {
        Self {
            start,
            end,
            effects,
            attachment: None,
            emoji_image: false,
        }
    }

    /// Build an attachment range carrying the given [`AttachmentMeta`], with
    /// no inline-emoji hint.
    #[must_use]
    pub fn attachment(start: usize, end: usize, meta: AttachmentMeta) -> Self {
        Self {
            start,
            end,
            effects: vec![],
            attachment: Some(meta),
            emoji_image: false,
        }
    }

    /// Build an inline-rendered attachment range, one Apple flagged with
    /// `__kIMEmojiImageAttributeName` to render inline like an emoji (a Memoji,
    /// genmoji, or custom sticker placed amongst text).
    #[must_use]
    pub fn inline_attachment(start: usize, end: usize, meta: AttachmentMeta) -> Self {
        Self {
            start,
            end,
            effects: vec![],
            attachment: Some(meta),
            emoji_image: true,
        }
    }

    /// `true` when this range stands in for an attachment rather than text.
    #[must_use]
    pub fn is_attachment(&self) -> bool {
        self.attachment.is_some()
    }
}
