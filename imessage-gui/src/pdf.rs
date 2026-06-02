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
    io::BufWriter,
    path::{Path, PathBuf},
};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use printpdf::image_crate;
use printpdf::{
    BuiltinFont, Color, Image, ImageTransform, IndirectFontRef, Mm, PdfDocument,
    PdfDocumentReference, PdfLayerReference, Rect, Rgb,
};

use crate::model::{PreviewAttachment, PreviewMessage};

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
const IMAGE_GAP: f32 = 4.0; // pt between embedded image thumbnails

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
        if let Some(bytes) = system_sans_font() {
            if let (Ok(font), Ok(metrics)) = (
                doc.add_external_font(bytes.as_slice()),
                FontVec::try_from_vec(bytes.clone()),
            ) {
                return Ok(Typeface {
                    pdf_font: font,
                    metrics: Some(metrics),
                });
            }
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
        match &self.metrics {
            Some(font) => {
                let scaled = font.as_scaled(PxScale::from(size));
                text.chars()
                    .map(|c| scaled.h_advance(font.glyph_id(c)))
                    .sum()
            }
            // Helvetica average advance is ~0.5em; good enough for fallback.
            None => text.chars().count() as f32 * size * 0.5,
        }
    }

    /// Greedy word-wrap `text` to `max_width` pt at `size` pt.
    fn wrap(&self, text: &str, size: f32, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            if paragraph.is_empty() {
                lines.push(String::new());
                continue;
            }
            let mut line = String::new();
            for word in paragraph.split(' ') {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.width(&candidate, size) <= max_width || line.is_empty() {
                    // If a single word is too wide, hard-split it by characters.
                    if line.is_empty() && self.width(word, size) > max_width {
                        let mut chunk = String::new();
                        for ch in word.chars() {
                            let trial = format!("{chunk}{ch}");
                            if self.width(&trial, size) > max_width && !chunk.is_empty() {
                                lines.push(std::mem::take(&mut chunk));
                            }
                            chunk.push(ch);
                        }
                        line = chunk;
                    } else {
                        line = candidate;
                    }
                } else {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                }
            }
            lines.push(line);
        }
        lines
    }
}

/// A bubble laid out and ready to draw.
struct Bubble {
    from_me: bool,
    header: String,
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
    let max_inner = content_w * BUBBLE_MAX_FRAC - 2.0 * BUBBLE_PAD;
    let header = format!("{} · {}", msg.sender, msg.timestamp);
    let body_lines = if msg.text.trim().is_empty() {
        Vec::new()
    } else {
        face.wrap(&msg.text, BODY_SIZE, max_inner)
    };
    let (image_rows, image_chips) = layout_image_rows(&msg.attachments, max_inner);
    let mut chips = pdf_chips(msg);
    chips.extend(image_chips);

    // Inner content width = widest line we actually draw.
    let mut inner_w = face.width(&header, HEADER_SIZE);
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

    let mut content_h = HEADER_SIZE;
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
        header,
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
        let mut cursor = bubble_top - BUBBLE_PAD;
        let ty = cursor - HEADER_SIZE;

        page.layer.set_fill_color(header_col.clone());
        page.layer.use_text(
            bubble.header.as_str(),
            HEADER_SIZE,
            mm(text_x),
            mm(ty),
            &face.pdf_font,
        );
        cursor = ty;

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
            msg(true, "Doing great — here is a much longer message that should wrap across multiple lines within the bubble to exercise the word-wrapping logic."),
        ];
        render(&messages, "Sample Conversation", &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        let _ = std::fs::remove_file(&out);
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
