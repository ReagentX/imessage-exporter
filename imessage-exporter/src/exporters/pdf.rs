//! Rich, styled PDF rendering of a conversation — built entirely in-process
//! with `printpdf` (no headless browser, no external programs), so the app
//! stays fully portable.
//!
//! Messages are drawn as colored chat bubbles (iMessage-style: sent on the
//! right, received on the left) with a proportional font, a sender/timestamp
//! header, word-wrapped body text, and styled chips for attachments, replies,
//! and edits. Text is measured with `ab_glyph` so wrapping is accurate for the
//! embedded proportional font; the font is embedded in the PDF, so the output
//! is self-contained.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    io::BufWriter,
    path::{Path, PathBuf},
};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use imessage_database::{
    tables::{
        attachment::{Attachment, MediaType},
        messages::{Message, models::BubbleComponent},
        table::Table,
    },
    util::{dates::format as fmt_date, query_context::QueryContext},
};
use printpdf::image_crate;
use printpdf::{
    BuiltinFont, Color, Image, ImageTransform, IndirectFontRef, Mm, PdfDocument,
    PdfDocumentReference, PdfLayerReference, Rect, Rgb,
};

use crate::app::{error::RuntimeError, runtime::Config, sanitizers::sanitize_filename};

#[derive(Clone, Debug)]
pub struct PreviewMessage {
    pub is_from_me: bool,
    pub sender: String,
    pub timestamp: String,
    pub text: String,
    /// Total attachment rows referenced by the message.
    pub attachment_count: usize,
    /// Image attachments resolved for PDF embedding.
    pub attachments: Vec<PreviewAttachment>,
    /// Short badges such as attachment/reply/edit markers.
    pub annotations: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PreviewAttachment {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfExportSummary {
    pub produced_files: usize,
    pub total_messages: usize,
}

struct PdfConversation {
    title: String,
    raw_chat_ids: BTreeSet<i32>,
}

#[must_use]
pub fn filename_stem(name: &str) -> String {
    let cleaned = sanitize_filename(name);
    let trimmed = cleaned.trim().trim_matches('.');
    let stem = if trimmed.is_empty() {
        "conversation"
    } else {
        trimmed
    };
    stem.chars().take(120).collect()
}

// A4 geometry in points (1 pt = 1/72"). printpdf takes Mm; we convert.
const PT_PER_MM: f32 = 72.0 / 25.4;
const PAGE_W_MM: f32 = 210.0;
const PAGE_H_MM: f32 = 297.0;
const MARGIN_MM: f32 = 15.0;

const HEADER_SIZE: f32 = 8.0; // sender · timestamp
const BODY_SIZE: f32 = 10.5; // message text
const CHIP_SIZE: f32 = 8.0; // attachment / reply chips
const TITLE_SIZE: f32 = 16.0;

const LINE_GAP: f32 = 2.0; // pt between wrapped lines
const PARA_GAP: f32 = 6.0; // pt between bubbles
const BUBBLE_PAD: f32 = 6.0; // pt padding inside a bubble
const BUBBLE_MAX_FRAC: f32 = 0.74; // bubble width as fraction of content width
const TEXT_MEASURE_SCALE: f32 = 1.38; // guard for PDF/font renderer metric differences
const TEXT_WRAP_SAFETY: f32 = 18.0; // pt guard inside the bubble after wrapping
const IMAGE_GAP: f32 = 4.0; // pt between embedded image thumbnails

pub fn export(config: &Config) -> Result<PdfExportSummary, RuntimeError> {
    create_dir_all(&config.options.export_path)?;

    let conversations = pdf_conversations(config);
    if conversations.is_empty() {
        return Err(RuntimeError::PdfError(
            "No conversations matched the current selection.".to_string(),
        ));
    }

    let total_convs = conversations.len();
    let mut summary = PdfExportSummary::default();
    let mut used_names: HashSet<String> = HashSet::new();

    for (idx, conv) in conversations.iter().enumerate() {
        if let Some(callback) = &config.progress_callback {
            callback(idx as u64, total_convs as u64);
        }

        let qc = query_for_conversation(&config.options.query_context, conv);
        let messages = collect_messages(config, &qc, usize::MAX, true)?;
        if messages.is_empty() {
            continue;
        }
        summary.total_messages += messages.len();

        let mut stem = filename_stem(&conv.title);
        let base = stem.clone();
        let mut n = 1;
        while !used_names.insert(stem.clone()) {
            n += 1;
            stem = format!("{base} ({n})");
        }

        let pdf_path = config.options.export_path.join(format!("{stem}.pdf"));
        render(&messages, &conv.title, &pdf_path).map_err(RuntimeError::PdfError)?;
        summary.produced_files += 1;
    }

    if let Some(callback) = &config.progress_callback {
        callback(total_convs as u64, total_convs as u64);
    }

    if summary.produced_files == 0 {
        return Err(RuntimeError::PdfError(
            "No messages matched the current filters.".to_string(),
        ));
    }

    Ok(summary)
}

fn pdf_conversations(config: &Config) -> Vec<PdfConversation> {
    let selected = config.options.query_context.selected_chat_ids.as_ref();
    let mut groups: BTreeMap<i32, BTreeSet<i32>> = BTreeMap::new();

    for raw in config.chatrooms.keys() {
        if selected.is_some_and(|ids| !ids.contains(raw)) {
            continue;
        }
        let real = *config.real_chatrooms.get(raw).unwrap_or(raw);
        groups.entry(real).or_default().insert(*raw);
    }

    if groups.is_empty() && selected.is_none() {
        return vec![PdfConversation {
            title: "Orphaned".to_string(),
            raw_chat_ids: BTreeSet::new(),
        }];
    }

    let mut out = Vec::with_capacity(groups.len());
    for raw_chat_ids in groups.into_values() {
        let title = raw_chat_ids
            .iter()
            .filter_map(|id| config.chatrooms.get(id))
            .find_map(|chat| chat.display_name().map(|title| title.to_string()))
            .or_else(|| {
                raw_chat_ids
                    .iter()
                    .filter_map(|id| config.chatrooms.get(id))
                    .map(|chat| chat.chat_identifier.clone())
                    .next()
            })
            .unwrap_or_else(|| "Conversation".to_string());

        out.push(PdfConversation {
            title,
            raw_chat_ids,
        });
    }

    out.sort_by_key(|conversation| conversation.title.to_lowercase());
    out
}

fn query_for_conversation(base: &QueryContext, conversation: &PdfConversation) -> QueryContext {
    let mut qc = QueryContext {
        start: base.start,
        end: base.end,
        selected_handle_ids: base.selected_handle_ids.clone(),
        selected_chat_ids: None,
    };
    qc.set_selected_chat_ids(conversation.raw_chat_ids.iter().copied().collect());
    qc
}

pub fn collect_messages(
    config: &Config,
    qc: &QueryContext,
    limit: usize,
    include_image_attachments: bool,
) -> Result<Vec<PreviewMessage>, RuntimeError> {
    let db = config.db();
    let mut statement = Message::stream_rows(db, qc)?;
    let mut out: Vec<PreviewMessage> = Vec::new();

    for row in Message::rows(&mut statement, [])? {
        let mut msg = row?;

        // Tapbacks and poll updates are rendered in context by the regular
        // exporters, so the PDF/preview list should not render them as their
        // own top-level bubbles either.
        if !msg.is_edited() && (msg.is_tapback() || msg.is_poll_vote() || msg.is_poll_update()) {
            continue;
        }

        if let Ok(body) = msg.parse_body(db) {
            msg.apply_body(body);
        }

        let sender = config
            .who(msg.handle_id, msg.is_from_me, &msg.destination_caller_id)
            .to_string();
        let timestamp = msg
            .date(config.offset)
            .map(|d| fmt_date(&d))
            .unwrap_or_default();
        let attachment_count = msg.num_attachments.max(0) as usize;
        let attachments = if include_image_attachments {
            collect_image_attachments(config, &msg)?
        } else {
            Vec::new()
        };

        out.push(PreviewMessage {
            is_from_me: msg.is_from_me,
            sender,
            timestamp,
            text: preview_text(&msg),
            attachment_count,
            attachments,
            annotations: preview_annotations(&msg),
        });

        if out.len() >= limit {
            break;
        }
    }

    Ok(out)
}

fn collect_image_attachments(
    config: &Config,
    msg: &Message,
) -> Result<Vec<PreviewAttachment>, RuntimeError> {
    let attachments = Attachment::from_message(config.db(), msg)?;
    let mut out = Vec::new();

    for mut attachment in ordered_attachments_for_message(msg, attachments) {
        if !is_pdf_image_candidate(&attachment) {
            continue;
        }

        if let Err(why) =
            config
                .options
                .attachment_manager
                .handle_attachment(msg, &mut attachment, config)
        {
            eprintln!(
                "Skipping PDF image attachment rowid={} for message rowid={}: {why}",
                attachment.rowid, msg.rowid
            );
            continue;
        }

        let path = attachment.copied_path.clone().or_else(|| {
            attachment
                .resolved_attachment_path(
                    &config.options.platform,
                    &config.options.db_path,
                    config.options.attachment_root.as_deref(),
                )
                .map(PathBuf::from)
        });
        let Some(path) = path.filter(|p| p.is_file()) else {
            continue;
        };

        let name = attachment
            .filename()
            .map(ToOwned::to_owned)
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "image attachment".to_string());

        out.push(PreviewAttachment { path, name });
    }

    Ok(out)
}

fn ordered_attachments_for_message(msg: &Message, attachments: Vec<Attachment>) -> Vec<Attachment> {
    if attachments.len() <= 1 {
        return attachments;
    }

    let by_guid: HashMap<String, usize> = attachments
        .iter()
        .enumerate()
        .filter_map(|(idx, attachment)| attachment.guid.clone().map(|guid| (guid, idx)))
        .collect();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    let mut next_positional = 0usize;

    for component in &msg.components {
        let BubbleComponent::Run(ranges) = component else {
            continue;
        };
        for range in ranges {
            if let Some(meta) = &range.attachment {
                let idx = meta
                    .guid
                    .as_deref()
                    .and_then(|guid| by_guid.get(guid).copied())
                    .unwrap_or_else(|| {
                        let idx = next_positional;
                        next_positional += 1;
                        idx
                    });
                if idx < attachments.len() && seen.insert(idx) {
                    order.push(idx);
                }
            }
        }
    }

    if order.is_empty() {
        return attachments;
    }

    for idx in 0..attachments.len() {
        if seen.insert(idx) {
            order.push(idx);
        }
    }

    let mut slots: Vec<Option<Attachment>> = attachments.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|idx| slots.get_mut(idx).and_then(Option::take))
        .collect()
}

