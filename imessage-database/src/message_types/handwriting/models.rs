/*!
[Handwritten](https://support.apple.com/en-us/HT206894) messages are animated doodles or messages sent in your own handwriting.
*/

use std::io::{Cursor, ErrorKind};
use std::{fmt::Write, io::Error};

use crate::{
    error::handwriting::HandwritingError,
    message_types::handwriting::handwriting_proto::{BaseMessage, Compression},
};

use protobuf::Message;

/// Parser for [handwritten](https://support.apple.com/en-us/HT206894) iMessages.
///
/// This message type is not documented by Apple, but represents messages displayed as
/// `com.apple.Handwriting.HandwritingProvider`.
#[derive(Debug, PartialEq, Eq)]
pub struct HandwrittenMessage {
    /// Unique identifier for the handwritten message.
    pub id: String,
    /// Timestamp for when the handwritten message was created, stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone.
    pub created_at: i64,
    /// Render height in pixels.
    pub height: u16,
    /// Render width in pixels.
    pub width: u16,
    /// Strokes that make up the handwritten image.
    pub strokes: Vec<Vec<Point>>,
}

/// Point along a handwritten line.
#[derive(Debug, PartialEq, Eq)]
pub struct Point {
    /// X-coordinate of the point.
    pub x: u16,
    /// Y-coordinate of the point.
    pub y: u16,
    /// Stroke width at this point.
    pub width: u16,
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl HandwrittenMessage {
    /// Converts a raw byte payload from the database into a [`HandwrittenMessage`].
    pub fn from_payload(payload: &[u8]) -> Result<Self, HandwritingError> {
        let msg =
            BaseMessage::parse_from_bytes(payload).map_err(HandwritingError::ProtobufError)?;
        let (width, height) = parse_dimensions(&msg)?;

        // Pad the frame dimensions by 5 to ensure strokes on the edge stay within the canvas.
        let (Some(padded_width), Some(padded_height)) =
            (width.checked_add(5), height.checked_add(5))
        else {
            return Err(HandwritingError::InvalidDimensions(width, height));
        };

        let strokes = parse_strokes(&msg)?;
        let (max_x, max_y, max_width) = get_max_dimension(&strokes);
        Ok(Self {
            id: msg.ID.clone(),
            created_at: msg.CreatedAt,
            height: padded_height,
            width: padded_width,
            strokes: fit_strokes(&strokes, height, width, max_x, max_y, max_width),
        })
    }

    /// Render the handwriting message as SVG.
    #[must_use]
    pub fn render_svg(&self) -> String {
        let mut svg = String::new();
        svg.push('\n');
        svg.push_str(format!(r#"<svg viewBox="0 0 {} {}" preserveAspectRatio="xMidYMid meet" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#, self.width, self.height).as_str());
        svg.push('\n');
        svg.push_str(&format!("<title>{}</title>\n", xml_text(&self.id)));
        svg.push_str("<metadata>\n");
        svg.push_str(&format!("<id>{}</id>\n", xml_text(&self.id)));
        svg.push_str(&format!("<createdAt>{}</createdAt>\n", self.created_at));
        svg.push_str("</metadata>\n");
        svg.push_str("<style>\n");
        svg.push_str(
            r"    .line {
        fill: none;
        stroke: black;
        stroke-linecap: round;
        stroke-linejoin: round;
    }
",
        );
        svg.push_str("</style>\n");
        generate_strokes(&mut svg, &self.strokes);
        svg.push_str("</svg>\n");
        svg
    }

    /// Render the handwriting message as ASCII with a maximum height.
    #[must_use]
    pub fn render_ascii(&self, max_height: usize) -> String {
        // Create a blank canvas filled with spaces
        let h = max_height.min(self.height as usize);
        let w = ((self.width as usize) * h)
            .checked_div(self.height as usize)
            .unwrap_or(0);

        // Return an empty string if the canvas has no height or width
        if h == 0 || w == 0 {
            return String::new();
        }
        let mut canvas = vec![vec![' '; w]; h];

        // Plot the lines on the canvas
        // Width is only used when drawing the line on an SVG
        for line in &fit_strokes(
            &self.strokes,
            w as u16,
            h as u16,
            self.height,
            self.width,
            1,
        ) {
            line.windows(2).for_each(|window| {
                draw_line(&mut canvas, &window[0], &window[1]);
            });
        }

        let mut output = String::with_capacity(h * (w + 1));
        for row in canvas {
            for &ch in &row {
                let _ = write!(output, "{ch}");
            }
            output.push('\n');
        }

        output
    }
}

/// Draws a line on a 2d character grid using Bresenham's line algorithm.
fn draw_line(canvas: &mut [Vec<char>], start: &Point, end: &Point) {
    let mut x_curr = i64::from(start.x);
    let mut y_curr = i64::from(start.y);
    let x_end = i64::from(end.x);
    let y_end = i64::from(end.y);

    let dx = (x_end - x_curr).abs();
    let dy = -(y_end - y_curr).abs();
    let sx = if x_curr < x_end { 1 } else { -1 };
    let sy = if y_curr < y_end { 1 } else { -1 };
    let mut err = dx + dy;

    while x_curr != x_end || y_curr != y_end {
        draw_point(canvas, x_curr, y_curr);
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x_curr += sx;
        }
        if e2 <= dx {
            err += dx;
            y_curr += sy;
        }
    }

    draw_point(canvas, x_end, y_end);
}

/// Draws a point on a 2d character grid.
fn draw_point(canvas: &mut [Vec<char>], x: i64, y: i64) {
    if x >= 0 && x < canvas[0].len() as i64 && y >= 0 && y < canvas.len() as i64 {
        canvas[y as usize][x as usize] = '*';
    }
}

/// Write SVG polylines for the parsed strokes.
fn generate_strokes(svg: &mut String, strokes: &[Vec<Point>]) {
    for stroke in strokes {
        if stroke.is_empty() {
            continue;
        }
        let mut segments = String::with_capacity(80 * (stroke.len() - 1));
        for (width, points) in &group_points(stroke) {
            let mut points_svg = String::with_capacity(points.len() * 3);
            for point in points {
                let _ = write!(points_svg, " {},{}", point.x, point.y);
            }
            segments.push_str(
                format!(
                    r#"<polyline class="line" points="{}" stroke-width="{}" />"#,
                    points_svg.trim_start(),
                    width
                )
                .as_str(),
            );
            segments.push('\n');
        }
        svg.push_str(segments.as_str());
    }
}

/// Group adjacent stroke points by width.
fn group_points(stroke: &[Point]) -> Vec<(u16, Vec<&Point>)> {
    let mut groups = vec![];
    if stroke.is_empty() {
        return groups;
    }
    let mut curr = stroke[0].width;
    let mut segment = vec![];

    for point in stroke {
        segment.push(point);
        if curr != point.width {
            if segment.len() == 1 {
                segment.push(point);
            }
            groups.push((curr, segment.clone()));
            segment = vec![point];
            curr = point.width;
        }
    }

    if !segment.is_empty() {
        segment.push(segment[segment.len() - 1]);
        groups.push((curr, segment));
    }
    groups
}

/// Converts all points from a canvas of `max_x` by `max_y` to a canvas of `height` and `width`.
fn fit_strokes(
    strokes: &[Vec<Point>],
    height: u16,
    width: u16,
    max_x: u16,
    max_y: u16,
    max_width: u16,
) -> Vec<Vec<Point>> {
    strokes
        .iter()
        .map(|stroke| -> Vec<Point> {
            stroke
                .iter()
                .map(|point| -> Point {
                    Point {
                        x: resize(point.x, width, max_x),
                        y: resize(point.y, height, max_y),
                        width: resize(point.width, 9, max_width) + 1,
                    }
                })
                .collect()
        })
        .collect()
}

/// Resize converts `v` from a coordinate where `max_v` is the current height/width and `box_size` is the wanted height/width.
fn resize(v: u16, box_size: u16, max_v: u16) -> u16 {
    (i64::from(v) * i64::from(box_size))
        .checked_div(i64::from(max_v))
        .unwrap_or(0) as u16
}

/// Return the maximum `x`, `y`, and stroke-width values across all strokes.
fn get_max_dimension(strokes: &[Vec<Point>]) -> (u16, u16, u16) {
    strokes.iter().flat_map(|stroke| stroke.iter()).fold(
        (0, 0, 0),
        |(max_x, max_y, max_width), point| {
            (
                max_x.max(point.x),
                max_y.max(point.y),
                max_width.max(point.width.saturating_sub(1)),
            )
        },
    )
}

/// Parse raw stroke bytes into point groups.
fn parse_strokes(msg: &BaseMessage) -> Result<Vec<Vec<Point>>, HandwritingError> {
    let data = decompress_strokes(msg)?;

    let mut strokes = vec![];
    let mut idx = 0;
    let length = data.len();
    while idx < length {
        if idx + 1 >= length {
            return Err(HandwritingError::InvalidStrokesLength(idx + 1, length));
        }

        let num_points = u16::from_le_bytes([data[idx], data[idx + 1]]) as usize;
        idx += 2;
        if num_points == 0 {
            return Err(HandwritingError::EmptyStroke);
        }
        if idx + (num_points * 8) > length {
            return Err(HandwritingError::InvalidStrokesLength(
                idx + (num_points * 8),
                length,
            ));
        }

        let mut stroke = vec![];
        (0..num_points).try_for_each(|_| -> Result<(), HandwritingError> {
            let x = parse_coordinates(data[idx], data[idx + 1]);
            let y = parse_coordinates(data[idx + 2], data[idx + 3]);
            let width = parse_coordinates(data[idx + 4], data[idx + 5]);
            idx += 8;
            stroke.push(Point { x, y, width });
            Ok(())
        })?;
        strokes.push(stroke);
    }
    Ok(strokes)
}

/// Hard ceiling on decompressed handwriting stroke data, independent of the
/// attacker-controlled `DecompressedLength`, so a crafted XZ payload cannot
/// force an unbounded allocation. Real handwriting payloads are only a few KB.
const MAX_DECOMPRESSED_STROKES: usize = 32 * 1024 * 1024;

/// A sink that appends decompressed bytes to a buffer but fails once a write
/// would exceed `limit`, so a decompression bomb is rejected mid-stream rather
/// than fully materialized in memory.
struct CappedBuffer {
    buf: Vec<u8>,
    limit: usize,
}

impl std::io::Write for CappedBuffer {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.buf.len().saturating_add(data.len()) > self.limit {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "decompressed handwriting data exceeded the expected length",
            ));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Decompress raw stroke bytes and verify the expected length.
fn decompress_strokes(msg: &BaseMessage) -> Result<Vec<u8>, HandwritingError> {
    let (data, length) = match msg.Handwriting.Compression.enum_value_or_default() {
        Compression::None => {
            let data = msg.Handwriting.Strokes.clone();
            let length = data.len();
            (data, length)
        }
        Compression::XZ => {
            // Bound the output up front so a crafted stream cannot expand without
            // limit: cap at the declared length, itself clamped to a sane maximum.
            let length = match msg.Handwriting.DecompressedLength {
                Some(decompress_size) => usize::try_from(decompress_size)
                    .map_err(|_| HandwritingError::ConversionError)?,
                None => return Err(HandwritingError::DecompressedNotSet),
            };
            let mut cursor = Cursor::new(&msg.Handwriting.Strokes);
            let mut sink = CappedBuffer {
                buf: Vec::new(),
                limit: length.min(MAX_DECOMPRESSED_STROKES),
            };
            lzma_rs::xz_decompress(&mut cursor, &mut sink).map_err(HandwritingError::XZError)?;
            (sink.buf, length)
        }
        Compression::Unknown => {
            return Err(HandwritingError::CompressionUnknown);
        }
    };

    if length != data.len() {
        return Err(HandwritingError::InvalidDecompressedLength(
            length,
            data.len(),
        ));
    }
    Ok(data)
}

/// Parse drawing dimensions from the protobuf frame.
fn parse_dimensions(msg: &BaseMessage) -> Result<(u16, u16), HandwritingError> {
    let rect = &msg.Handwriting.Frame;
    if rect.len() != 8 {
        return Err(HandwritingError::InvalidFrameSize(rect.len()));
    }
    Ok((
        parse_coordinates(rect[4], rect[5]),
        parse_coordinates(rect[6], rect[7]),
    ))
}

/// Decode a signed coordinate stored in two bytes.
fn parse_coordinates(b1: u8, b2: u8) -> u16 {
    u16::from_le_bytes([b1, b2]) ^ 0x8000
}

#[cfg(test)]
mod tests {
    use crate::message_types::handwriting::models::{HandwrittenMessage, Point};

