/*!
Parser for [Digital Touch](https://support.apple.com/guide/ipod-touch/send-a-digital-touch-effect-iph3fadba219/ios) iMessages.

This message type is not documented by Apple, but represents messages displayed
as `com.apple.DigitalTouchBalloonProvider`. Each message is a `BaseMessage`
envelope wrapping an effect-specific protobuf selected by its `TouchKind`.

The effects share a handful of binary encodings, captured by the helpers here:

- Coordinates are little-endian `u16` pairs `(x, y)` where `0..=u16::MAX` spans
  the canvas in each axis, origin bottom-left, y growing upward ([`decode_points`]).
- Timing delays are little-endian `u16` milliseconds ([`decode_u16s`]).
- Colors are four bytes in RGBA order ([`Color`]).
*/

use std::borrow::Cow;

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, TouchKind},
        fireball::DigitalTouchFireball,
        heartbeat::DigitalTouchHeartbeat,
        kiss::DigitalTouchKiss,
        media::DigitalTouchMedia,
        sketch::DigitalTouchSketch,
        svg::Canvas,
        tap::DigitalTouchTap,
    },
};

/// A parsed [Digital Touch](https://support.apple.com/guide/ipod-touch/send-a-digital-touch-effect-iph3fadba219/ios) message.
///
/// Construct one with [`DigitalTouchMessage::from_payload`], then render it with
/// [`render_svg`](DigitalTouchMessage::render_svg) or
/// [`render_text`](DigitalTouchMessage::render_text).
#[derive(Debug, Clone, PartialEq)]
pub enum DigitalTouchMessage {
    /// One or more taps, each a colored burst at a point.
    Tap(DigitalTouchTap),
    /// A freehand drawing made of colored strokes.
    Sketch(DigitalTouchSketch),
    /// One or more kisses, each placed and rotated.
    Kiss(DigitalTouchKiss),
    /// A heartbeat, optionally breaking partway through.
    Heartbeat(DigitalTouchHeartbeat),
    /// A fireball dragged along a path.
    Fireball(DigitalTouchFireball),
    /// A photo or video; a photo may have effects drawn on top of it.
    Media(DigitalTouchMedia),
}

/// A still-image backdrop for [`render_svg`](DigitalTouchMessage::render_svg),
/// drawn behind the effect in place of the default black canvas.
///
/// Only a still image is ever a backdrop. The Digital Touch UI disables drawing
/// over video, so a video [`Media`](DigitalTouchMessage::Media) message has no
/// overlay to composite.
///
/// This is a render-time input, not parsed state: the caller resolves the image
/// via [`Attachment::from_message`](crate::tables::attachment::Attachment::from_message)
/// and supplies it here. The value becomes the SVG `<image>` `href` after XML
/// attribute escaping. The renderer treats it as an opaque reference and never
/// reads the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBackdrop<'a>(pub Cow<'a, str>);

impl ImageBackdrop<'_> {
    /// Render the `<image>` markup that displays this backdrop, sized to cover a
    /// `width`×`height` canvas (`slice`/`cover` crops the overflow).
    pub(super) fn render(&self, width: usize, height: usize) -> String {
        format!(
            r#"<image href="{}" x="0" y="0" width="{width}" height="{height}" preserveAspectRatio="xMidYMid slice" />"#,
            escape_attr(&self.0)
        )
    }
}