fn is_pdf_image_candidate(attachment: &Attachment) -> bool {
    if matches!(attachment.mime_type(), MediaType::Image(_)) {
        return true;
    }

    attachment.extension().is_some_and(|ext| {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "gif" | "jpg" | "jpeg" | "png"
        )
    })
}

fn preview_text(msg: &Message) -> String {
    let raw = msg.text.clone().unwrap_or_default();
    let cleaned = raw.replace(['\u{FFFC}', '\u{FFFD}'], " ");
    let cleaned = cleaned.trim();
    if !cleaned.is_empty() {
        return cleaned.to_string();
    }
    if msg.is_url() {
        "Link".to_string()
    } else {
        String::new()
    }
}

fn preview_annotations(msg: &Message) -> Vec<String> {
    let mut annotations = Vec::new();
    if msg.has_attachments() {
        let n = msg.num_attachments;
        annotations.push(format!("{n} attachment{}", if n == 1 { "" } else { "s" }));
    }
    if msg.is_reply() {
        annotations.push("reply".to_string());
    }
    if msg.has_replies() {
        let n = msg.num_replies;
        annotations.push(format!("{n} repl{}", if n == 1 { "y" } else { "ies" }));
    }
    if msg.is_edited() {
        annotations.push("edited".to_string());
    }
    if msg.is_expressive() {
        annotations.push("effect".to_string());
    }
    annotations
}