    use super::{CappedBuffer, get_max_dimension, parse_strokes};
    use crate::error::handwriting::HandwritingError;
    use crate::message_types::handwriting::handwriting_proto::{
        BaseMessage, Compression, Handwriting,
    };
    use protobuf::{EnumOrUnknown, Message, MessageField};

    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn capped_buffer_rejects_data_beyond_limit() {
        use std::io::Write as _;

        let mut sink = CappedBuffer {
            buf: Vec::new(),
            limit: 4,
        };
        // Writing exactly up to the limit is allowed.
        assert!(sink.write_all(b"abcd").is_ok());
        // The next byte must be rejected rather than buffered, so a decompression
        // bomb cannot grow the buffer past the declared/clamped length.
        assert!(sink.write_all(b"e").is_err());
        assert_eq!(sink.buf, b"abcd");
    }

    #[test]
    fn test_parse_handwritten_from_payload() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        // The SVG golden pins every coordinate; this test localizes a decoder
        // failure to the header or to one stroke.
        assert_eq!(balloon.id, "e8fae151-5b83-4efa-98c6-b207381f004c");
        assert_eq!(balloon.created_at, 577_234_961_941);
        assert_eq!((balloon.width, balloon.height), (753, 243));
        assert_eq!(balloon.strokes.len(), 20);
        let point_counts: Vec<usize> = balloon.strokes.iter().map(Vec::len).collect();
        assert_eq!(
            point_counts,
            vec![
                79, 67, 55, 105, 725, 295, 99, 3, 3, 17, 67, 43, 55, 39, 85, 83, 87, 63, 75, 79
            ]
        );
    }

