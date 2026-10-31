use askama::Template;

use imessage_database::{
    message_types::{expressives::Expressive, variants::Announcement},
    tables::messages::models::GroupAction,
};

use crate::exporters::{
    formatter::RenderContext,
    shared::{
        announcement::AnnouncementBody, edited::Edit, reply::ReplyEntry, tapback::TapbackKind,
        text::OptionalText,
    },
};

#[derive(Template)]
#[template(path = "balloons/apple_pay.txt")]
pub(super) struct ApplePayVM<'a> {
    pub caption: OptionalText<'a>,
    pub ldtext: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/fitness.txt")]
pub(super) struct FitnessVM<'a> {
    pub app_name: OptionalText<'a>,
    pub ldtext: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/slideshow.txt")]
pub(super) struct SlideshowVM<'a> {
    pub ldtext: OptionalText<'a>,
    pub url: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/find_my.txt")]
pub(super) struct FindMyVM<'a> {
    pub app_name: OptionalText<'a>,
    pub ldtext: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/check_in.txt")]
pub(super) struct CheckInVM<'a> {
    /// Resolved label: `balloon.caption.unwrap_or("Check In")`.
    pub caption: &'a str,
    /// Pre-formatted footer line (e.g., `"Checked in at Oct 14, …"`). `None`
    /// when the metadata yields no recognized timestamp.
    pub footer: Option<String>,
}

#[derive(Template)]
#[template(path = "balloons/poll.txt")]
pub(super) struct PollVM<'a> {
    pub options: Vec<PollOptionVM<'a>>,
}

pub(super) struct PollOptionVM<'a> {
    pub text: &'a str,
    pub vote_count: usize,
    pub voters: Vec<&'a str>,
}

#[derive(Template)]
#[template(path = "balloons/business_quick_reply.txt")]
pub(super) struct QuickReplyVM<'a> {
    pub summary: OptionalText<'a>,
    pub options: Vec<QuickReplyOptionVM<'a>>,
}

pub(super) struct QuickReplyOptionVM<'a> {
    pub text: &'a str,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "balloons/business_form_response.txt")]
pub(super) struct FormResponseVM<'a> {
    pub summary: OptionalText<'a>,
    pub answers: Vec<FormAnswerVM<'a>>,
}

pub(super) struct FormAnswerVM<'a> {
    pub question: &'a str,
    pub answer: String,
}

#[derive(Template)]
#[template(path = "balloons/business_form_request.txt")]
pub(super) struct FormRequestVM<'a> {
    pub title: OptionalText<'a>,
    pub subtitle: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/business_list_picker.txt")]
pub(super) struct ListPickerVM<'a> {
    pub summary: OptionalText<'a>,
    pub items: Vec<ListPickerItemVM<'a>>,
}

pub(super) struct ListPickerItemVM<'a> {
    pub title: &'a str,
    pub subtitle: OptionalText<'a>,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "balloons/generic_app.txt")]
pub(super) struct GenericAppVM<'a> {
    /// `app_name` falling back to `bundle_id`.
    pub name: &'a str,
    pub title: OptionalText<'a>,
    pub subtitle: OptionalText<'a>,
    pub caption: OptionalText<'a>,
    pub subcaption: OptionalText<'a>,
    pub trailing_caption: OptionalText<'a>,
    pub trailing_subcaption: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/url.txt")]
pub(super) struct UrlVM<'a> {
    /// Resolved primary line: `balloon.get_url()` falling back to `msg.text`.
    pub primary: OptionalText<'a>,
    pub title: OptionalText<'a>,
    pub summary: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/music.txt")]
pub(super) struct MusicVM<'a> {
    pub lyrics: Option<&'a [&'a str]>,
    pub track_name: OptionalText<'a>,
    pub album: OptionalText<'a>,
    pub artist: OptionalText<'a>,
    pub url: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/collaboration.txt")]
pub(super) struct CollaborationVM<'a> {
    /// `app_name` falling back to `bundle_id`.
    pub name: OptionalText<'a>,
    pub has_label: bool,
    pub title: OptionalText<'a>,
    /// `balloon.get_url()`.
    pub url: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/app_store.txt")]
pub(super) struct AppStoreVM<'a> {
    pub app_name: OptionalText<'a>,
    pub description: OptionalText<'a>,
    pub platform: OptionalText<'a>,
    pub genre: OptionalText<'a>,
    pub url: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "balloons/placemark.txt")]