fn mm(pt: f32) -> Mm {
    Mm(pt / PT_PER_MM)
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(Rgb::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        None,
    ))
}

/// Try to read a system proportional (sans) font for embedding + metrics.
fn system_sans_font() -> Option<Vec<u8>> {
    const CANDIDATES: &[&str] = &[
        // Windows
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
        r"C:\Windows\Fonts\verdana.ttf",
        // macOS
        "/Library/Fonts/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        // Linux
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

/// Font + metrics bundle. Uses an embedded TrueType font when available;
/// otherwise falls back to the builtin Helvetica with approximate metrics.
struct Typeface {
    pdf_font: IndirectFontRef,
    metrics: Option<FontVec>,
}

impl Typeface {
    fn load(doc: &PdfDocumentReference) -> Result<Self, String> {
        if let Some(bytes) = system_sans_font()
            && let (Ok(font), Ok(metrics)) = (
                doc.add_external_font(bytes.as_slice()),
                FontVec::try_from_vec(bytes.clone()),
            )
        {
            return Ok(Typeface {
                pdf_font: font,
                metrics: Some(metrics),
            });
        }
        let pdf_font = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| format!("could not load a PDF font: {e}"))?;
        Ok(Typeface {
            pdf_font,
            metrics: None,
        })
    }

    /// Width of `text` at `size` pt, in points.
    fn width(&self, text: &str, size: f32) -> f32 {
        let measured = match &self.metrics {
            Some(font) => {
                let scaled = font.as_scaled(PxScale::from(size));
                text.chars()
                    .map(|c| scaled.h_advance(font.glyph_id(c)))
                    .sum()
            }
            // Helvetica average advance is ~0.5em; good enough for fallback.
            None => text.chars().count() as f32 * size * 0.5,
        };
        measured * TEXT_MEASURE_SCALE
    }

    /// Greedy word-wrap `text` to `max_width` pt at `size` pt.
    fn wrap(&self, text: &str, size: f32, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            if paragraph.trim().is_empty() {
                lines.push(String::new());
                continue;
            }
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                for piece in self.split_word(word, size, max_width) {
                    let candidate = if line.is_empty() {
                        piece.clone()
                    } else {
                        format!("{line} {piece}")
                    };
                    if self.width(&candidate, size) <= max_width || line.is_empty() {
                        line = candidate;
                    } else {
                        lines.push(std::mem::take(&mut line));
                        line = piece;
                    }
                }
            }
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }

    fn split_word(&self, word: &str, size: f32, max_width: f32) -> Vec<String> {
        if self.width(word, size) <= max_width {
            return vec![word.to_string()];
        }

        let mut chunks = Vec::new();
        let mut chunk = String::new();
        for ch in word.chars() {
            let trial = format!("{chunk}{ch}");
            if self.width(&trial, size) > max_width && !chunk.is_empty() {
                chunks.push(std::mem::take(&mut chunk));
            }
            chunk.push(ch);
        }
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        chunks
    }
}