    #[test]
    fn test_parse_handwritten_as_ascii() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.ascii");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_ascii(40), expected);
    }

    #[test]
    fn test_parse_handwritten_as_ascii_half() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting_half.ascii");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_ascii(20), expected);
    }

    #[test]
    fn test_parse_handwritten_as_ascii_old() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/test.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/test.ascii");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_ascii(20), expected);
    }

    #[test]
    fn test_parse_handwritten_as_ascii_builtin() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/hello.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/hello.ascii");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_ascii(20), expected);
    }

    #[test]
    fn test_parse_handwritten_as_ascii_pollock() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/pollock.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/pollock.ascii");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_ascii(20), expected);
    }

    #[test]
    fn test_parse_handwritten_as_svg() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/handwriting.svg");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_svg(), expected);
    }

    #[test]
    fn test_parse_handwritten_as_svg_old() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/test.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/test.svg");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_svg(), expected);
    }

    #[test]
    fn test_parse_handwritten_as_svg_builtin() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/hello.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/hello.svg");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_svg(), expected);
    }

    #[test]
    fn test_parse_handwritten_as_svg_pollock() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/pollock.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();
        let balloon = HandwrittenMessage::from_payload(&data).unwrap();

        let mut expected = String::new();
        let expected_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/handwritten_message/pollock.svg");
        let mut expected_data = File::open(expected_path).unwrap();
        expected_data.read_to_string(&mut expected).unwrap();

        assert_eq!(balloon.render_svg(), expected);
    }

    #[test]
    fn from_payload_rejects_frame_near_u16_max() {
        let mut msg = BaseMessage::new();
        msg.Handwriting = MessageField::some(Handwriting {
            Frame: vec![0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0xFF, 0x7F],
            ..Default::default()
        });
        let payload = msg.write_to_bytes().unwrap();

        assert!(matches!(
            HandwrittenMessage::from_payload(&payload),
            Err(HandwritingError::InvalidDimensions(0, u16::MAX))
        ));
    }

    #[test]
    fn render_ascii_with_no_rows_is_empty() {
        let message = HandwrittenMessage {
            id: String::new(),
            created_at: 0,
            height: 5,
            width: 5,
            strokes: vec![vec![
                Point {
                    x: 0,
                    y: 0,
                    width: 1,
                },
                Point {
                    x: 4,
                    y: 4,
                    width: 1,
                },
            ]],
        };

        assert_eq!(message.render_ascii(0), "");
    }

    #[test]
    fn parse_strokes_rejects_empty_stroke() {
        let mut handwriting = Handwriting::new();
        handwriting.Compression = EnumOrUnknown::new(Compression::None);
        handwriting.Strokes = vec![0x00, 0x00];

        let mut msg = BaseMessage::new();
        msg.Handwriting = MessageField::some(handwriting);

        assert!(matches!(
            parse_strokes(&msg),
            Err(HandwritingError::EmptyStroke)
        ));
    }

    #[test]
    fn render_svg_skips_empty_strokes() {
        let message = HandwrittenMessage {
            id: "test".to_string(),
            created_at: 0,
            height: 10,
            width: 10,
            strokes: vec![
                vec![],
                vec![
                    Point {
                        x: 1,
                        y: 1,
                        width: 2,
                    },
                    Point {
                        x: 5,
                        y: 5,
                        width: 2,
                    },
                ],
                vec![],
            ],
        };

        // Only the single non-empty stroke renders; the empty ones are skipped.
        assert_eq!(message.render_svg().matches("<polyline").count(), 1);
    }

    #[test]
    fn get_max_dimension_handles_zero_width_point() {
        let strokes = vec![vec![
            Point {
                x: 5,
                y: 7,
                width: 0,
            },
            Point {
                x: 1,
                y: 1,
                width: 3,
            },
        ]];
        assert_eq!(get_max_dimension(&strokes), (5, 7, 2));

        // A lone width-0 point yields `max_width` 0 rather than wrapping to u16::MAX.
        let zero = vec![vec![Point {
            x: 2,
            y: 4,
            width: 0,
        }]];
        assert_eq!(get_max_dimension(&zero), (2, 4, 0));
    }
}