/// Escape the characters significant inside a double-quoted XML attribute value,
/// for references such as a backdrop `href`.
fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl DigitalTouchMessage {
    /// Convert a raw `payload_data` byte blob into a [`DigitalTouchMessage`].
    pub fn from_payload(payload: &[u8]) -> Result<Self, DigitalTouchError> {
        let msg =
            BaseMessage::parse_from_bytes(payload).map_err(DigitalTouchError::ProtobufError)?;
        Self::from_base(&msg)
    }

    /// Dispatch an already-parsed [`BaseMessage`] envelope to the parser for its
    /// [`TouchKind`]. Used both for top-level messages and for the nested effects
    /// a [`Media`](DigitalTouchMessage::Media) message draws over its photo.
    pub(super) fn from_base(msg: &BaseMessage) -> Result<Self, DigitalTouchError> {
        match msg.TouchKind.enum_value_or_default() {
            TouchKind::Tap => DigitalTouchTap::from_payload(msg),
            TouchKind::Sketch => DigitalTouchSketch::from_payload(msg),
            TouchKind::Kiss => DigitalTouchKiss::from_payload(msg),
            TouchKind::Heartbeat => DigitalTouchHeartbeat::from_payload(msg),
            TouchKind::Fireball => DigitalTouchFireball::from_payload(msg),
            TouchKind::Media => DigitalTouchMedia::from_payload(msg),
            TouchKind::Unknown => Err(DigitalTouchError::UnknownDigitalTouchKind(
                msg.TouchKind.value(),
            )),
        }
    }

    /// Render a static SVG depiction of the effect.
    ///
    /// `backdrop` optionally references a still image to draw behind the effect in
    /// place of the default black canvas. Pass `None` for a plain black background.
    #[must_use]
    pub fn render_svg(&self, backdrop: Option<ImageBackdrop<'_>>) -> String {
        let mut canvas = Canvas::new(self.summary(), backdrop);
        self.append_svg(&mut canvas);
        canvas.finish()
    }

    /// Append this effect's markup to `canvas`. Factored out of
    /// [`render_svg`](Self::render_svg) so a [`Media`](Self::Media) message can
    /// draw its overlay effects through the same dispatch.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        match self {
            DigitalTouchMessage::Tap(t) => t.append_svg(canvas),
            DigitalTouchMessage::Sketch(s) => s.append_svg(canvas),
            DigitalTouchMessage::Kiss(k) => k.append_svg(canvas),
            DigitalTouchMessage::Heartbeat(h) => h.append_svg(canvas),
            DigitalTouchMessage::Fireball(f) => f.append_svg(canvas),
            DigitalTouchMessage::Media(m) => m.append_svg(canvas),
        }
    }

    /// Render a one-line, human-readable summary of the effect.
    #[must_use]
    pub fn render_text(&self) -> String {
        self.summary()
    }

    /// Concise label used for both the text rendering and the SVG `<title>`.
    fn summary(&self) -> String {
        match self {
            DigitalTouchMessage::Tap(t) => t.summary(),
            DigitalTouchMessage::Sketch(s) => s.summary(),
            DigitalTouchMessage::Kiss(k) => k.summary(),
            DigitalTouchMessage::Heartbeat(h) => h.summary(),
            DigitalTouchMessage::Fireball(f) => f.summary(),
            DigitalTouchMessage::Media(m) => m.summary(),
        }
    }
}

/// A point captured by an effect, paired with effect-specific `extra` data
/// (a color, a delay, a rotation, …). Coordinates are normalized `u16` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point<T> {
    /// X coordinate, `0..=u16::MAX` spanning the canvas left-to-right.
    pub x: u16,
    /// Y coordinate, `0..=u16::MAX` spanning the canvas bottom-to-top
    /// (Digital Touch uses a bottom-left origin).
    pub y: u16,
    /// Effect-specific data associated with this point.
    pub extra: T,
}

/// An RGBA color, one byte per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl Color {
    /// Opaque white, used as a fallback when an effect carries no color.
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Decode a packed buffer of consecutive RGBA colors.
    #[must_use]
    pub fn decode_all(buf: &[u8]) -> Vec<Color> {
        buf.as_chunks::<4>()
            .0
            .iter()
            .map(|c| Color {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            })
            .collect()
    }

    /// Render as an SVG/CSS `rgba(…)` color string.
    #[must_use]
    pub fn css(&self) -> String {
        // SVG/CSS `rgba()` expects alpha in `0..=1`, not the raw `0..=255` byte.
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            f32::from(self.a) / 255.0
        )
    }
}

/// Decode a packed buffer of little-endian `u16` values (delays, rotations).
#[must_use]
pub fn decode_u16s(buf: &[u8]) -> Vec<u16> {
    buf.as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Decode `(x, y)` points from `raw` and zip each with the parallel `extras`.
///
/// Returns [`DigitalTouchError::ArraysDoNotMatch`] if the number of points does
/// not match the number of `extras`, which would mean the effect's parallel
/// arrays disagree on how many events it contains.
pub fn decode_points<T>(raw: &[u8], extras: Vec<T>) -> Result<Vec<Point<T>>, DigitalTouchError> {
    let coords: Vec<(u16, u16)> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            (
                u16::from_le_bytes([c[0], c[1]]),
                u16::from_le_bytes([c[2], c[3]]),
            )
        })
        .collect();

    if coords.len() != extras.len() {
        return Err(DigitalTouchError::ArraysDoNotMatch(
            "points",
            coords.len(),
            "values",
            extras.len(),
        ));
    }

    Ok(coords
        .into_iter()
        .zip(extras)
        .map(|((x, y), extra)| Point { x, y, extra })
        .collect())
}