/// A bubble laid out and ready to draw.
struct Bubble {
    from_me: bool,
    header_lines: Vec<String>,
    body_lines: Vec<String>,
    chips: Vec<String>,
    image_rows: Vec<ImageRow>,
    inner_width_pt: f32,
    width_pt: f32,
    height_pt: f32,
}

struct ImageRow {
    images: Vec<BubbleImage>,
    width_pt: f32,
    height_pt: f32,
}

struct BubbleImage {
    path: PathBuf,
    name: String,
    width_px: u32,
    height_px: u32,
    width_pt: f32,
    height_pt: f32,
}

fn layout_bubble(face: &Typeface, msg: &PreviewMessage, content_w: f32) -> Bubble {
    let max_inner = (content_w * BUBBLE_MAX_FRAC - 2.0 * BUBBLE_PAD).max(BODY_SIZE);
    let wrap_inner = (max_inner - TEXT_WRAP_SAFETY).max(BODY_SIZE);
    let header = format!("{} · {}", msg.sender, msg.timestamp);
    let header_lines = face.wrap(&header, HEADER_SIZE, wrap_inner);
    let body_lines = if msg.text.trim().is_empty() {
        Vec::new()
    } else {
        face.wrap(&msg.text, BODY_SIZE, wrap_inner)
    };
    let (image_rows, image_chips) = layout_image_rows(&msg.attachments, max_inner);
    let mut chips = pdf_chips(msg);
    chips.extend(image_chips);

    // Inner content width = widest line we actually draw.
    let mut inner_w = HEADER_SIZE;
    for l in &header_lines {
        inner_w = inner_w.max(face.width(l, HEADER_SIZE));
    }
    for l in &body_lines {
        inner_w = inner_w.max(face.width(l, BODY_SIZE));
    }
    for row in &image_rows {
        inner_w = inner_w.max(row.width_pt);
    }
    if !chips.is_empty() {
        inner_w = inner_w.max(face.width(&chips.join("   "), CHIP_SIZE));
    }
    inner_w = inner_w.min(max_inner);

    let mut content_h = 0.0;
    for (idx, _) in header_lines.iter().enumerate() {
        if idx > 0 {
            content_h += LINE_GAP;
        }
        content_h += HEADER_SIZE;
    }
    for _ in &body_lines {
        content_h += LINE_GAP + BODY_SIZE;
    }
    if !image_rows.is_empty() {
        content_h += IMAGE_GAP;
        for (idx, row) in image_rows.iter().enumerate() {
            if idx > 0 {
                content_h += IMAGE_GAP;
            }
            content_h += row.height_pt;
        }
    }
    if !chips.is_empty() {
        content_h += LINE_GAP + CHIP_SIZE;
    }
    let height = content_h + 2.0 * BUBBLE_PAD;

    Bubble {
        from_me: msg.is_from_me,
        header_lines,
        body_lines,
        chips,
        image_rows,
        inner_width_pt: inner_w,
        width_pt: inner_w + 2.0 * BUBBLE_PAD,
        height_pt: height,
    }
}