pub(super) struct PlacemarkVM<'a> {
    pub place_name: OptionalText<'a>,
    pub url: OptionalText<'a>,
    pub name: OptionalText<'a>,
    pub address: OptionalText<'a>,
    pub state: OptionalText<'a>,
    pub city: OptionalText<'a>,
    pub iso_country_code: OptionalText<'a>,
    pub postal_code: OptionalText<'a>,
    pub country: OptionalText<'a>,
    pub street: OptionalText<'a>,
    pub sub_administrative_area: OptionalText<'a>,
    pub sub_locality: OptionalText<'a>,
}

#[derive(Template)]
#[template(path = "attachments/attachment.txt")]
pub(super) struct AttachmentVM<'a> {
    pub embed_path: String,
    pub transcription: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "attachments/sticker.txt")]
pub(super) struct StickerVM<'a> {
    /// e.g. `"Outline "` (trailing space) for `UserGenerated` effects;
    /// `None` for the other variants.
    pub effect_prefix: Option<String>,
    pub who: &'a str,
    /// Rendered path from `render_attachment`, or the display name on
    /// preparation failure. Empty without filename metadata on the row.
    pub path: String,
    /// Pre-formatted parenthesized suffix like `" (App: Memoji)"`. `None` for
    /// `UserGenerated` (which uses `effect_prefix`) and on attachment errors.
    pub suffix: Option<String>,
}

#[derive(Template)]
#[template(path = "tapback.txt")]
pub(super) struct TapbackVM<'a> {
    pub kind: TapbackKind<'a, String>,
}

#[derive(Template)]
#[template(path = "edited.txt")]
pub(super) struct EditedVM<'a> {
    pub kind: Edit<'a, EditedRow<'a>>,
}

pub(super) struct EditedRow<'a> {
    /// Either `"{absolute_timestamp} "` for the first edit, or
    /// `"Edited {diff} later: "` for subsequent edits.
    pub timestamp_prefix: String,
    pub text: &'a str,
}

#[derive(Template)]
#[template(path = "announcement.txt")]
pub(super) struct AnnouncementVM<'a> {
    pub kind: AnnouncementBody<'a>,
}

#[derive(Template)]
#[template(path = "message.txt")]
pub(super) struct MessageVM<'a> {
    pub timestamp: String,
    pub sender: &'a str,
    pub is_deleted: bool,
    pub subject: Option<&'a str>,
    /// Static FaceTime or SharePlay marker.
    pub shareplay: Option<&'static str>,
    /// Static shared-location marker.
    pub shared_location: Option<&'static str>,
    pub parts: Vec<MessagePartVM<'a>>,
    /// Whether the source message is itself a reply
    pub is_reply: bool,
    /// Where this message sits in the rendered hierarchy
    pub context: RenderContext,
}

#[derive(Template)]
#[template(path = "message_part.txt")]
pub(super) struct MessagePartVM<'a> {
    pub body: PartBody,
    pub expressive: Option<Expressive<'a>>,
    pub tapbacks: Option<TapbacksVM>,
    pub replies: Option<RepliesVM>,
}

#[derive(Template)]
#[template(path = "tapbacks.txt")]
pub(super) struct TapbacksVM {
    /// Each entry is one fully-rendered tapback line (e.g. `"Loved by Me"`).
    pub tapbacks: Vec<String>,
}

#[derive(Template)]
#[template(path = "replies.txt")]
pub(super) struct RepliesVM {
    /// Each entry's `body` is a multi-line reply render with its own
    /// [`REPLY_INDENT`](super::REPLY_INDENT) already applied by the recursive
    /// [`format_message_into`](crate::exporters::formatter::MessageFormatter::format_message_into) call and ends
    /// in `\n`; the template adds a second `\n` after it so siblings are
    /// separated by a blank line. `guid` is unused by this template.
    pub replies: Vec<ReplyEntry<String>>,
}

/// Each variant becomes a single `add_line` style emission in the part body
/// (i.e., one indent prefix + content + trailing newline). `Translated` is
/// the only multi-line variant; the rest collapse to a single line.
pub(crate) enum PartBody {
    Empty,
    Line {
        text: String,
    },
    Translated {
        translated: String,
        original: String,
    },
}