/// Decode `count` concatenated strokes from a single buffer.
///
/// Each stroke is laid out as a two-byte per-stroke header (observed as zero), a
/// little-endian `u16` point count, then that many `(x, y)` points. `count` is
/// the number of strokes to read, e.g. a [`SketchMessage`](super::sketch)'s
/// `StrokesCount`.
pub fn decode_strokes(raw: &[u8], count: usize) -> Result<Vec<Vec<(u16, u16)>>, DigitalTouchError> {
    // Guard against pathological counts that would cause us to allocate more memory than the input buffer size
    let mut strokes = Vec::with_capacity(count.min(raw.len() / 4));
    let mut idx = 0;

    for _ in 0..count {
        // Every stroke begins with a four-byte header: two reserved bytes
        // followed by the point count.
        if idx + 4 > raw.len() {
            return Err(DigitalTouchError::InvalidStrokesLength(idx + 4, raw.len()));
        }

        let points = usize::from(u16::from_le_bytes([raw[idx + 2], raw[idx + 3]]));
        idx += 4;

        let end = idx + points * 4;
        if end > raw.len() {
            return Err(DigitalTouchError::InvalidStrokesLength(end, raw.len()));
        }

        strokes.push(
            raw[idx..end]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| {
                    (
                        u16::from_le_bytes([c[0], c[1]]),
                        u16::from_le_bytes([c[2], c[3]]),
                    )
                })
                .collect(),
        );
        idx = end;
    }

    Ok(strokes)
}