fn pdf_chips(msg: &PreviewMessage) -> Vec<String> {
    let mut chips = msg.annotations.clone();
    let image_count = msg.attachments.len();
    if image_count > 0 && msg.attachment_count > 0 {
        chips.retain(|chip| !chip.contains("attachment"));
        let other_count = msg.attachment_count.saturating_sub(image_count);
        if other_count > 0 {
            chips.insert(
                0,
                format!(
                    "{other_count} other attachment{}",
                    if other_count == 1 { "" } else { "s" }
                ),
            );
        }
    }
    chips
}

fn layout_image_rows(
    attachments: &[PreviewAttachment],
    max_inner: f32,
) -> (Vec<ImageRow>, Vec<String>) {
    if attachments.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let count = attachments.len();
    let columns = if count >= 6 {
        3usize
    } else if count >= 2 {
        2usize
    } else {
        1usize
    };
    let row_max_h = match count {
        0 | 1 => 210.0,
        2..=4 => 120.0,
        5..=8 => 84.0,
        _ => 54.0,
    };
    let cell_w = (max_inner - IMAGE_GAP * (columns.saturating_sub(1) as f32)) / columns as f32;

    let mut images = Vec::new();
    let mut chips = Vec::new();
    for attachment in attachments {
        match image_crate::image_dimensions(&attachment.path) {
            Ok((width_px, height_px)) if width_px > 0 && height_px > 0 => {
                let scale = (cell_w / width_px as f32)
                    .min(row_max_h / height_px as f32)
                    .min(1.0);
                images.push(BubbleImage {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    width_px,
                    height_px,
                    width_pt: width_px as f32 * scale,
                    height_pt: height_px as f32 * scale,
                });
            }
            _ => chips.push(format!("Image unavailable: {}", attachment.name)),
        }
    }

    let mut rows = Vec::new();
    for chunk in images.chunks(columns) {
        let width_pt = chunk.iter().map(|img| img.width_pt).sum::<f32>()
            + IMAGE_GAP * (chunk.len().saturating_sub(1) as f32);
        let height_pt = chunk
            .iter()
            .map(|img| img.height_pt)
            .fold(0.0_f32, f32::max);
        rows.push(ImageRow {
            images: chunk
                .iter()
                .map(|img| BubbleImage {
                    path: img.path.clone(),
                    name: img.name.clone(),
                    width_px: img.width_px,
                    height_px: img.height_px,
                    width_pt: img.width_pt,
                    height_pt: img.height_pt,
                })
                .collect(),
            width_pt,
            height_pt,
        });
    }

    (rows, chips)
}

struct Page {
    layer: PdfLayerReference,
}