/// Pluralize a noun for a count: `pluralize(1, "tap")` → `"1 tap"`.
pub(super) fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{Color, DigitalTouchMessage, ImageBackdrop};
    use crate::message_types::digital_touch::media::MediaKind;

    use std::env::current_dir;
    use std::fs::read;

    /// Magenta, the color the tap and sketch fixtures were drawn with.
    const MAGENTA: Color = Color {
        r: 255,
        g: 0,
        b: 252,
        a: 255,
    };

    fn load(name: &str) -> Vec<u8> {
        let path = current_dir()
            .unwrap()
            .join("test_data/digital_touch_message")
            .join(name);
        read(path).unwrap()
    }

    fn parse(name: &str) -> DigitalTouchMessage {
        DigitalTouchMessage::from_payload(&load(name)).unwrap()
    }

    #[test]
    fn parses_tap() {
        let DigitalTouchMessage::Tap(tap) = parse("tap.bin") else {
            panic!("expected a tap");
        };
        assert_eq!(tap.id, "E3F4E72A-A863-43C3-8277-E17680251B06");
        assert_eq!(tap.taps.len(), 1);
        assert_eq!((tap.taps[0].x, tap.taps[0].y), (30809, 37418));
        assert_eq!(tap.taps[0].extra.delay_ms, 0);
        assert_eq!(tap.taps[0].extra.color, MAGENTA);
    }

    #[test]
    fn parses_sketch() {
        let DigitalTouchMessage::Sketch(sketch) = parse("sketch.bin") else {
            panic!("expected a sketch");
        };
        assert_eq!(sketch.id, "F7D92232-92B3-4C5A-8DC7-2704BE93890E");
        assert_eq!(sketch.strokes.len(), 1);
        assert_eq!(sketch.strokes[0].len(), 81);
        assert_eq!(
            (sketch.strokes[0][0].x, sketch.strokes[0][0].y),
            (14168, 43154)
        );
        assert_eq!(sketch.strokes[0][0].extra, MAGENTA);
    }

    #[test]
    fn parses_kiss() {
        let DigitalTouchMessage::Kiss(kiss) = parse("kiss.bin") else {
            panic!("expected a kiss");
        };
        assert_eq!(kiss.id, "24AA9029-0725-4318-B449-10C2D255AB9E");
        assert_eq!(kiss.kisses.len(), 1);
        assert_eq!((kiss.kisses[0].x, kiss.kisses[0].y), (33913, 34117));
        assert_eq!(kiss.kisses[0].extra.delay_ms, 0);
        assert_eq!(kiss.kisses[0].extra.rotation_milliradians, 294);
    }

    #[test]
    fn parses_heartbeat() {
        let DigitalTouchMessage::Heartbeat(heartbeat) = parse("heartbeat.bin") else {
            panic!("expected a heartbeat");
        };
        assert_eq!(heartbeat.id, "12864C14-0F81-4362-953C-82D1008E46EC");
        assert_eq!(heartbeat.bpm, 84.0);
        assert_eq!(heartbeat.duration, 2);
        assert_eq!(heartbeat.broken_at, None);
    }

    #[test]
    fn parses_heartbreak() {
        let DigitalTouchMessage::Heartbeat(heartbeat) = parse("heartbreak.bin") else {
            panic!("expected a heartbeat");
        };
        assert_eq!(heartbeat.id, "6BAB7D7D-2E3C-4995-9887-01AD37C3B0E2");
        assert_eq!(heartbeat.bpm, 84.0);
        assert_eq!(heartbeat.duration, 2);
        let broken_at = heartbeat.broken_at.expect("heartbreak should be broken");
        assert!((broken_at - 1.714_62).abs() < 0.001, "got {broken_at}");
    }

    #[test]
    fn parses_fireball() {
        let DigitalTouchMessage::Fireball(fireball) = parse("fireball.bin") else {
            panic!("expected a fireball");
        };
        assert_eq!(fireball.id, "0AC74C8E-BEF0-4AB5-97C9-C35DADE5EC65");
        assert_eq!(fireball.points.len(), 3);
        assert!(
            (fireball.duration - 2.079_86).abs() < 0.001,
            "got {}",
            fireball.duration
        );
        assert_eq!((fireball.points[0].x, fireball.points[0].y), (32252, 31366));
        // Delays are decoded as each point's `extra`.
        let delays: Vec<u16> = fireball.points.iter().map(|p| p.extra).collect();
        assert_eq!(delays, vec![859, 0, 83]);
    }

    #[test]
    fn parses_image() {
        let DigitalTouchMessage::Media(media) = parse("image.bin") else {
            panic!("expected media");
        };
        let MediaKind::Image { overlay } = &media.kind else {
            panic!("expected an image");
        };
        assert!(overlay.is_empty());
    }

    #[test]
    fn parses_video() {
        let DigitalTouchMessage::Media(media) = parse("video.bin") else {
            panic!("expected media");
        };
        // A video carries no overlay by construction
        assert!(matches!(media.kind, MediaKind::Video));
    }

    #[test]
    fn parses_image_with_drawing() {
        let DigitalTouchMessage::Media(media) = parse("image_with_drawing.bin") else {
            panic!("expected media");
        };
        let MediaKind::Image { overlay } = &media.kind else {
            panic!("expected an image");
        };
        // The overlay is a single nested sketch message with five strokes.
        assert_eq!(overlay.len(), 1);
        let DigitalTouchMessage::Sketch(sketch) = &overlay[0] else {
            panic!("expected a sketch overlay");
        };
        assert_eq!(sketch.strokes.len(), 5);
        let points: usize = sketch.strokes.iter().map(Vec::len).sum();
        assert_eq!(points, 97);
    }

    #[test]
    fn renders_svg_and_text() {
        let sketch = parse("sketch.bin");
        let svg = sketch.render_svg(None);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains(&MAGENTA.css()));
        assert_eq!(
            sketch.render_text(),
            "Digital Touch Sketch (1 stroke, 81 points)"
        );

        assert_eq!(
            parse("heartbreak.bin").render_text(),
            "Digital Touch Heartbreak (84 BPM, broke at 1.71s)"
        );
        assert_eq!(
            parse("image_with_drawing.bin").render_text(),
            "Digital Touch Image with drawing (5 strokes)"
        );
        assert_eq!(parse("video.bin").render_text(), "Digital Touch Video");
    }

    #[test]
    fn renders_single_point_stroke_as_dot() {
        let message = parse("hi.bin");
        let DigitalTouchMessage::Sketch(sketch) = &message else {
            panic!("expected a sketch");
        };
        // "hi": four multi-point strokes plus the single-point dot on the "i".
        let lengths: Vec<usize> = sketch.strokes.iter().map(Vec::len).collect();
        assert_eq!(lengths, vec![38, 10, 1, 17, 38]);

        // SVG won't stroke a one-vertex polyline, so that 1-point stroke renders as
        // a <circle> while the other four stay <polyline>s.
        let svg = message.render_svg(None);
        assert_eq!(svg.matches("<circle").count(), 1);
        assert_eq!(svg.matches("<polyline").count(), 4);
    }

    #[test]
    fn image_backdrop_references_media_and_drops_label() {
        let media = parse("image.bin");

        // An image backdrop is referenced via <image>, and the placeholder kind
        // label is omitted because the media itself is shown.
        let with_image = media.render_svg(Some(ImageBackdrop("attachments/0/bg.png".into())));
        assert!(with_image.contains(r#"<image href="attachments/0/bg.png""#));
        assert!(!with_image.contains(">Image</text>"));

        // Without a backdrop, nothing is referenced and the kind is labeled.
        let without_backdrop = media.render_svg(None);
        assert!(!without_backdrop.contains("<image"));
        assert!(without_backdrop.contains(">Image</text>"));
    }

    #[test]
    fn decode_strokes_rejects_oversized_count_without_allocating() {
        assert!(super::decode_strokes(&[], 1_000_000_000).is_err());
    }
}