/// Render the conversation `messages` to a styled PDF at `out_path`.
pub fn render(messages: &[PreviewMessage], title: &str, out_path: &Path) -> Result<(), String> {
    let (doc, page1, layer1) = PdfDocument::new(title, Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
    let face = Typeface::load(&doc)?;

    let content_left = MARGIN_MM * PT_PER_MM;
    let content_right = (PAGE_W_MM - MARGIN_MM) * PT_PER_MM;
    let content_w = content_right - content_left;
    let top = (PAGE_H_MM - MARGIN_MM) * PT_PER_MM;
    let bottom = MARGIN_MM * PT_PER_MM;

    let mut page = Page {
        layer: doc.get_page(page1).get_layer(layer1),
    };
    // y is the *baseline cursor* measured from the page bottom, in points.
    let mut y = top;

    let new_page = |doc: &PdfDocumentReference| -> Page {
        let (p, l) = doc.add_page(Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
        Page {
            layer: doc.get_page(p).get_layer(l),
        }
    };

    // Title
    page.layer.set_fill_color(rgb(20, 20, 20));
    page.layer.use_text(
        title,
        TITLE_SIZE,
        mm(content_left),
        mm(y - TITLE_SIZE),
        &face.pdf_font,
    );
    y -= TITLE_SIZE + PARA_GAP * 2.0;

    for msg in messages {
        let bubble = layout_bubble(&face, msg, content_w);

        // Page break if the bubble won't fit here. Very image-heavy bubbles may
        // be taller than a page, but they should at least start on a fresh page.
        if y - bubble.height_pt < bottom && (bubble.height_pt <= (top - bottom) || y < top) {
            page = new_page(&doc);
            y = top;
        }

        let bubble_left = if bubble.from_me {
            content_right - bubble.width_pt
        } else {
            content_left
        };
        let bubble_top = y;
        let bubble_bottom = y - bubble.height_pt;

        // Bubble background
        let (fill, header_col, body_col, chip_col) = if bubble.from_me {
            (
                rgb(0, 122, 255),
                rgb(225, 235, 255),
                rgb(255, 255, 255),
                rgb(220, 230, 255),
            )
        } else {
            (
                rgb(229, 229, 234),
                rgb(90, 90, 95),
                rgb(20, 20, 20),
                rgb(90, 90, 95),
            )
        };
        draw_rect(
            &page.layer,
            bubble_left,
            bubble_bottom,
            bubble.width_pt,
            bubble.height_pt,
            &fill,
        );

        // Content inside the bubble, drawn top-down.
        let text_x = bubble_left + BUBBLE_PAD;
        page.layer.set_fill_color(header_col.clone());
        let mut cursor = bubble_top - BUBBLE_PAD;
        for (idx, line) in bubble.header_lines.iter().enumerate() {
            if idx > 0 {
                cursor -= LINE_GAP;
            }
            cursor -= HEADER_SIZE;
            page.layer.use_text(
                line.as_str(),
                HEADER_SIZE,
                mm(text_x),
                mm(cursor),
                &face.pdf_font,
            );
        }

        page.layer.set_fill_color(body_col.clone());
        for line in &bubble.body_lines {
            cursor -= LINE_GAP + BODY_SIZE;
            page.layer.use_text(
                line.as_str(),
                BODY_SIZE,
                mm(text_x),
                mm(cursor),
                &face.pdf_font,
            );
        }

        if !bubble.image_rows.is_empty() {
            cursor -= IMAGE_GAP;
            for (row_idx, row) in bubble.image_rows.iter().enumerate() {
                if row_idx > 0 {
                    cursor -= IMAGE_GAP;
                }
                let row_top = cursor;
                let mut image_x = text_x + ((bubble.inner_width_pt - row.width_pt) / 2.0).max(0.0);
                for image in &row.images {
                    let image_y = row_top - image.height_pt;
                    draw_image(&page.layer, image, image_x, image_y, &face, &body_col);
                    image_x += image.width_pt + IMAGE_GAP;
                }
                cursor -= row.height_pt;
            }
        }

        if !bubble.chips.is_empty() {
            cursor -= LINE_GAP + CHIP_SIZE;
            page.layer.set_fill_color(chip_col.clone());
            page.layer.use_text(
                bubble.chips.join("   "),
                CHIP_SIZE,
                mm(text_x),
                mm(cursor),
                &face.pdf_font,
            );
        }

        y = bubble_bottom - PARA_GAP;
    }

    let file = std::fs::File::create(out_path)
        .map_err(|e| format!("cannot create {}: {e}", out_path.display()))?;
    doc.save(&mut BufWriter::new(file))
        .map_err(|e| format!("cannot write PDF {}: {e}", out_path.display()))?;
    Ok(())
}

/// Draw a filled rectangle given bottom-left origin and size in points.
fn draw_rect(layer: &PdfLayerReference, x: f32, y: f32, w: f32, h: f32, fill: &Color) {
    // `Rect::new` defaults to `PaintMode::Fill`, filled with the current fill color.
    let rect = Rect::new(mm(x), mm(y), mm(x + w), mm(y + h));
    layer.set_fill_color(fill.clone());
    layer.add_rect(rect);
}

fn draw_image(
    layer: &PdfLayerReference,
    image: &BubbleImage,
    x: f32,
    y: f32,
    face: &Typeface,
    fallback_color: &Color,
) {
    match image_crate::open(&image.path) {
        Ok(dynamic_image) => {
            let pdf_image = Image::from_dynamic_image(&dynamic_image);
            pdf_image.add_to_layer(
                layer.clone(),
                ImageTransform {
                    translate_x: Some(mm(x)),
                    translate_y: Some(mm(y)),
                    scale_x: Some(image.width_pt / image.width_px as f32),
                    scale_y: Some(image.height_pt / image.height_px as f32),
                    dpi: Some(72.0),
                    ..Default::default()
                },
            );
        }
        Err(_) => {
            draw_rect(
                layer,
                x,
                y,
                image.width_pt,
                image.height_pt,
                &rgb(245, 245, 245),
            );
            layer.set_fill_color(fallback_color.clone());
            layer.use_text(
                "Image unavailable",
                CHIP_SIZE,
                mm(x + 4.0),
                mm(y + (image.height_pt / 2.0).max(CHIP_SIZE)),
                &face.pdf_font,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(from_me: bool, text: &str) -> PreviewMessage {
        PreviewMessage {
            is_from_me: from_me,
            sender: if from_me { "Me".into() } else { "Alice".into() },
            timestamp: "Jan 01, 2024 12:00:00 PM".into(),
            text: text.into(),
            attachment_count: 1,
            attachments: Vec::new(),
            annotations: vec!["📎 1 attachment".into()],
        }
    }

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    #[test]
    fn renders_styled_pdf() {
        let dir = std::env::temp_dir().join("imessage-gui-pdf-rich-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("sample.pdf");
        let messages = vec![
            msg(false, "Hey, how are you?"),
            msg(
                true,
                "Doing great — here is a much longer message that should wrap across multiple lines within the bubble to exercise the word-wrapping logic.",
            ),
        ];
        render(&messages, "Sample Conversation", &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn wraps_long_tokens_after_existing_text() {
        let (doc, _, _) = PdfDocument::new("wrap-test", Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
        let face = Typeface::load(&doc).unwrap();
        let content_w = (PAGE_W_MM - (2.0 * MARGIN_MM)) * PT_PER_MM;
        let mut message = msg(
            true,
            "See this receipt https://example.com/abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        );
        message.attachment_count = 0;
        message.annotations.clear();

        let bubble = layout_bubble(&face, &message, content_w);
        let widest_body_line = bubble
            .body_lines
            .iter()
            .map(|line| face.width(line, BODY_SIZE))
            .fold(0.0_f32, f32::max);

        assert!(bubble.body_lines.len() > 2);
        assert!(widest_body_line <= bubble.inner_width_pt + 0.1);
        assert!(bubble.width_pt <= content_w * BUBBLE_MAX_FRAC + 0.1);
    }

    #[test]
    fn wraps_long_headers_within_bubble() {
        let (doc, _, _) = PdfDocument::new("header-test", Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
        let face = Typeface::load(&doc).unwrap();
        let content_w = (PAGE_W_MM - (2.0 * MARGIN_MM)) * PT_PER_MM;
        let mut message = msg(true, "Short message");
        message.sender = "A Very Long Contact Name That Should Not Clip Inside The PDF Bubble Even When The Sender Label Has Many Words".into();
        message.timestamp = "Jan 01, 2024 12:00:00 PM".into();

        let bubble = layout_bubble(&face, &message, content_w);
        let widest_header_line = bubble
            .header_lines
            .iter()
            .map(|line| face.width(line, HEADER_SIZE))
            .fold(0.0_f32, f32::max);

        assert!(bubble.header_lines.len() > 1);
        assert!(widest_header_line <= bubble.inner_width_pt + 0.1);
        assert!(bubble.width_pt <= content_w * BUBBLE_MAX_FRAC + 0.1);
    }

    #[test]
    fn renders_embedded_image_pdf() {
        let dir = std::env::temp_dir().join("imessage-gui-pdf-image-test");
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("pixel.png");
        std::fs::write(&image_path, PNG_1X1).unwrap();
        let out = dir.join("sample-image.pdf");

        let mut message = msg(false, "Photo attached:");
        message.attachments = vec![PreviewAttachment {
            path: image_path.clone(),
            name: "pixel.png".into(),
        }];

        render(&[message], "Sample Images", &out).unwrap();
        let pdf = std::fs::read(&out).unwrap();
        assert!(
            pdf.windows(b"/Subtype/Image".len())
                .any(|window| window == b"/Subtype/Image"),
            "PDF should contain an embedded image XObject"
        );

        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&image_path);
    }
}
