/*!
[Handwritten](https://support.apple.com/en-us/HT206894) messages are animated doodles or messages sent in your own handwriting.
*/

use std::fmt::Write;
use std::io::Cursor;

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
    /// Unique identifier for the handwritten message
    pub id: String,
    /// Timestamp for when the handwritten message was created, stored as a unix timestamp with an epoch of `2001-01-01 00:00:00` in the local time zone
    pub created_at: i64,
    /// Height of the handwritten message in pixels
    pub height: u16,
    /// Width of the handwritten message in pixels
    pub width: u16,
    /// Collection of strokes that make up the handwritten image
    pub strokes: Vec<Vec<Point>>,
}

/// Represents a point along a handwritten line.
#[derive(Debug, PartialEq, Eq)]
pub struct Point {
    /// X-coordinate of the point
    pub x: u16,
    /// Y-coordinate of the point
    pub y: u16,
    /// Width of the stroke at this point
    pub width: u16,
}

impl HandwrittenMessage {
    /// Converts a raw byte payload from the database into a [`HandwrittenMessage`].
    pub fn from_payload(payload: &[u8]) -> Result<Self, HandwritingError> {
        let msg =
            BaseMessage::parse_from_bytes(payload).map_err(HandwritingError::ProtobufError)?;
        let (width, height) = parse_dimensions(&msg)?;
        let strokes = parse_strokes(&msg)?;
        let (max_x, max_y, max_width) = get_max_dimension(&strokes);
        Ok(Self {
            id: msg.ID.to_string(),
            created_at: msg.CreatedAt,
            height: height + 5,
            width: width + 5,
            strokes: fit_strokes(&strokes, height, width, max_x, max_y, max_width),
        })
    }

    /// Renders the handwriting message as an `svg` graphic.
    #[must_use]
    pub fn render_svg(&self) -> String {
        let mut svg = String::new();
        svg.push('\n');
        svg.push_str(format!(r#"<svg viewBox="0 0 {} {}" preserveAspectRatio="xMidYMid meet" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#, self.width, self.height).as_str());
        svg.push('\n');
        svg.push_str(&format!("<title>{}</title>\n", self.id));
        svg.push_str("<metadata>\n");
        svg.push_str(&format!("<id>{}</id>\n", self.id));
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

    /// Renders the handwriting message as an ASCII graphic with a maximum height.
    #[must_use]
    pub fn render_ascii(&self, max_height: usize) -> String {
        // Create a blank canvas filled with spaces
        let h = max_height.min(self.height as usize);
        let w = ((self.width as usize) * h)
            .checked_div(self.height as usize)
            .unwrap_or(0);
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

        // Convert the canvas to a string
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

/// Generates svg lines from an array of strokes.
fn generate_strokes(svg: &mut String, strokes: &[Vec<Point>]) {
    for stroke in strokes {
        let mut segments = String::with_capacity(80 * (stroke.len() - 1));
        for (width, points) in &group_points(stroke) {
            let mut points_svg = String::with_capacity(points.len() * 3);
            for point in points {
                points_svg.push_str(&format!(" {},{}", point.x, point.y));
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

/// Group points along a stroke together by width
fn group_points(stroke: &[Point]) -> Vec<(u16, Vec<&Point>)> {
    let mut groups = vec![];
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

/// Iterates through each point in each stroke and extracts the maximum `x`, `y`, and `width` values.
fn get_max_dimension(strokes: &[Vec<Point>]) -> (u16, u16, u16) {
    strokes.iter().flat_map(|stroke| stroke.iter()).fold(
        (0, 0, 0),
        |(max_x, max_y, max_width), point| {
            (
                max_x.max(point.x),
                max_y.max(point.y),
                max_width.max(point.width - 1),
            )
        },
    )
}

/// Parses raw stroke data into an array of strokes.
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

/// Decompresses raw stroke data and verifies length.
fn decompress_strokes(msg: &BaseMessage) -> Result<Vec<u8>, HandwritingError> {
    let data = match msg.Handwriting.Compression.enum_value_or_default() {
        Compression::None => msg.Handwriting.Strokes.clone(),
        Compression::XZ => {
            let mut cursor = Cursor::new(&msg.Handwriting.Strokes);
            let mut buf = Vec::new();
            lzma_rs::xz_decompress(&mut cursor, &mut buf).map_err(HandwritingError::XZError)?;
            buf
        }
        Compression::Unknown => {
            return Err(HandwritingError::CompressionUnknown);
        }
    };

    let length = match msg.Handwriting.Compression.enum_value_or_default() {
        Compression::None => data.len(),
        Compression::XZ => {
            if let Some(decompress_size) = msg.Handwriting.DecompressedLength {
                usize::try_from(decompress_size).map_err(|_| HandwritingError::ConversionError)?
            } else {
                return Err(HandwritingError::DecompressedNotSet);
            }
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

/// Parses the drawing size from the protobuf message.
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

/// Converts coordinate bytes to an u16.
fn parse_coordinates(b1: u8, b2: u8) -> u16 {
    u16::from_le_bytes([b1, b2]) ^ 0x8000
}

#[cfg(test)]
mod tests {
    use crate::message_types::handwriting::models::{HandwrittenMessage, Point};

    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;

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

        let expected = HandwrittenMessage {
            id: "e8fae151-5b83-4efa-98c6-b207381f004c".to_string(),
            created_at: 577234961941,
            height: 243,
            width: 753,
            strokes: vec![
                vec![
                    Point {
                        x: 49,
                        y: 8,
                        width: 10,
                    },
                    Point {
                        x: 46,
                        y: 14,
                        width: 10,
                    },
                    Point {
                        x: 45,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 45,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 45,
                        y: 19,
                        width: 9,
                    },
                    Point {
                        x: 45,
                        y: 20,
                        width: 9,
                    },
                    Point {
                        x: 45,
                        y: 22,
                        width: 9,
                    },
                    Point {
                        x: 45,
                        y: 23,
                        width: 9,
                    },
                    Point {
                        x: 44,
                        y: 25,
                        width: 8,
                    },
                    Point {
                        x: 44,
                        y: 27,
                        width: 8,
                    },
                    Point {
                        x: 44,
                        y: 28,
                        width: 8,
                    },
                    Point {
                        x: 44,
                        y: 30,
                        width: 8,
                    },
                    Point {
                        x: 44,
                        y: 33,
                        width: 7,
                    },
                    Point {
                        x: 43,
                        y: 35,
                        width: 7,
                    },
                    Point {
                        x: 43,
                        y: 38,
                        width: 6,
                    },
                    Point {
                        x: 43,
                        y: 40,
                        width: 6,
                    },
                    Point {
                        x: 43,
                        y: 43,
                        width: 6,
                    },
                    Point {
                        x: 42,
                        y: 46,
                        width: 5,
                    },
                    Point {
                        x: 42,
                        y: 50,
                        width: 5,
                    },
                    Point {
                        x: 41,
                        y: 53,
                        width: 4,
                    },
                    Point {
                        x: 41,
                        y: 56,
                        width: 4,
                    },
                    Point {
                        x: 40,
                        y: 60,
                        width: 4,
                    },
                    Point {
                        x: 40,
                        y: 63,
                        width: 4,
                    },
                    Point {
                        x: 39,
                        y: 67,
                        width: 4,
                    },
                    Point {
                        x: 38,
                        y: 71,
                        width: 4,
                    },
                    Point {
                        x: 37,
                        y: 75,
                        width: 3,
                    },
                    Point {
                        x: 36,
                        y: 78,
                        width: 3,
                    },
                    Point {
                        x: 35,
                        y: 82,
                        width: 3,
                    },
                    Point {
                        x: 34,
                        y: 87,
                        width: 3,
                    },
                    Point {
                        x: 34,
                        y: 91,
                        width: 3,
                    },
                    Point {
                        x: 33,
                        y: 95,
                        width: 3,
                    },
                    Point {
                        x: 32,
                        y: 99,
                        width: 3,
                    },
                    Point {
                        x: 31,
                        y: 103,
                        width: 3,
                    },
                    Point {
                        x: 30,
                        y: 107,
                        width: 3,
                    },
                    Point {
                        x: 29,
                        y: 111,
                        width: 3,
                    },
                    Point {
                        x: 28,
                        y: 115,
                        width: 3,
                    },
                    Point {
                        x: 28,
                        y: 119,
                        width: 3,
                    },
                    Point {
                        x: 27,
                        y: 123,
                        width: 3,
                    },
                    Point {
                        x: 26,
                        y: 127,
                        width: 3,
                    },
                    Point {
                        x: 25,
                        y: 130,
                        width: 4,
                    },
                    Point {
                        x: 25,
                        y: 133,
                        width: 4,
                    },
                    Point {
                        x: 24,
                        y: 137,
                        width: 4,
                    },
                    Point {
                        x: 24,
                        y: 140,
                        width: 4,
                    },
                    Point {
                        x: 23,
                        y: 143,
                        width: 5,
                    },
                    Point {
                        x: 23,
                        y: 146,
                        width: 5,
                    },
                    Point {
                        x: 22,
                        y: 149,
                        width: 5,
                    },
                    Point {
                        x: 22,
                        y: 151,
                        width: 6,
                    },
                    Point {
                        x: 22,
                        y: 154,
                        width: 6,
                    },
                    Point {
                        x: 21,
                        y: 156,
                        width: 6,
                    },
                    Point {
                        x: 21,
                        y: 159,
                        width: 6,
                    },
                    Point {
                        x: 21,
                        y: 161,
                        width: 7,
                    },
                    Point {
                        x: 20,
                        y: 163,
                        width: 7,
                    },
                    Point {
                        x: 20,
                        y: 164,
                        width: 7,
                    },
                    Point {
                        x: 20,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 20,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 19,
                        y: 169,
                        width: 8,
                    },
                    Point {
                        x: 19,
                        y: 171,
                        width: 8,
                    },
                    Point {
                        x: 19,
                        y: 172,
                        width: 8,
                    },
                    Point {
                        x: 18,
                        y: 174,
                        width: 8,
                    },
                    Point {
                        x: 18,
                        y: 175,
                        width: 8,
                    },
                    Point {
                        x: 18,
                        y: 177,
                        width: 8,
                    },
                    Point {
                        x: 17,
                        y: 178,
                        width: 8,
                    },
                    Point {
                        x: 17,
                        y: 179,
                        width: 8,
                    },
                    Point {
                        x: 16,
                        y: 180,
                        width: 9,
                    },
                    Point {
                        x: 16,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 15,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 15,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 14,
                        y: 184,
                        width: 9,
                    },
                    Point {
                        x: 14,
                        y: 185,
                        width: 9,
                    },
                    Point {
                        x: 13,
                        y: 186,
                        width: 9,
                    },
                    Point {
                        x: 13,
                        y: 187,
                        width: 9,
                    },
                    Point {
                        x: 12,
                        y: 188,
                        width: 10,
                    },
                    Point {
                        x: 11,
                        y: 188,
                        width: 10,
                    },
                    Point {
                        x: 11,
                        y: 189,
                        width: 10,
                    },
                    Point {
                        x: 10,
                        y: 190,
                        width: 10,
                    },
                    Point {
                        x: 9,
                        y: 191,
                        width: 10,
                    },
                    Point {
                        x: 9,
                        y: 191,
                        width: 9,
                    },
                    Point {
                        x: 8,
                        y: 192,
                        width: 9,
                    },
                    Point {
                        x: 9,
                        y: 191,
                        width: 9,
                    },
                ],
                vec![
                    Point {
                        x: 110,
                        y: 8,
                        width: 10,
                    },
                    Point {
                        x: 110,
                        y: 13,
                        width: 10,
                    },
                    Point {
                        x: 110,
                        y: 15,
                        width: 9,
                    },
                    Point {
                        x: 110,
                        y: 17,
                        width: 9,
                    },
                    Point {
                        x: 110,
                        y: 19,
                        width: 9,
                    },
                    Point {
                        x: 110,
                        y: 20,
                        width: 8,
                    },
                    Point {
                        x: 110,
                        y: 22,
                        width: 8,
                    },
                    Point {
                        x: 110,
                        y: 24,
                        width: 8,
                    },
                    Point {
                        x: 110,
                        y: 27,
                        width: 7,
                    },
                    Point {
                        x: 110,
                        y: 29,
                        width: 6,
                    },
                    Point {
                        x: 110,
                        y: 32,
                        width: 6,
                    },
                    Point {
                        x: 110,
                        y: 35,
                        width: 6,
                    },
                    Point {
                        x: 110,
                        y: 38,
                        width: 5,
                    },
                    Point {
                        x: 110,
                        y: 41,
                        width: 4,
                    },
                    Point {
                        x: 110,
                        y: 45,
                        width: 4,
                    },
                    Point {
                        x: 110,
                        y: 49,
                        width: 4,
                    },
                    Point {
                        x: 110,
                        y: 53,
                        width: 4,
                    },
                    Point {
                        x: 110,
                        y: 57,
                        width: 3,
                    },
                    Point {
                        x: 110,
                        y: 60,
                        width: 3,
                    },
                    Point {
                        x: 110,
                        y: 64,
                        width: 3,
                    },
                    Point {
                        x: 109,
                        y: 68,
                        width: 3,
                    },
                    Point {
                        x: 109,
                        y: 72,
                        width: 3,
                    },
                    Point {
                        x: 109,
                        y: 77,
                        width: 3,
                    },
                    Point {
                        x: 108,
                        y: 81,
                        width: 3,
                    },
                    Point {
                        x: 107,
                        y: 84,
                        width: 3,
                    },
                    Point {
                        x: 107,
                        y: 88,
                        width: 3,
                    },
                    Point {
                        x: 106,
                        y: 92,
                        width: 3,
                    },
                    Point {
                        x: 105,
                        y: 96,
                        width: 3,
                    },
                    Point {
                        x: 105,
                        y: 100,
                        width: 3,
                    },
                    Point {
                        x: 104,
                        y: 104,
                        width: 3,
                    },
                    Point {
                        x: 103,
                        y: 108,
                        width: 3,
                    },
                    Point {
                        x: 102,
                        y: 111,
                        width: 3,
                    },
                    Point {
                        x: 102,
                        y: 115,
                        width: 3,
                    },
                    Point {
                        x: 101,
                        y: 119,
                        width: 3,
                    },
                    Point {
                        x: 101,
                        y: 123,
                        width: 3,
                    },
                    Point {
                        x: 100,
                        y: 127,
                        width: 4,
                    },
                    Point {
                        x: 100,
                        y: 130,
                        width: 4,
                    },
                    Point {
                        x: 99,
                        y: 134,
                        width: 4,
                    },
                    Point {
                        x: 99,
                        y: 137,
                        width: 4,
                    },
                    Point {
                        x: 98,
                        y: 141,
                        width: 4,
                    },
                    Point {
                        x: 98,
                        y: 144,
                        width: 4,
                    },
                    Point {
                        x: 97,
                        y: 147,
                        width: 4,
                    },
                    Point {
                        x: 97,
                        y: 150,
                        width: 5,
                    },
                    Point {
                        x: 97,
                        y: 153,
                        width: 5,
                    },
                    Point {
                        x: 96,
                        y: 156,
                        width: 5,
                    },
                    Point {
                        x: 96,
                        y: 158,
                        width: 6,
                    },
                    Point {
                        x: 96,
                        y: 161,
                        width: 6,
                    },
                    Point {
                        x: 96,
                        y: 163,
                        width: 6,
                    },
                    Point {
                        x: 95,
                        y: 165,
                        width: 7,
                    },
                    Point {
                        x: 95,
                        y: 167,
                        width: 8,
                    },
                    Point {
                        x: 95,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 95,
                        y: 170,
                        width: 8,
                    },
                    Point {
                        x: 94,
                        y: 171,
                        width: 8,
                    },
                    Point {
                        x: 94,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 94,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 94,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 94,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 94,
                        y: 175,
                        width: 9,
                    },
                    Point {
                        x: 93,
                        y: 176,
                        width: 10,
                    },
                    Point {
                        x: 93,
                        y: 176,
                        width: 10,
                    },
                    Point {
                        x: 93,
                        y: 177,
                        width: 10,
                    },
                    Point {
                        x: 93,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 93,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 92,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 92,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 91,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 92,
                        y: 178,
                        width: 10,
                    },
                ],
                vec![
                    Point {
                        x: 20,
                        y: 129,
                        width: 10,
                    },
                    Point {
                        x: 24,
                        y: 129,
                        width: 8,
                    },
                    Point {
                        x: 25,
                        y: 128,
                        width: 8,
                    },
                    Point {
                        x: 27,
                        y: 127,
                        width: 8,
                    },
                    Point {
                        x: 29,
                        y: 126,
                        width: 7,
                    },
                    Point {
                        x: 31,
                        y: 125,
                        width: 7,
                    },
                    Point {
                        x: 33,
                        y: 124,
                        width: 6,
                    },
                    Point {
                        x: 36,
                        y: 122,
                        width: 6,
                    },
                    Point {
                        x: 38,
                        y: 121,
                        width: 6,
                    },
                    Point {
                        x: 40,
                        y: 120,
                        width: 6,
                    },
                    Point {
                        x: 43,
                        y: 118,
                        width: 6,
                    },
                    Point {
                        x: 45,
                        y: 117,
                        width: 5,
                    },
                    Point {
                        x: 48,
                        y: 115,
                        width: 5,
                    },
                    Point {
                        x: 50,
                        y: 114,
                        width: 5,
                    },
                    Point {
                        x: 53,
                        y: 112,
                        width: 5,
                    },
                    Point {
                        x: 55,
                        y: 111,
                        width: 5,
                    },
                    Point {
                        x: 58,
                        y: 109,
                        width: 5,
                    },
                    Point {
                        x: 61,
                        y: 107,
                        width: 5,
                    },
                    Point {
                        x: 63,
                        y: 106,
                        width: 5,
                    },
                    Point {
                        x: 66,
                        y: 104,
                        width: 5,
                    },
                    Point {
                        x: 69,
                        y: 102,
                        width: 4,
                    },
                    Point {
                        x: 72,
                        y: 101,
                        width: 4,
                    },
                    Point {
                        x: 74,
                        y: 99,
                        width: 4,
                    },
                    Point {
                        x: 77,
                        y: 97,
                        width: 4,
                    },
                    Point {
                        x: 80,
                        y: 96,
                        width: 4,
                    },
                    Point {
                        x: 83,
                        y: 94,
                        width: 4,
                    },
                    Point {
                        x: 86,
                        y: 93,
                        width: 4,
                    },
                    Point {
                        x: 88,
                        y: 91,
                        width: 4,
                    },
                    Point {
                        x: 91,
                        y: 90,
                        width: 5,
                    },
                    Point {
                        x: 94,
                        y: 88,
                        width: 5,
                    },
                    Point {
                        x: 97,
                        y: 87,
                        width: 5,
                    },
                    Point {
                        x: 99,
                        y: 85,
                        width: 5,
                    },
                    Point {
                        x: 102,
                        y: 84,
                        width: 5,
                    },
                    Point {
                        x: 104,
                        y: 83,
                        width: 5,
                    },
                    Point {
                        x: 107,
                        y: 81,
                        width: 6,
                    },
                    Point {
                        x: 109,
                        y: 80,
                        width: 6,
                    },
                    Point {
                        x: 111,
                        y: 79,
                        width: 6,
                    },
                    Point {
                        x: 113,
                        y: 77,
                        width: 6,
                    },
                    Point {
                        x: 115,
                        y: 76,
                        width: 6,
                    },
                    Point {
                        x: 117,
                        y: 75,
                        width: 6,
                    },
                    Point {
                        x: 119,
                        y: 73,
                        width: 7,
                    },
                    Point {
                        x: 121,
                        y: 72,
                        width: 7,
                    },
                    Point {
                        x: 122,
                        y: 71,
                        width: 7,
                    },
                    Point {
                        x: 124,
                        y: 70,
                        width: 8,
                    },
                    Point {
                        x: 125,
                        y: 69,
                        width: 8,
                    },
                    Point {
                        x: 126,
                        y: 68,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 67,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 66,
                        width: 8,
                    },
                    Point {
                        x: 129,
                        y: 65,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 65,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 64,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 63,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 62,
                        width: 10,
                    },
                    Point {
                        x: 131,
                        y: 61,
                        width: 10,
                    },
                    Point {
                        x: 131,
                        y: 62,
                        width: 10,
                    },
                ],
                vec![
                    Point {
                        x: 127,
                        y: 156,
                        width: 10,
                    },
                    Point {
                        x: 129,
                        y: 156,
                        width: 10,
                    },
                    Point {
                        x: 129,
                        y: 156,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 132,
                        y: 154,
                        width: 9,
                    },
                    Point {
                        x: 132,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 133,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 134,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 134,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 135,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 136,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 136,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 137,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 137,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 138,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 138,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 139,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 139,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 139,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 139,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 139,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 138,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 137,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 135,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 134,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 133,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 132,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 132,
                        width: 10,
                    },
                    Point {
                        x: 140,
                        y: 131,
                        width: 10,
                    },
                    Point {
                        x: 139,
                        y: 131,
                        width: 10,
                    },
                    Point {
                        x: 139,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 138,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 137,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 135,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 134,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 133,
                        y: 130,
                        width: 10,
                    },
                    Point {
                        x: 132,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 132,
                        y: 132,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 133,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 134,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 135,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 146,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 147,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 150,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 151,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 153,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 154,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 157,
                        width: 8,
                    },
                    Point {
                        x: 127,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 160,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 128,
                        y: 162,
                        width: 9,
                    },
                    Point {
                        x: 128,
                        y: 164,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 165,
                        width: 9,
                    },
                    Point {
                        x: 129,
                        y: 166,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 166,
                        width: 9,
                    },
                    Point {
                        x: 130,
                        y: 167,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 131,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 132,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 133,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 134,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 135,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 136,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 138,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 139,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 140,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 141,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 142,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 143,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 144,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 145,
                        y: 164,
                        width: 8,
                    },
                    Point {
                        x: 146,
                        y: 163,
                        width: 8,
                    },
                    Point {
                        x: 147,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 148,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 149,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 150,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 151,
                        y: 157,
                        width: 8,
                    },
                    Point {
                        x: 152,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 153,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 154,
                        y: 155,
                        width: 8,
                    },
                    Point {
                        x: 155,
                        y: 154,
                        width: 8,
                    },
                    Point {
                        x: 156,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 157,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 158,
                        y: 151,
                        width: 8,
                    },
                    Point {
                        x: 159,
                        y: 150,
                        width: 8,
                    },
                    Point {
                        x: 160,
                        y: 149,
                        width: 8,
                    },
                    Point {
                        x: 161,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 161,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 161,
                        y: 148,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 178,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 179,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 180,
                        y: 165,
                        width: 10,
                    },
                    Point {
                        x: 181,
                        y: 164,
                        width: 10,
                    },
                    Point {
                        x: 181,
                        y: 164,
                        width: 10,
                    },
                    Point {
                        x: 182,
                        y: 163,
                        width: 9,
                    },
                    Point {
                        x: 183,
                        y: 162,
                        width: 9,
                    },
                    Point {
                        x: 183,
                        y: 162,
                        width: 9,
                    },
                    Point {
                        x: 184,
                        y: 161,
                        width: 9,
                    },
                    Point {
                        x: 184,
                        y: 160,
                        width: 9,
                    },
                    Point {
                        x: 185,
                        y: 160,
                        width: 9,
                    },
                    Point {
                        x: 186,
                        y: 159,
                        width: 9,
                    },
                    Point {
                        x: 187,
                        y: 159,
                        width: 9,
                    },
                    Point {
                        x: 187,
                        y: 158,
                        width: 9,
                    },
                    Point {
                        x: 188,
                        y: 157,
                        width: 9,
                    },
                    Point {
                        x: 189,
                        y: 157,
                        width: 9,
                    },
                    Point {
                        x: 189,
                        y: 156,
                        width: 9,
                    },
                    Point {
                        x: 190,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 191,
                        y: 154,
                        width: 9,
                    },
                    Point {
                        x: 191,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 192,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 193,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 194,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 194,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 195,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 196,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 196,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 197,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 198,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 199,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 199,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 200,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 201,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 201,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 202,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 203,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 204,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 204,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 205,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 206,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 206,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 207,
                        y: 135,
                        width: 9,
                    },
                    Point {
                        x: 208,
                        y: 134,
                        width: 9,
                    },
                    Point {
                        x: 208,
                        y: 133,
                        width: 9,
                    },
                    Point {
                        x: 209,
                        y: 132,
                        width: 9,
                    },
                    Point {
                        x: 210,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 210,
                        y: 130,
                        width: 9,
                    },
                    Point {
                        x: 211,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 212,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 212,
                        y: 128,
                        width: 9,
                    },
                    Point {
                        x: 213,
                        y: 127,
                        width: 9,
                    },
                    Point {
                        x: 214,
                        y: 126,
                        width: 9,
                    },
                    Point {
                        x: 214,
                        y: 125,
                        width: 9,
                    },
                    Point {
                        x: 215,
                        y: 125,
                        width: 9,
                    },
                    Point {
                        x: 216,
                        y: 124,
                        width: 9,
                    },
                    Point {
                        x: 216,
                        y: 123,
                        width: 9,
                    },
                    Point {
                        x: 217,
                        y: 122,
                        width: 9,
                    },
                    Point {
                        x: 217,
                        y: 122,
                        width: 9,
                    },
                    Point {
                        x: 218,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 218,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 219,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 219,
                        y: 118,
                        width: 9,
                    },
                    Point {
                        x: 220,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 220,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 221,
                        y: 116,
                        width: 9,
                    },
                    Point {
                        x: 221,
                        y: 115,
                        width: 9,
                    },
                    Point {
                        x: 222,
                        y: 114,
                        width: 9,
                    },
                    Point {
                        x: 222,
                        y: 113,
                        width: 9,
                    },
                    Point {
                        x: 223,
                        y: 112,
                        width: 9,
                    },
                    Point {
                        x: 224,
                        y: 111,
                        width: 9,
                    },
                    Point {
                        x: 224,
                        y: 110,
                        width: 9,
                    },
                    Point {
                        x: 225,
                        y: 109,
                        width: 9,
                    },
                    Point {
                        x: 225,
                        y: 108,
                        width: 9,
                    },
                    Point {
                        x: 226,
                        y: 107,
                        width: 9,
                    },
                    Point {
                        x: 226,
                        y: 106,
                        width: 9,
                    },
                    Point {
                        x: 227,
                        y: 105,
                        width: 9,
                    },
                    Point {
                        x: 227,
                        y: 104,
                        width: 9,
                    },
                    Point {
                        x: 228,
                        y: 103,
                        width: 9,
                    },
                    Point {
                        x: 228,
                        y: 102,
                        width: 9,
                    },
                    Point {
                        x: 229,
                        y: 101,
                        width: 9,
                    },
                    Point {
                        x: 229,
                        y: 100,
                        width: 9,
                    },
                    Point {
                        x: 230,
                        y: 99,
                        width: 9,
                    },
                    Point {
                        x: 230,
                        y: 98,
                        width: 9,
                    },
                    Point {
                        x: 230,
                        y: 97,
                        width: 9,
                    },
                    Point {
                        x: 231,
                        y: 96,
                        width: 9,
                    },
                    Point {
                        x: 231,
                        y: 95,
                        width: 9,
                    },
                    Point {
                        x: 232,
                        y: 93,
                        width: 9,
                    },
                    Point {
                        x: 232,
                        y: 92,
                        width: 9,
                    },
                    Point {
                        x: 233,
                        y: 91,
                        width: 9,
                    },
                    Point {
                        x: 233,
                        y: 90,
                        width: 9,
                    },
                    Point {
                        x: 233,
                        y: 88,
                        width: 8,
                    },
                    Point {
                        x: 234,
                        y: 87,
                        width: 8,
                    },
                    Point {
                        x: 234,
                        y: 86,
                        width: 8,
                    },
                    Point {
                        x: 234,
                        y: 84,
                        width: 8,
                    },
                    Point {
                        x: 235,
                        y: 83,
                        width: 8,
                    },
                    Point {
                        x: 235,
                        y: 81,
                        width: 8,
                    },
                    Point {
                        x: 236,
                        y: 80,
                        width: 8,
                    },
                    Point {
                        x: 236,
                        y: 78,
                        width: 8,
                    },
                    Point {
                        x: 236,
                        y: 77,
                        width: 8,
                    },
                    Point {
                        x: 237,
                        y: 75,
                        width: 8,
                    },
                    Point {
                        x: 237,
                        y: 74,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 72,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 71,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 69,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 68,
                        width: 8,
                    },
                    Point {
                        x: 239,
                        y: 66,
                        width: 8,
                    },
                    Point {
                        x: 239,
                        y: 64,
                        width: 8,
                    },
                    Point {
                        x: 239,
                        y: 63,
                        width: 8,
                    },
                    Point {
                        x: 240,
                        y: 61,
                        width: 8,
                    },
                    Point {
                        x: 240,
                        y: 59,
                        width: 8,
                    },
                    Point {
                        x: 240,
                        y: 58,
                        width: 8,
                    },
                    Point {
                        x: 240,
                        y: 56,
                        width: 8,
                    },
                    Point {
                        x: 241,
                        y: 54,
                        width: 8,
                    },
                    Point {
                        x: 241,
                        y: 53,
                        width: 8,
                    },
                    Point {
                        x: 241,
                        y: 51,
                        width: 8,
                    },
                    Point {
                        x: 241,
                        y: 49,
                        width: 8,
                    },
                    Point {
                        x: 241,
                        y: 48,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 47,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 45,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 44,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 42,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 41,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 40,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 38,
                        width: 9,
                    },
                    Point {
                        x: 243,
                        y: 37,
                        width: 9,
                    },
                    Point {
                        x: 243,
                        y: 36,
                        width: 9,
                    },
                    Point {
                        x: 243,
                        y: 35,
                        width: 9,
                    },
                    Point {
                        x: 243,
                        y: 34,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 33,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 32,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 31,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 30,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 29,
                        width: 9,
                    },
                    Point {
                        x: 244,
                        y: 29,
                        width: 9,
                    },
                    Point {
                        x: 245,
                        y: 28,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 27,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 26,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 25,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 24,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 23,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 22,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 21,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 21,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 20,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 19,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 17,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 244,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 244,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 243,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 243,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 241,
                        y: 17,
                        width: 9,
                    },
                    Point {
                        x: 241,
                        y: 18,
                        width: 9,
                    },
                    Point {
                        x: 240,
                        y: 19,
                        width: 9,
                    },
                    Point {
                        x: 240,
                        y: 20,
                        width: 9,
                    },
                    Point {
                        x: 239,
                        y: 22,
                        width: 9,
                    },
                    Point {
                        x: 239,
                        y: 23,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 25,
                        width: 8,
                    },
                    Point {
                        x: 238,
                        y: 26,
                        width: 8,
                    },
                    Point {
                        x: 237,
                        y: 28,
                        width: 8,
                    },
                    Point {
                        x: 236,
                        y: 29,
                        width: 8,
                    },
                    Point {
                        x: 236,
                        y: 31,
                        width: 8,
                    },
                    Point {
                        x: 235,
                        y: 33,
                        width: 8,
                    },
                    Point {
                        x: 234,
                        y: 35,
                        width: 7,
                    },
                    Point {
                        x: 233,
                        y: 37,
                        width: 7,
                    },
                    Point {
                        x: 232,
                        y: 39,
                        width: 7,
                    },
                    Point {
                        x: 231,
                        y: 41,
                        width: 7,
                    },
                    Point {
                        x: 231,
                        y: 43,
                        width: 7,
                    },
                    Point {
                        x: 230,
                        y: 45,
                        width: 7,
                    },
                    Point {
                        x: 229,
                        y: 47,
                        width: 7,
                    },
                    Point {
                        x: 228,
                        y: 49,
                        width: 7,
                    },
                    Point {
                        x: 227,
                        y: 51,
                        width: 7,
                    },
                    Point {
                        x: 226,
                        y: 53,
                        width: 6,
                    },
                    Point {
                        x: 225,
                        y: 55,
                        width: 6,
                    },
                    Point {
                        x: 224,
                        y: 58,
                        width: 6,
                    },
                    Point {
                        x: 223,
                        y: 60,
                        width: 6,
                    },
                    Point {
                        x: 223,
                        y: 62,
                        width: 6,
                    },
                    Point {
                        x: 222,
                        y: 65,
                        width: 6,
                    },
                    Point {
                        x: 221,
                        y: 67,
                        width: 6,
                    },
                    Point {
                        x: 220,
                        y: 70,
                        width: 6,
                    },
                    Point {
                        x: 219,
                        y: 72,
                        width: 6,
                    },
                    Point {
                        x: 219,
                        y: 75,
                        width: 6,
                    },
                    Point {
                        x: 218,
                        y: 78,
                        width: 6,
                    },
                    Point {
                        x: 217,
                        y: 81,
                        width: 6,
                    },
                    Point {
                        x: 216,
                        y: 83,
                        width: 5,
                    },
                    Point {
                        x: 215,
                        y: 86,
                        width: 5,
                    },
                    Point {
                        x: 215,
                        y: 89,
                        width: 5,
                    },
                    Point {
                        x: 214,
                        y: 93,
                        width: 5,
                    },
                    Point {
                        x: 213,
                        y: 96,
                        width: 4,
                    },
                    Point {
                        x: 212,
                        y: 100,
                        width: 4,
                    },
                    Point {
                        x: 211,
                        y: 103,
                        width: 4,
                    },
                    Point {
                        x: 210,
                        y: 107,
                        width: 4,
                    },
                    Point {
                        x: 209,
                        y: 111,
                        width: 3,
                    },
                    Point {
                        x: 209,
                        y: 115,
                        width: 3,
                    },
                    Point {
                        x: 208,
                        y: 119,
                        width: 3,
                    },
                    Point {
                        x: 207,
                        y: 123,
                        width: 3,
                    },
                    Point {
                        x: 206,
                        y: 127,
                        width: 3,
                    },
                    Point {
                        x: 205,
                        y: 131,
                        width: 3,
                    },
                    Point {
                        x: 204,
                        y: 135,
                        width: 3,
                    },
                    Point {
                        x: 204,
                        y: 139,
                        width: 3,
                    },
                    Point {
                        x: 203,
                        y: 143,
                        width: 3,
                    },
                    Point {
                        x: 202,
                        y: 147,
                        width: 3,
                    },
                    Point {
                        x: 201,
                        y: 151,
                        width: 3,
                    },
                    Point {
                        x: 201,
                        y: 154,
                        width: 4,
                    },
                    Point {
                        x: 200,
                        y: 158,
                        width: 4,
                    },
                    Point {
                        x: 200,
                        y: 161,
                        width: 4,
                    },
                    Point {
                        x: 199,
                        y: 164,
                        width: 4,
                    },
                    Point {
                        x: 199,
                        y: 167,
                        width: 5,
                    },
                    Point {
                        x: 198,
                        y: 170,
                        width: 5,
                    },
                    Point {
                        x: 198,
                        y: 173,
                        width: 6,
                    },
                    Point {
                        x: 198,
                        y: 175,
                        width: 6,
                    },
                    Point {
                        x: 198,
                        y: 177,
                        width: 7,
                    },
                    Point {
                        x: 198,
                        y: 179,
                        width: 8,
                    },
                    Point {
                        x: 198,
                        y: 180,
                        width: 8,
                    },
                    Point {
                        x: 198,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 198,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 199,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 199,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 200,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 201,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 202,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 203,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 204,
                        y: 183,
                        width: 10,
                    },
                    Point {
                        x: 205,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 206,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 207,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 208,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 208,
                        y: 180,
                        width: 9,
                    },
                    Point {
                        x: 209,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 210,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 211,
                        y: 177,
                        width: 9,
                    },
                    Point {
                        x: 212,
                        y: 176,
                        width: 8,
                    },
                    Point {
                        x: 213,
                        y: 175,
                        width: 8,
                    },
                    Point {
                        x: 214,
                        y: 173,
                        width: 8,
                    },
                    Point {
                        x: 215,
                        y: 172,
                        width: 8,
                    },
                    Point {
                        x: 216,
                        y: 170,
                        width: 8,
                    },
                    Point {
                        x: 218,
                        y: 169,
                        width: 7,
                    },
                    Point {
                        x: 219,
                        y: 167,
                        width: 7,
                    },
                    Point {
                        x: 220,
                        y: 165,
                        width: 7,
                    },
                    Point {
                        x: 222,
                        y: 163,
                        width: 6,
                    },
                    Point {
                        x: 223,
                        y: 161,
                        width: 6,
                    },
                    Point {
                        x: 225,
                        y: 158,
                        width: 6,
                    },
                    Point {
                        x: 226,
                        y: 156,
                        width: 6,
                    },
                    Point {
                        x: 228,
                        y: 154,
                        width: 6,
                    },
                    Point {
                        x: 229,
                        y: 152,
                        width: 6,
                    },
                    Point {
                        x: 231,
                        y: 150,
                        width: 6,
                    },
                    Point {
                        x: 232,
                        y: 147,
                        width: 6,
                    },
                    Point {
                        x: 234,
                        y: 145,
                        width: 6,
                    },
                    Point {
                        x: 235,
                        y: 143,
                        width: 6,
                    },
                    Point {
                        x: 237,
                        y: 141,
                        width: 6,
                    },
                    Point {
                        x: 238,
                        y: 138,
                        width: 6,
                    },
                    Point {
                        x: 240,
                        y: 136,
                        width: 6,
                    },
                    Point {
                        x: 241,
                        y: 133,
                        width: 6,
                    },
                    Point {
                        x: 243,
                        y: 131,
                        width: 6,
                    },
                    Point {
                        x: 244,
                        y: 129,
                        width: 6,
                    },
                    Point {
                        x: 246,
                        y: 126,
                        width: 6,
                    },
                    Point {
                        x: 247,
                        y: 124,
                        width: 5,
                    },
                    Point {
                        x: 249,
                        y: 121,
                        width: 5,
                    },
                    Point {
                        x: 250,
                        y: 119,
                        width: 5,
                    },
                    Point {
                        x: 252,
                        y: 116,
                        width: 5,
                    },
                    Point {
                        x: 253,
                        y: 114,
                        width: 5,
                    },
                    Point {
                        x: 254,
                        y: 111,
                        width: 5,
                    },
                    Point {
                        x: 255,
                        y: 108,
                        width: 5,
                    },
                    Point {
                        x: 256,
                        y: 106,
                        width: 5,
                    },
                    Point {
                        x: 258,
                        y: 103,
                        width: 5,
                    },
                    Point {
                        x: 259,
                        y: 100,
                        width: 5,
                    },
                    Point {
                        x: 260,
                        y: 97,
                        width: 5,
                    },
                    Point {
                        x: 261,
                        y: 94,
                        width: 5,
                    },
                    Point {
                        x: 262,
                        y: 91,
                        width: 5,
                    },
                    Point {
                        x: 263,
                        y: 88,
                        width: 4,
                    },
                    Point {
                        x: 264,
                        y: 85,
                        width: 4,
                    },
                    Point {
                        x: 265,
                        y: 81,
                        width: 4,
                    },
                    Point {
                        x: 266,
                        y: 78,
                        width: 4,
                    },
                    Point {
                        x: 266,
                        y: 74,
                        width: 4,
                    },
                    Point {
                        x: 267,
                        y: 71,
                        width: 4,
                    },
                    Point {
                        x: 268,
                        y: 67,
                        width: 4,
                    },
                    Point {
                        x: 269,
                        y: 64,
                        width: 4,
                    },
                    Point {
                        x: 270,
                        y: 61,
                        width: 4,
                    },
                    Point {
                        x: 270,
                        y: 57,
                        width: 4,
                    },
                    Point {
                        x: 271,
                        y: 54,
                        width: 4,
                    },
                    Point {
                        x: 272,
                        y: 51,
                        width: 5,
                    },
                    Point {
                        x: 272,
                        y: 48,
                        width: 5,
                    },
                    Point {
                        x: 273,
                        y: 45,
                        width: 5,
                    },
                    Point {
                        x: 273,
                        y: 43,
                        width: 6,
                    },
                    Point {
                        x: 274,
                        y: 40,
                        width: 6,
                    },
                    Point {
                        x: 274,
                        y: 38,
                        width: 6,
                    },
                    Point {
                        x: 274,
                        y: 35,
                        width: 6,
                    },
                    Point {
                        x: 274,
                        y: 33,
                        width: 7,
                    },
                    Point {
                        x: 275,
                        y: 31,
                        width: 7,
                    },
                    Point {
                        x: 275,
                        y: 30,
                        width: 8,
                    },
                    Point {
                        x: 275,
                        y: 28,
                        width: 8,
                    },
                    Point {
                        x: 275,
                        y: 27,
                        width: 8,
                    },
                    Point {
                        x: 275,
                        y: 25,
                        width: 8,
                    },
                    Point {
                        x: 275,
                        y: 25,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 24,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 23,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 22,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 21,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 20,
                        width: 10,
                    },
                    Point {
                        x: 275,
                        y: 19,
                        width: 10,
                    },
                    Point {
                        x: 275,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 275,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 274,
                        y: 17,
                        width: 10,
                    },
                    Point {
                        x: 274,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 273,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 273,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 272,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 271,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 271,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 270,
                        y: 15,
                        width: 10,
                    },
                    Point {
                        x: 269,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 268,
                        y: 16,
                        width: 10,
                    },
                    Point {
                        x: 268,
                        y: 17,
                        width: 10,
                    },
                    Point {
                        x: 267,
                        y: 17,
                        width: 10,
                    },
                    Point {
                        x: 267,
                        y: 18,
                        width: 10,
                    },
                    Point {
                        x: 266,
                        y: 19,
                        width: 9,
                    },
                    Point {
                        x: 266,
                        y: 20,
                        width: 9,
                    },
                    Point {
                        x: 266,
                        y: 21,
                        width: 9,
                    },
                    Point {
                        x: 265,
                        y: 22,
                        width: 9,
                    },
                    Point {
                        x: 265,
                        y: 23,
                        width: 9,
                    },
                    Point {
                        x: 264,
                        y: 25,
                        width: 9,
                    },
                    Point {
                        x: 264,
                        y: 26,
                        width: 8,
                    },
                    Point {
                        x: 263,
                        y: 27,
                        width: 8,
                    },
                    Point {
                        x: 263,
                        y: 29,
                        width: 8,
                    },
                    Point {
                        x: 262,
                        y: 31,
                        width: 8,
                    },
                    Point {
                        x: 262,
                        y: 32,
                        width: 8,
                    },
                    Point {
                        x: 261,
                        y: 34,
                        width: 8,
                    },
                    Point {
                        x: 261,
                        y: 36,
                        width: 8,
                    },
                    Point {
                        x: 260,
                        y: 38,
                        width: 7,
                    },
                    Point {
                        x: 260,
                        y: 40,
                        width: 7,
                    },
                    Point {
                        x: 259,
                        y: 42,
                        width: 7,
                    },
                    Point {
                        x: 258,
                        y: 45,
                        width: 7,
                    },
                    Point {
                        x: 258,
                        y: 47,
                        width: 7,
                    },
                    Point {
                        x: 257,
                        y: 49,
                        width: 6,
                    },
                    Point {
                        x: 257,
                        y: 52,
                        width: 6,
                    },
                    Point {
                        x: 256,
                        y: 54,
                        width: 6,
                    },
                    Point {
                        x: 256,
                        y: 56,
                        width: 6,
                    },
                    Point {
                        x: 255,
                        y: 59,
                        width: 6,
                    },
                    Point {
                        x: 254,
                        y: 61,
                        width: 6,
                    },
                    Point {
                        x: 254,
                        y: 64,
                        width: 6,
                    },
                    Point {
                        x: 253,
                        y: 66,
                        width: 6,
                    },
                    Point {
                        x: 253,
                        y: 69,
                        width: 6,
                    },
                    Point {
                        x: 252,
                        y: 71,
                        width: 6,
                    },
                    Point {
                        x: 251,
                        y: 73,
                        width: 6,
                    },
                    Point {
                        x: 251,
                        y: 76,
                        width: 6,
                    },
                    Point {
                        x: 250,
                        y: 78,
                        width: 7,
                    },
                    Point {
                        x: 250,
                        y: 80,
                        width: 7,
                    },
                    Point {
                        x: 249,
                        y: 82,
                        width: 7,
                    },
                    Point {
                        x: 249,
                        y: 84,
                        width: 7,
                    },
                    Point {
                        x: 248,
                        y: 86,
                        width: 7,
                    },
                    Point {
                        x: 248,
                        y: 88,
                        width: 7,
                    },
                    Point {
                        x: 247,
                        y: 90,
                        width: 8,
                    },
                    Point {
                        x: 247,
                        y: 92,
                        width: 8,
                    },
                    Point {
                        x: 246,
                        y: 93,
                        width: 8,
                    },
                    Point {
                        x: 246,
                        y: 95,
                        width: 8,
                    },
                    Point {
                        x: 245,
                        y: 96,
                        width: 8,
                    },
                    Point {
                        x: 245,
                        y: 98,
                        width: 8,
                    },
                    Point {
                        x: 245,
                        y: 99,
                        width: 8,
                    },
                    Point {
                        x: 244,
                        y: 101,
                        width: 8,
                    },
                    Point {
                        x: 244,
                        y: 102,
                        width: 8,
                    },
                    Point {
                        x: 244,
                        y: 104,
                        width: 8,
                    },
                    Point {
                        x: 244,
                        y: 105,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 107,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 108,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 110,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 111,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 113,
                        width: 8,
                    },
                    Point {
                        x: 243,
                        y: 115,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 116,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 120,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 122,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 123,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 125,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 127,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 128,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 130,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 131,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 133,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 134,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 136,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 137,
                        width: 8,
                    },
                    Point {
                        x: 242,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 242,
                        y: 149,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 150,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 151,
                        width: 10,
                    },
                    Point {
                        x: 242,
                        y: 152,
                        width: 10,
                    },
                    Point {
                        x: 243,
                        y: 153,
                        width: 10,
                    },
                    Point {
                        x: 243,
                        y: 154,
                        width: 10,
                    },
                    Point {
                        x: 243,
                        y: 155,
                        width: 10,
                    },
                    Point {
                        x: 244,
                        y: 156,
                        width: 10,
                    },
                    Point {
                        x: 244,
                        y: 157,
                        width: 10,
                    },
                    Point {
                        x: 244,
                        y: 158,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 158,
                        width: 10,
                    },
                    Point {
                        x: 245,
                        y: 159,
                        width: 10,
                    },
                    Point {
                        x: 246,
                        y: 160,
                        width: 10,
                    },
                    Point {
                        x: 246,
                        y: 161,
                        width: 10,
                    },
                    Point {
                        x: 247,
                        y: 161,
                        width: 10,
                    },
                    Point {
                        x: 247,
                        y: 162,
                        width: 10,
                    },
                    Point {
                        x: 248,
                        y: 163,
                        width: 10,
                    },
                    Point {
                        x: 249,
                        y: 163,
                        width: 10,
                    },
                    Point {
                        x: 249,
                        y: 164,
                        width: 10,
                    },
                    Point {
                        x: 249,
                        y: 164,
                        width: 10,
                    },
                    Point {
                        x: 250,
                        y: 165,
                        width: 10,
                    },
                    Point {
                        x: 250,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 251,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 251,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 252,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 253,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 254,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 254,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 255,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 256,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 257,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 258,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 259,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 260,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 261,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 261,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 262,
                        y: 165,
                        width: 10,
                    },
                    Point {
                        x: 263,
                        y: 164,
                        width: 10,
                    },
                    Point {
                        x: 264,
                        y: 163,
                        width: 10,
                    },
                    Point {
                        x: 265,
                        y: 162,
                        width: 10,
                    },
                    Point {
                        x: 265,
                        y: 162,
                        width: 10,
                    },
                    Point {
                        x: 266,
                        y: 161,
                        width: 10,
                    },
                    Point {
                        x: 267,
                        y: 161,
                        width: 10,
                    },
                    Point {
                        x: 267,
                        y: 160,
                        width: 10,
                    },
                    Point {
                        x: 268,
                        y: 159,
                        width: 9,
                    },
                    Point {
                        x: 268,
                        y: 159,
                        width: 9,
                    },
                    Point {
                        x: 269,
                        y: 158,
                        width: 9,
                    },
                    Point {
                        x: 270,
                        y: 158,
                        width: 9,
                    },
                    Point {
                        x: 270,
                        y: 157,
                        width: 9,
                    },
                    Point {
                        x: 271,
                        y: 156,
                        width: 9,
                    },
                    Point {
                        x: 271,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 272,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 273,
                        y: 154,
                        width: 9,
                    },
                    Point {
                        x: 273,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 278,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 278,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 279,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 280,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 280,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 281,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 282,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 282,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 283,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 283,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 284,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 284,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 285,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 285,
                        y: 135,
                        width: 9,
                    },
                    Point {
                        x: 286,
                        y: 134,
                        width: 9,
                    },
                    Point {
                        x: 287,
                        y: 133,
                        width: 9,
                    },
                    Point {
                        x: 287,
                        y: 133,
                        width: 9,
                    },
                    Point {
                        x: 288,
                        y: 132,
                        width: 9,
                    },
                    Point {
                        x: 288,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 289,
                        y: 130,
                        width: 9,
                    },
                    Point {
                        x: 290,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 291,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 291,
                        y: 128,
                        width: 9,
                    },
                    Point {
                        x: 292,
                        y: 127,
                        width: 9,
                    },
                    Point {
                        x: 293,
                        y: 127,
                        width: 9,
                    },
                    Point {
                        x: 294,
                        y: 126,
                        width: 9,
                    },
                    Point {
                        x: 294,
                        y: 126,
                        width: 9,
                    },
                    Point {
                        x: 295,
                        y: 125,
                        width: 9,
                    },
                    Point {
                        x: 296,
                        y: 124,
                        width: 9,
                    },
                    Point {
                        x: 297,
                        y: 124,
                        width: 9,
                    },
                    Point {
                        x: 297,
                        y: 123,
                        width: 9,
                    },
                    Point {
                        x: 298,
                        y: 123,
                        width: 9,
                    },
                    Point {
                        x: 299,
                        y: 122,
                        width: 9,
                    },
                    Point {
                        x: 300,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 301,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 302,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 303,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 304,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 305,
                        y: 118,
                        width: 9,
                    },
                    Point {
                        x: 305,
                        y: 118,
                        width: 9,
                    },
                    Point {
                        x: 306,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 307,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 307,
                        y: 116,
                        width: 9,
                    },
                    Point {
                        x: 308,
                        y: 116,
                        width: 9,
                    },
                    Point {
                        x: 309,
                        y: 115,
                        width: 9,
                    },
                    Point {
                        x: 309,
                        y: 115,
                        width: 9,
                    },
                    Point {
                        x: 310,
                        y: 114,
                        width: 10,
                    },
                    Point {
                        x: 310,
                        y: 114,
                        width: 10,
                    },
                    Point {
                        x: 310,
                        y: 113,
                        width: 10,
                    },
                    Point {
                        x: 310,
                        y: 113,
                        width: 10,
                    },
                    Point {
                        x: 310,
                        y: 113,
                        width: 10,
                    },
                    Point {
                        x: 309,
                        y: 113,
                        width: 10,
                    },
                    Point {
                        x: 308,
                        y: 113,
                        width: 9,
                    },
                    Point {
                        x: 307,
                        y: 113,
                        width: 9,
                    },
                    Point {
                        x: 307,
                        y: 114,
                        width: 9,
                    },
                    Point {
                        x: 306,
                        y: 115,
                        width: 9,
                    },
                    Point {
                        x: 305,
                        y: 115,
                        width: 9,
                    },
                    Point {
                        x: 304,
                        y: 116,
                        width: 9,
                    },
                    Point {
                        x: 303,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 302,
                        y: 118,
                        width: 9,
                    },
                    Point {
                        x: 301,
                        y: 119,
                        width: 8,
                    },
                    Point {
                        x: 300,
                        y: 120,
                        width: 8,
                    },
                    Point {
                        x: 299,
                        y: 121,
                        width: 8,
                    },
                    Point {
                        x: 298,
                        y: 122,
                        width: 8,
                    },
                    Point {
                        x: 296,
                        y: 123,
                        width: 8,
                    },
                    Point {
                        x: 295,
                        y: 124,
                        width: 8,
                    },
                    Point {
                        x: 294,
                        y: 125,
                        width: 8,
                    },
                    Point {
                        x: 293,
                        y: 126,
                        width: 8,
                    },
                    Point {
                        x: 292,
                        y: 127,
                        width: 8,
                    },
                    Point {
                        x: 291,
                        y: 128,
                        width: 8,
                    },
                    Point {
                        x: 290,
                        y: 129,
                        width: 8,
                    },
                    Point {
                        x: 289,
                        y: 130,
                        width: 8,
                    },
                    Point {
                        x: 288,
                        y: 131,
                        width: 8,
                    },
                    Point {
                        x: 287,
                        y: 132,
                        width: 8,
                    },
                    Point {
                        x: 286,
                        y: 133,
                        width: 8,
                    },
                    Point {
                        x: 285,
                        y: 134,
                        width: 8,
                    },
                    Point {
                        x: 284,
                        y: 135,
                        width: 8,
                    },
                    Point {
                        x: 283,
                        y: 137,
                        width: 8,
                    },
                    Point {
                        x: 283,
                        y: 138,
                        width: 8,
                    },
                    Point {
                        x: 282,
                        y: 139,
                        width: 8,
                    },
                    Point {
                        x: 281,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 280,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 279,
                        y: 142,
                        width: 8,
                    },
                    Point {
                        x: 279,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 278,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 154,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 156,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 157,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 158,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 159,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 160,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 161,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 162,
                        width: 9,
                    },
                    Point {
                        x: 274,
                        y: 163,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 164,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 165,
                        width: 9,
                    },
                    Point {
                        x: 275,
                        y: 166,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 167,
                        width: 9,
                    },
                    Point {
                        x: 276,
                        y: 167,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 277,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 278,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 278,
                        y: 171,
                        width: 9,
                    },
                    Point {
                        x: 279,
                        y: 171,
                        width: 9,
                    },
                    Point {
                        x: 279,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 279,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 280,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 280,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 281,
                        y: 175,
                        width: 9,
                    },
                    Point {
                        x: 282,
                        y: 176,
                        width: 9,
                    },
                    Point {
                        x: 283,
                        y: 176,
                        width: 10,
                    },
                    Point {
                        x: 284,
                        y: 177,
                        width: 10,
                    },
                    Point {
                        x: 284,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 285,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 286,
                        y: 179,
                        width: 10,
                    },
                    Point {
                        x: 287,
                        y: 179,
                        width: 10,
                    },
                    Point {
                        x: 288,
                        y: 179,
                        width: 10,
                    },
                    Point {
                        x: 289,
                        y: 179,
                        width: 10,
                    },
                    Point {
                        x: 290,
                        y: 179,
                        width: 10,
                    },
                    Point {
                        x: 291,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 292,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 293,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 294,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 295,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 295,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 296,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 297,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 298,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 299,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 300,
                        y: 177,
                        width: 9,
                    },
                    Point {
                        x: 301,
                        y: 176,
                        width: 9,
                    },
                    Point {
                        x: 302,
                        y: 176,
                        width: 9,
                    },
                    Point {
                        x: 303,
                        y: 175,
                        width: 9,
                    },
                    Point {
                        x: 304,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 305,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 306,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 307,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 308,
                        y: 171,
                        width: 9,
                    },
                    Point {
                        x: 309,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 309,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 310,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 311,
                        y: 167,
                        width: 9,
                    },
                    Point {
                        x: 312,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 313,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 314,
                        y: 164,
                        width: 8,
                    },
                    Point {
                        x: 315,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 316,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 317,
                        y: 160,
                        width: 8,
                    },
                    Point {
                        x: 317,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 318,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 319,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 320,
                        y: 155,
                        width: 8,
                    },
                    Point {
                        x: 321,
                        y: 154,
                        width: 8,
                    },
                    Point {
                        x: 321,
                        y: 153,
                        width: 8,
                    },
                    Point {
                        x: 322,
                        y: 151,
                        width: 8,
                    },
                    Point {
                        x: 323,
                        y: 150,
                        width: 8,
                    },
                    Point {
                        x: 323,
                        y: 149,
                        width: 8,
                    },
                    Point {
                        x: 324,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 325,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 325,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 326,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 326,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 326,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 136,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 135,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 134,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 133,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 132,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 130,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 128,
                        width: 9,
                    },
                    Point {
                        x: 328,
                        y: 128,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 127,
                        width: 9,
                    },
                    Point {
                        x: 327,
                        y: 126,
                        width: 9,
                    },
                    Point {
                        x: 326,
                        y: 126,
                        width: 9,
                    },
                    Point {
                        x: 326,
                        y: 125,
                        width: 9,
                    },
                    Point {
                        x: 325,
                        y: 124,
                        width: 9,
                    },
                    Point {
                        x: 324,
                        y: 124,
                        width: 9,
                    },
                    Point {
                        x: 324,
                        y: 123,
                        width: 9,
                    },
                    Point {
                        x: 323,
                        y: 123,
                        width: 9,
                    },
                    Point {
                        x: 322,
                        y: 122,
                        width: 9,
                    },
                    Point {
                        x: 321,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 321,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 320,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 319,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 319,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 318,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 317,
                        y: 118,
                        width: 10,
                    },
                    Point {
                        x: 316,
                        y: 118,
                        width: 10,
                    },
                    Point {
                        x: 315,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 314,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 313,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 313,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 312,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 312,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 312,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 312,
                        y: 117,
                        width: 10,
                    },
                    Point {
                        x: 313,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 314,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 315,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 317,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 318,
                        y: 117,
                        width: 9,
                    },
                    Point {
                        x: 319,
                        y: 118,
                        width: 9,
                    },
                    Point {
                        x: 321,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 322,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 323,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 325,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 326,
                        y: 118,
                        width: 8,
                    },
                    Point {
                        x: 328,
                        y: 119,
                        width: 8,
                    },
                    Point {
                        x: 329,
                        y: 119,
                        width: 8,
                    },
                    Point {
                        x: 330,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 331,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 333,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 334,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 335,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 336,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 337,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 338,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 339,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 339,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 340,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 341,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 342,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 343,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 344,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 345,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 346,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 347,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 348,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 349,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 350,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 351,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 352,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 353,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 353,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 354,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 355,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 356,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 357,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 358,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 359,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 360,
                        y: 119,
                        width: 9,
                    },
                    Point {
                        x: 361,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 362,
                        y: 120,
                        width: 9,
                    },
                    Point {
                        x: 363,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 363,
                        y: 121,
                        width: 9,
                    },
                    Point {
                        x: 363,
                        y: 121,
                        width: 9,
                    },
                ],
                vec![
                    Point {
                        x: 404,
                        y: 17,
                        width: 10,
                    },
                    Point {
                        x: 403,
                        y: 19,
                        width: 10,
                    },
                    Point {
                        x: 403,
                        y: 20,
                        width: 9,
                    },
                    Point {
                        x: 403,
                        y: 22,
                        width: 9,
                    },
                    Point {
                        x: 403,
                        y: 23,
                        width: 9,
                    },
                    Point {
                        x: 403,
                        y: 24,
                        width: 9,
                    },
                    Point {
                        x: 403,
                        y: 26,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 27,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 29,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 31,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 33,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 34,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 36,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 38,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 40,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 42,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 43,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 45,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 47,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 49,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 50,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 52,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 54,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 56,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 57,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 59,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 61,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 63,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 65,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 67,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 69,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 70,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 72,
                        width: 8,
                    },
                    Point {
                        x: 403,
                        y: 74,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 77,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 79,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 81,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 83,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 86,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 88,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 91,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 93,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 96,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 99,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 102,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 105,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 108,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 111,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 114,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 117,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 120,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 123,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 126,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 129,
                        width: 5,
                    },
                    Point {
                        x: 403,
                        y: 131,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 134,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 137,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 139,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 142,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 145,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 147,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 149,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 152,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 154,
                        width: 6,
                    },
                    Point {
                        x: 403,
                        y: 156,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 159,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 161,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 163,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 165,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 168,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 170,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 172,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 174,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 177,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 179,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 181,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 183,
                        width: 7,
                    },
                    Point {
                        x: 403,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 404,
                        y: 186,
                        width: 8,
                    },
                    Point {
                        x: 404,
                        y: 188,
                        width: 8,
                    },
                    Point {
                        x: 405,
                        y: 189,
                        width: 8,
                    },
                    Point {
                        x: 405,
                        y: 191,
                        width: 8,
                    },
                    Point {
                        x: 406,
                        y: 192,
                        width: 8,
                    },
                    Point {
                        x: 407,
                        y: 193,
                        width: 8,
                    },
                    Point {
                        x: 407,
                        y: 194,
                        width: 9,
                    },
                    Point {
                        x: 408,
                        y: 194,
                        width: 9,
                    },
                    Point {
                        x: 409,
                        y: 195,
                        width: 9,
                    },
                    Point {
                        x: 410,
                        y: 195,
                        width: 9,
                    },
                    Point {
                        x: 410,
                        y: 196,
                        width: 9,
                    },
                    Point {
                        x: 411,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 412,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 413,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 413,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 414,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 415,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 416,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 416,
                        y: 196,
                        width: 10,
                    },
                    Point {
                        x: 417,
                        y: 195,
                        width: 10,
                    },
                    Point {
                        x: 418,
                        y: 194,
                        width: 10,
                    },
                    Point {
                        x: 418,
                        y: 194,
                        width: 10,
                    },
                    Point {
                        x: 419,
                        y: 193,
                        width: 10,
                    },
                    Point {
                        x: 419,
                        y: 192,
                        width: 10,
                    },
                    Point {
                        x: 419,
                        y: 191,
                        width: 10,
                    },
                    Point {
                        x: 420,
                        y: 190,
                        width: 10,
                    },
                    Point {
                        x: 420,
                        y: 189,
                        width: 9,
                    },
                    Point {
                        x: 420,
                        y: 188,
                        width: 9,
                    },
                    Point {
                        x: 421,
                        y: 188,
                        width: 9,
                    },
                    Point {
                        x: 421,
                        y: 187,
                        width: 9,
                    },
                    Point {
                        x: 421,
                        y: 186,
                        width: 9,
                    },
                    Point {
                        x: 422,
                        y: 185,
                        width: 9,
                    },
                    Point {
                        x: 422,
                        y: 184,
                        width: 9,
                    },
                    Point {
                        x: 422,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 423,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 423,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 424,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 424,
                        y: 180,
                        width: 9,
                    },
                    Point {
                        x: 424,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 425,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 425,
                        y: 177,
                        width: 9,
                    },
                    Point {
                        x: 425,
                        y: 175,
                        width: 9,
                    },
                    Point {
                        x: 425,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 426,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 426,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 426,
                        y: 170,
                        width: 9,
                    },
                    Point {
                        x: 426,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 427,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 427,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 427,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 427,
                        y: 163,
                        width: 8,
                    },
                    Point {
                        x: 427,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 427,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 428,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 428,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 428,
                        y: 157,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 155,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 154,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 153,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 152,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 144,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 143,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 140,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 137,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 136,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 135,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 134,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 133,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 133,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 133,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 133,
                        width: 10,
                    },
                    Point {
                        x: 428,
                        y: 134,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 135,
                        width: 9,
                    },
                    Point {
                        x: 428,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 429,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 429,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 429,
                        y: 142,
                        width: 8,
                    },
                    Point {
                        x: 429,
                        y: 143,
                        width: 8,
                    },
                    Point {
                        x: 429,
                        y: 145,
                        width: 8,
                    },
                    Point {
                        x: 430,
                        y: 147,
                        width: 8,
                    },
                    Point {
                        x: 430,
                        y: 149,
                        width: 7,
                    },
                    Point {
                        x: 430,
                        y: 151,
                        width: 7,
                    },
                    Point {
                        x: 430,
                        y: 153,
                        width: 7,
                    },
                    Point {
                        x: 431,
                        y: 155,
                        width: 7,
                    },
                    Point {
                        x: 431,
                        y: 157,
                        width: 7,
                    },
                    Point {
                        x: 431,
                        y: 159,
                        width: 7,
                    },
                    Point {
                        x: 432,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 432,
                        y: 163,
                        width: 8,
                    },
                    Point {
                        x: 432,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 433,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 433,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 433,
                        y: 169,
                        width: 8,
                    },
                    Point {
                        x: 434,
                        y: 170,
                        width: 8,
                    },
                    Point {
                        x: 434,
                        y: 172,
                        width: 8,
                    },
                    Point {
                        x: 435,
                        y: 173,
                        width: 8,
                    },
                    Point {
                        x: 435,
                        y: 174,
                        width: 8,
                    },
                    Point {
                        x: 435,
                        y: 176,
                        width: 9,
                    },
                    Point {
                        x: 435,
                        y: 177,
                        width: 9,
                    },
                    Point {
                        x: 436,
                        y: 178,
                        width: 9,
                    },
                    Point {
                        x: 436,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 436,
                        y: 180,
                        width: 9,
                    },
                    Point {
                        x: 437,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 437,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 437,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 438,
                        y: 184,
                        width: 10,
                    },
                    Point {
                        x: 438,
                        y: 184,
                        width: 10,
                    },
                    Point {
                        x: 439,
                        y: 185,
                        width: 10,
                    },
                    Point {
                        x: 440,
                        y: 185,
                        width: 10,
                    },
                    Point {
                        x: 441,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 442,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 443,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 444,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 445,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 446,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 447,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 448,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 449,
                        y: 186,
                        width: 10,
                    },
                    Point {
                        x: 450,
                        y: 185,
                        width: 10,
                    },
                    Point {
                        x: 451,
                        y: 185,
                        width: 10,
                    },
                    Point {
                        x: 452,
                        y: 184,
                        width: 10,
                    },
                    Point {
                        x: 452,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 453,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 453,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 454,
                        y: 180,
                        width: 9,
                    },
                    Point {
                        x: 454,
                        y: 179,
                        width: 9,
                    },
                    Point {
                        x: 455,
                        y: 177,
                        width: 8,
                    },
                    Point {
                        x: 455,
                        y: 176,
                        width: 8,
                    },
                    Point {
                        x: 455,
                        y: 174,
                        width: 8,
                    },
                    Point {
                        x: 456,
                        y: 173,
                        width: 8,
                    },
                    Point {
                        x: 456,
                        y: 171,
                        width: 8,
                    },
                    Point {
                        x: 456,
                        y: 169,
                        width: 8,
                    },
                    Point {
                        x: 457,
                        y: 167,
                        width: 8,
                    },
                    Point {
                        x: 457,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 457,
                        y: 164,
                        width: 8,
                    },
                    Point {
                        x: 457,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 458,
                        y: 160,
                        width: 8,
                    },
                    Point {
                        x: 458,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 458,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 458,
                        y: 154,
                        width: 8,
                    },
                    Point {
                        x: 459,
                        y: 152,
                        width: 8,
                    },
                    Point {
                        x: 459,
                        y: 150,
                        width: 8,
                    },
                    Point {
                        x: 459,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 459,
                        y: 146,
                        width: 7,
                    },
                    Point {
                        x: 459,
                        y: 144,
                        width: 7,
                    },
                    Point {
                        x: 459,
                        y: 142,
                        width: 7,
                    },
                    Point {
                        x: 460,
                        y: 140,
                        width: 7,
                    },
                    Point {
                        x: 460,
                        y: 137,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 135,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 132,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 129,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 127,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 124,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 121,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 118,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 114,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 111,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 108,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 105,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 102,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 98,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 95,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 92,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 89,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 86,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 83,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 80,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 77,
                        width: 5,
                    },
                    Point {
                        x: 460,
                        y: 75,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 72,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 69,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 67,
                        width: 6,
                    },
                    Point {
                        x: 460,
                        y: 64,
                        width: 6,
                    },
                    Point {
                        x: 461,
                        y: 62,
                        width: 6,
                    },
                    Point {
                        x: 461,
                        y: 59,
                        width: 6,
                    },
                    Point {
                        x: 461,
                        y: 57,
                        width: 6,
                    },
                    Point {
                        x: 461,
                        y: 55,
                        width: 7,
                    },
                    Point {
                        x: 461,
                        y: 53,
                        width: 7,
                    },
                    Point {
                        x: 461,
                        y: 50,
                        width: 7,
                    },
                    Point {
                        x: 461,
                        y: 48,
                        width: 7,
                    },
                    Point {
                        x: 461,
                        y: 46,
                        width: 7,
                    },
                    Point {
                        x: 462,
                        y: 44,
                        width: 8,
                    },
                    Point {
                        x: 462,
                        y: 43,
                        width: 8,
                    },
                    Point {
                        x: 462,
                        y: 41,
                        width: 8,
                    },
                    Point {
                        x: 462,
                        y: 40,
                        width: 8,
                    },
                    Point {
                        x: 462,
                        y: 38,
                        width: 8,
                    },
                    Point {
                        x: 462,
                        y: 37,
                        width: 9,
                    },
                    Point {
                        x: 463,
                        y: 36,
                        width: 9,
                    },
                    Point {
                        x: 463,
                        y: 35,
                        width: 9,
                    },
                    Point {
                        x: 463,
                        y: 34,
                        width: 9,
                    },
                    Point {
                        x: 463,
                        y: 33,
                        width: 9,
                    },
                    Point {
                        x: 463,
                        y: 32,
                        width: 9,
                    },
                    Point {
                        x: 464,
                        y: 31,
                        width: 9,
                    },
                    Point {
                        x: 464,
                        y: 31,
                        width: 10,
                    },
                    Point {
                        x: 464,
                        y: 30,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 29,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 29,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 28,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 27,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 27,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 26,
                        width: 10,
                    },
                    Point {
                        x: 465,
                        y: 25,
                        width: 10,
                    },
                    Point {
                        x: 466,
                        y: 24,
                        width: 10,
                    },
                    Point {
                        x: 466,
                        y: 24,
                        width: 10,
                    },
                    Point {
                        x: 466,
                        y: 23,
                        width: 10,
                    },
                    Point {
                        x: 466,
                        y: 24,
                        width: 10,
                    },
                ],
                vec![
                    Point {
                        x: 502,
                        y: 129,
                        width: 10,
                    },
                    Point {
                        x: 499,
                        y: 129,
                        width: 9,
                    },
                    Point {
                        x: 498,
                        y: 130,
                        width: 9,
                    },
                    Point {
                        x: 497,
                        y: 130,
                        width: 9,
                    },
                    Point {
                        x: 496,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 495,
                        y: 131,
                        width: 9,
                    },
                    Point {
                        x: 494,
                        y: 133,
                        width: 8,
                    },
                    Point {
                        x: 493,
                        y: 134,
                        width: 8,
                    },
                    Point {
                        x: 492,
                        y: 135,
                        width: 8,
                    },
                    Point {
                        x: 491,
                        y: 136,
                        width: 8,
                    },
                    Point {
                        x: 490,
                        y: 138,
                        width: 8,
                    },
                    Point {
                        x: 489,
                        y: 139,
                        width: 8,
                    },
                    Point {
                        x: 489,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 488,
                        y: 142,
                        width: 8,
                    },
                    Point {
                        x: 488,
                        y: 144,
                        width: 8,
                    },
                    Point {
                        x: 487,
                        y: 146,
                        width: 8,
                    },
                    Point {
                        x: 487,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 487,
                        y: 150,
                        width: 7,
                    },
                    Point {
                        x: 487,
                        y: 152,
                        width: 7,
                    },
                    Point {
                        x: 487,
                        y: 154,
                        width: 7,
                    },
                    Point {
                        x: 487,
                        y: 157,
                        width: 7,
                    },
                    Point {
                        x: 487,
                        y: 159,
                        width: 7,
                    },
                    Point {
                        x: 487,
                        y: 162,
                        width: 6,
                    },
                    Point {
                        x: 487,
                        y: 164,
                        width: 6,
                    },
                    Point {
                        x: 487,
                        y: 166,
                        width: 6,
                    },
                    Point {
                        x: 488,
                        y: 168,
                        width: 6,
                    },
                    Point {
                        x: 489,
                        y: 170,
                        width: 6,
                    },
                    Point {
                        x: 490,
                        y: 172,
                        width: 6,
                    },
                    Point {
                        x: 492,
                        y: 174,
                        width: 6,
                    },
                    Point {
                        x: 494,
                        y: 176,
                        width: 6,
                    },
                    Point {
                        x: 495,
                        y: 178,
                        width: 6,
                    },
                    Point {
                        x: 497,
                        y: 179,
                        width: 6,
                    },
                    Point {
                        x: 499,
                        y: 180,
                        width: 6,
                    },
                    Point {
                        x: 502,
                        y: 181,
                        width: 6,
                    },
                    Point {
                        x: 504,
                        y: 182,
                        width: 6,
                    },
                    Point {
                        x: 507,
                        y: 183,
                        width: 6,
                    },
                    Point {
                        x: 509,
                        y: 183,
                        width: 6,
                    },
                    Point {
                        x: 512,
                        y: 184,
                        width: 6,
                    },
                    Point {
                        x: 515,
                        y: 184,
                        width: 6,
                    },
                    Point {
                        x: 518,
                        y: 184,
                        width: 5,
                    },
                    Point {
                        x: 520,
                        y: 183,
                        width: 5,
                    },
                    Point {
                        x: 523,
                        y: 183,
                        width: 6,
                    },
                    Point {
                        x: 526,
                        y: 182,
                        width: 5,
                    },
                    Point {
                        x: 529,
                        y: 181,
                        width: 5,
                    },
                    Point {
                        x: 531,
                        y: 179,
                        width: 5,
                    },
                    Point {
                        x: 533,
                        y: 176,
                        width: 4,
                    },
                    Point {
                        x: 535,
                        y: 174,
                        width: 4,
                    },
                    Point {
                        x: 537,
                        y: 171,
                        width: 4,
                    },
                    Point {
                        x: 539,
                        y: 168,
                        width: 4,
                    },
                    Point {
                        x: 540,
                        y: 165,
                        width: 4,
                    },
                    Point {
                        x: 542,
                        y: 162,
                        width: 4,
                    },
                    Point {
                        x: 543,
                        y: 159,
                        width: 4,
                    },
                    Point {
                        x: 544,
                        y: 156,
                        width: 5,
                    },
                    Point {
                        x: 544,
                        y: 153,
                        width: 5,
                    },
                    Point {
                        x: 545,
                        y: 150,
                        width: 5,
                    },
                    Point {
                        x: 545,
                        y: 147,
                        width: 6,
                    },
                    Point {
                        x: 545,
                        y: 145,
                        width: 6,
                    },
                    Point {
                        x: 546,
                        y: 143,
                        width: 6,
                    },
                    Point {
                        x: 546,
                        y: 141,
                        width: 7,
                    },
                    Point {
                        x: 546,
                        y: 139,
                        width: 7,
                    },
                    Point {
                        x: 546,
                        y: 137,
                        width: 7,
                    },
                    Point {
                        x: 546,
                        y: 135,
                        width: 8,
                    },
                    Point {
                        x: 546,
                        y: 133,
                        width: 8,
                    },
                    Point {
                        x: 545,
                        y: 131,
                        width: 8,
                    },
                    Point {
                        x: 545,
                        y: 130,
                        width: 8,
                    },
                    Point {
                        x: 545,
                        y: 128,
                        width: 8,
                    },
                    Point {
                        x: 544,
                        y: 127,
                        width: 8,
                    },
                    Point {
                        x: 543,
                        y: 125,
                        width: 8,
                    },
                    Point {
                        x: 542,
                        y: 124,
                        width: 8,
                    },
                    Point {
                        x: 541,
                        y: 123,
                        width: 8,
                    },
                    Point {
                        x: 540,
                        y: 122,
                        width: 8,
                    },
                    Point {
                        x: 539,
                        y: 120,
                        width: 8,
                    },
                    Point {
                        x: 537,
                        y: 120,
                        width: 8,
                    },
                    Point {
                        x: 536,
                        y: 119,
                        width: 8,
                    },
                    Point {
                        x: 534,
                        y: 119,
                        width: 8,
                    },
                    Point {
                        x: 532,
                        y: 119,
                        width: 7,
                    },
                    Point {
                        x: 529,
                        y: 118,
                        width: 7,
                    },
                    Point {
                        x: 527,
                        y: 118,
                        width: 6,
                    },
                    Point {
                        x: 524,
                        y: 119,
                        width: 6,
                    },
                    Point {
                        x: 521,
                        y: 120,
                        width: 5,
                    },
                    Point {
                        x: 518,
                        y: 121,
                        width: 5,
                    },
                    Point {
                        x: 516,
                        y: 122,
                        width: 4,
                    },
                    Point {
                        x: 513,
                        y: 124,
                        width: 4,
                    },
                    Point {
                        x: 510,
                        y: 126,
                        width: 4,
                    },
                    Point {
                        x: 507,
                        y: 128,
                        width: 4,
                    },
                    Point {
                        x: 504,
                        y: 130,
                        width: 4,
                    },
                    Point {
                        x: 502,
                        y: 132,
                        width: 4,
                    },
                    Point {
                        x: 500,
                        y: 134,
                        width: 5,
                    },
                    Point {
                        x: 498,
                        y: 135,
                        width: 6,
                    },
                    Point {
                        x: 496,
                        y: 137,
                        width: 6,
                    },
                    Point {
                        x: 494,
                        y: 138,
                        width: 6,
                    },
                    Point {
                        x: 492,
                        y: 139,
                        width: 7,
                    },
                    Point {
                        x: 491,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 489,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 488,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 487,
                        y: 142,
                        width: 8,
                    },
                    Point {
                        x: 486,
                        y: 143,
                        width: 8,
                    },
                    Point {
                        x: 485,
                        y: 144,
                        width: 8,
                    },
                    Point {
                        x: 486,
                        y: 143,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 510,
                        y: 140,
                        width: 10,
                    },
                    Point {
                        x: 510,
                        y: 140,
                        width: 10,
                    },
                    Point {
                        x: 510,
                        y: 140,
                        width: 10,
                    },
                ],
                vec![
                    Point {
                        x: 527,
                        y: 135,
                        width: 10,
                    },
                    Point {
                        x: 527,
                        y: 135,
                        width: 10,
                    },
                    Point {
                        x: 527,
                        y: 135,
                        width: 10,
                    },
                ],
                vec![
                    Point {
                        x: 513,
                        y: 163,
                        width: 10,
                    },
                    Point {
                        x: 513,
                        y: 165,
                        width: 10,
                    },
                    Point {
                        x: 514,
                        y: 166,
                        width: 10,
                    },
                    Point {
                        x: 514,
                        y: 167,
                        width: 10,
                    },
                    Point {
                        x: 514,
                        y: 168,
                        width: 10,
                    },
                    Point {
                        x: 515,
                        y: 169,
                        width: 10,
                    },
                    Point {
                        x: 515,
                        y: 169,
                        width: 10,
                    },
                    Point {
                        x: 516,
                        y: 169,
                        width: 10,
                    },
                    Point {
                        x: 517,
                        y: 168,
                        width: 9,
                    },
                    Point {
                        x: 517,
                        y: 167,
                        width: 9,
                    },
                    Point {
                        x: 518,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 519,
                        y: 164,
                        width: 8,
                    },
                    Point {
                        x: 520,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 520,
                        y: 160,
                        width: 8,
                    },
                    Point {
                        x: 521,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 522,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 521,
                        y: 159,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 572,
                        y: 178,
                        width: 10,
                    },
                    Point {
                        x: 572,
                        y: 175,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 174,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 173,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 171,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 169,
                        width: 9,
                    },
                    Point {
                        x: 572,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 572,
                        y: 166,
                        width: 8,
                    },
                    Point {
                        x: 572,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 572,
                        y: 163,
                        width: 8,
                    },
                    Point {
                        x: 572,
                        y: 162,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 158,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 156,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 155,
                        width: 8,
                    },
                    Point {
                        x: 571,
                        y: 154,
                        width: 8,
                    },
                    Point {
                        x: 570,
                        y: 152,
                        width: 8,
                    },
                    Point {
                        x: 570,
                        y: 151,
                        width: 9,
                    },
                    Point {
                        x: 570,
                        y: 150,
                        width: 9,
                    },
                    Point {
                        x: 570,
                        y: 149,
                        width: 9,
                    },
                    Point {
                        x: 569,
                        y: 148,
                        width: 9,
                    },
                    Point {
                        x: 569,
                        y: 147,
                        width: 9,
                    },
                    Point {
                        x: 569,
                        y: 146,
                        width: 9,
                    },
                    Point {
                        x: 569,
                        y: 145,
                        width: 9,
                    },
                    Point {
                        x: 569,
                        y: 145,
                        width: 10,
                    },
                    Point {
                        x: 569,
                        y: 144,
                        width: 10,
                    },
                    Point {
                        x: 569,
                        y: 143,
                        width: 10,
                    },
                    Point {
                        x: 570,
                        y: 142,
                        width: 10,
                    },
                    Point {
                        x: 570,
                        y: 142,
                        width: 10,
                    },
                    Point {
                        x: 571,
                        y: 142,
                        width: 10,
                    },
                    Point {
                        x: 571,
                        y: 141,
                        width: 10,
                    },
                    Point {
                        x: 572,
                        y: 141,
                        width: 10,
                    },
                    Point {
                        x: 573,
                        y: 141,
                        width: 10,
                    },
                    Point {
                        x: 574,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 575,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 576,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 577,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 578,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 579,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 580,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 581,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 583,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 584,
                        y: 141,
                        width: 9,
                    },
                    Point {
                        x: 585,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 587,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 588,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 589,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 591,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 592,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 594,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 595,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 596,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 598,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 599,
                        y: 141,
                        width: 8,
                    },
                    Point {
                        x: 601,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 602,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 603,
                        y: 140,
                        width: 8,
                    },
                    Point {
                        x: 604,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 605,
                        y: 139,
                        width: 9,
                    },
                    Point {
                        x: 606,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 607,
                        y: 138,
                        width: 9,
                    },
                    Point {
                        x: 608,
                        y: 137,
                        width: 9,
                    },
                    Point {
                        x: 609,
                        y: 136,
                        width: 8,
                    },
                    Point {
                        x: 610,
                        y: 136,
                        width: 8,
                    },
                    Point {
                        x: 609,
                        y: 136,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 644,
                        y: 40,
                        width: 10,
                    },
                    Point {
                        x: 645,
                        y: 56,
                        width: 8,
                    },
                    Point {
                        x: 645,
                        y: 62,
                        width: 6,
                    },
                    Point {
                        x: 645,
                        y: 68,
                        width: 5,
                    },
                    Point {
                        x: 646,
                        y: 73,
                        width: 4,
                    },
                    Point {
                        x: 646,
                        y: 77,
                        width: 3,
                    },
                    Point {
                        x: 646,
                        y: 82,
                        width: 2,
                    },
                    Point {
                        x: 647,
                        y: 86,
                        width: 2,
                    },
                    Point {
                        x: 648,
                        y: 92,
                        width: 2,
                    },
                    Point {
                        x: 648,
                        y: 97,
                        width: 1,
                    },
                    Point {
                        x: 649,
                        y: 103,
                        width: 1,
                    },
                    Point {
                        x: 650,
                        y: 108,
                        width: 1,
                    },
                    Point {
                        x: 651,
                        y: 114,
                        width: 1,
                    },
                    Point {
                        x: 651,
                        y: 120,
                        width: 1,
                    },
                    Point {
                        x: 652,
                        y: 126,
                        width: 1,
                    },
                    Point {
                        x: 652,
                        y: 132,
                        width: 1,
                    },
                    Point {
                        x: 653,
                        y: 138,
                        width: 1,
                    },
                    Point {
                        x: 653,
                        y: 143,
                        width: 1,
                    },
                    Point {
                        x: 653,
                        y: 148,
                        width: 1,
                    },
                    Point {
                        x: 654,
                        y: 153,
                        width: 1,
                    },
                    Point {
                        x: 654,
                        y: 158,
                        width: 2,
                    },
                    Point {
                        x: 654,
                        y: 162,
                        width: 2,
                    },
                    Point {
                        x: 654,
                        y: 166,
                        width: 3,
                    },
                    Point {
                        x: 655,
                        y: 170,
                        width: 3,
                    },
                    Point {
                        x: 655,
                        y: 173,
                        width: 4,
                    },
                    Point {
                        x: 655,
                        y: 176,
                        width: 4,
                    },
                    Point {
                        x: 655,
                        y: 179,
                        width: 5,
                    },
                    Point {
                        x: 655,
                        y: 181,
                        width: 6,
                    },
                    Point {
                        x: 655,
                        y: 183,
                        width: 6,
                    },
                    Point {
                        x: 656,
                        y: 185,
                        width: 7,
                    },
                    Point {
                        x: 656,
                        y: 187,
                        width: 7,
                    },
                    Point {
                        x: 656,
                        y: 189,
                        width: 8,
                    },
                    Point {
                        x: 656,
                        y: 190,
                        width: 8,
                    },
                    Point {
                        x: 656,
                        y: 192,
                        width: 8,
                    },
                    Point {
                        x: 656,
                        y: 193,
                        width: 8,
                    },
                    Point {
                        x: 656,
                        y: 194,
                        width: 9,
                    },
                    Point {
                        x: 657,
                        y: 195,
                        width: 9,
                    },
                    Point {
                        x: 657,
                        y: 196,
                        width: 9,
                    },
                    Point {
                        x: 657,
                        y: 196,
                        width: 9,
                    },
                    Point {
                        x: 658,
                        y: 196,
                        width: 9,
                    },
                    Point {
                        x: 659,
                        y: 195,
                        width: 9,
                    },
                    Point {
                        x: 659,
                        y: 195,
                        width: 9,
                    },
                    Point {
                        x: 659,
                        y: 195,
                        width: 9,
                    },
                ],
                vec![
                    Point {
                        x: 696,
                        y: 140,
                        width: 10,
                    },
                    Point {
                        x: 693,
                        y: 142,
                        width: 9,
                    },
                    Point {
                        x: 692,
                        y: 144,
                        width: 8,
                    },
                    Point {
                        x: 691,
                        y: 145,
                        width: 8,
                    },
                    Point {
                        x: 690,
                        y: 147,
                        width: 8,
                    },
                    Point {
                        x: 689,
                        y: 148,
                        width: 8,
                    },
                    Point {
                        x: 689,
                        y: 150,
                        width: 8,
                    },
                    Point {
                        x: 688,
                        y: 152,
                        width: 8,
                    },
                    Point {
                        x: 687,
                        y: 153,
                        width: 8,
                    },
                    Point {
                        x: 686,
                        y: 155,
                        width: 8,
                    },
                    Point {
                        x: 685,
                        y: 157,
                        width: 8,
                    },
                    Point {
                        x: 685,
                        y: 159,
                        width: 8,
                    },
                    Point {
                        x: 684,
                        y: 161,
                        width: 8,
                    },
                    Point {
                        x: 684,
                        y: 163,
                        width: 7,
                    },
                    Point {
                        x: 683,
                        y: 165,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 167,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 168,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 170,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 172,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 174,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 176,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 177,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 179,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 180,
                        width: 8,
                    },
                    Point {
                        x: 683,
                        y: 181,
                        width: 9,
                    },
                    Point {
                        x: 683,
                        y: 182,
                        width: 9,
                    },
                    Point {
                        x: 683,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 684,
                        y: 183,
                        width: 9,
                    },
                    Point {
                        x: 685,
                        y: 184,
                        width: 9,
                    },
                    Point {
                        x: 686,
                        y: 184,
                        width: 9,
                    },
                    Point {
                        x: 687,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 689,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 691,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 692,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 694,
                        y: 184,
                        width: 8,
                    },
                    Point {
                        x: 696,
                        y: 184,
                        width: 7,
                    },
                    Point {
                        x: 698,
                        y: 184,
                        width: 7,
                    },
                    Point {
                        x: 700,
                        y: 183,
                        width: 7,
                    },
                    Point {
                        x: 702,
                        y: 182,
                        width: 7,
                    },
                    Point {
                        x: 704,
                        y: 182,
                        width: 7,
                    },
                    Point {
                        x: 706,
                        y: 181,
                        width: 7,
                    },
                    Point {
                        x: 708,
                        y: 180,
                        width: 7,
                    },
                    Point {
                        x: 709,
                        y: 179,
                        width: 7,
                    },
                    Point {
                        x: 711,
                        y: 178,
                        width: 8,
                    },
                    Point {
                        x: 712,
                        y: 177,
                        width: 8,
                    },
                    Point {
                        x: 713,
                        y: 176,
                        width: 8,
                    },
                    Point {
                        x: 714,
                        y: 175,
                        width: 8,
                    },
                    Point {
                        x: 715,
                        y: 174,
                        width: 8,
                    },
                    Point {
                        x: 716,
                        y: 173,
                        width: 8,
                    },
                    Point {
                        x: 717,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 718,
                        y: 172,
                        width: 9,
                    },
                    Point {
                        x: 719,
                        y: 171,
                        width: 9,
                    },
                    Point {
                        x: 720,
                        y: 170,
                        width: 8,
                    },
                    Point {
                        x: 721,
                        y: 169,
                        width: 8,
                    },
                    Point {
                        x: 720,
                        y: 170,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 718,
                        y: 45,
                        width: 10,
                    },
                    Point {
                        x: 718,
                        y: 56,
                        width: 6,
                    },
                    Point {
                        x: 718,
                        y: 60,
                        width: 5,
                    },
                    Point {
                        x: 718,
                        y: 65,
                        width: 4,
                    },
                    Point {
                        x: 718,
                        y: 69,
                        width: 4,
                    },
                    Point {
                        x: 718,
                        y: 73,
                        width: 3,
                    },
                    Point {
                        x: 718,
                        y: 78,
                        width: 3,
                    },
                    Point {
                        x: 718,
                        y: 82,
                        width: 2,
                    },
                    Point {
                        x: 718,
                        y: 87,
                        width: 2,
                    },
                    Point {
                        x: 718,
                        y: 91,
                        width: 2,
                    },
                    Point {
                        x: 717,
                        y: 96,
                        width: 2,
                    },
                    Point {
                        x: 717,
                        y: 101,
                        width: 2,
                    },
                    Point {
                        x: 717,
                        y: 105,
                        width: 2,
                    },
                    Point {
                        x: 716,
                        y: 110,
                        width: 2,
                    },
                    Point {
                        x: 716,
                        y: 114,
                        width: 2,
                    },
                    Point {
                        x: 715,
                        y: 119,
                        width: 2,
                    },
                    Point {
                        x: 715,
                        y: 123,
                        width: 2,
                    },
                    Point {
                        x: 715,
                        y: 127,
                        width: 3,
                    },
                    Point {
                        x: 714,
                        y: 131,
                        width: 3,
                    },
                    Point {
                        x: 714,
                        y: 135,
                        width: 3,
                    },
                    Point {
                        x: 714,
                        y: 138,
                        width: 3,
                    },
                    Point {
                        x: 714,
                        y: 142,
                        width: 4,
                    },
                    Point {
                        x: 714,
                        y: 146,
                        width: 4,
                    },
                    Point {
                        x: 714,
                        y: 149,
                        width: 4,
                    },
                    Point {
                        x: 714,
                        y: 152,
                        width: 4,
                    },
                    Point {
                        x: 714,
                        y: 155,
                        width: 5,
                    },
                    Point {
                        x: 714,
                        y: 158,
                        width: 5,
                    },
                    Point {
                        x: 714,
                        y: 161,
                        width: 6,
                    },
                    Point {
                        x: 714,
                        y: 163,
                        width: 6,
                    },
                    Point {
                        x: 714,
                        y: 165,
                        width: 6,
                    },
                    Point {
                        x: 714,
                        y: 168,
                        width: 6,
                    },
                    Point {
                        x: 714,
                        y: 170,
                        width: 7,
                    },
                    Point {
                        x: 715,
                        y: 172,
                        width: 7,
                    },
                    Point {
                        x: 715,
                        y: 174,
                        width: 7,
                    },
                    Point {
                        x: 716,
                        y: 175,
                        width: 8,
                    },
                    Point {
                        x: 716,
                        y: 177,
                        width: 8,
                    },
                    Point {
                        x: 717,
                        y: 178,
                        width: 8,
                    },
                    Point {
                        x: 718,
                        y: 179,
                        width: 8,
                    },
                    Point {
                        x: 717,
                        y: 178,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 423,
                        y: 207,
                        width: 10,
                    },
                    Point {
                        x: 425,
                        y: 207,
                        width: 9,
                    },
                    Point {
                        x: 426,
                        y: 207,
                        width: 9,
                    },
                    Point {
                        x: 427,
                        y: 207,
                        width: 9,
                    },
                    Point {
                        x: 429,
                        y: 207,
                        width: 9,
                    },
                    Point {
                        x: 430,
                        y: 207,
                        width: 9,
                    },
                    Point {
                        x: 431,
                        y: 207,
                        width: 8,
                    },
                    Point {
                        x: 433,
                        y: 207,
                        width: 8,
                    },
                    Point {
                        x: 434,
                        y: 207,
                        width: 8,
                    },
                    Point {
                        x: 436,
                        y: 207,
                        width: 8,
                    },
                    Point {
                        x: 438,
                        y: 207,
                        width: 8,
                    },
                    Point {
                        x: 440,
                        y: 207,
                        width: 7,
                    },
                    Point {
                        x: 443,
                        y: 207,
                        width: 7,
                    },
                    Point {
                        x: 445,
                        y: 207,
                        width: 6,
                    },
                    Point {
                        x: 448,
                        y: 207,
                        width: 6,
                    },
                    Point {
                        x: 451,
                        y: 207,
                        width: 5,
                    },
                    Point {
                        x: 454,
                        y: 207,
                        width: 5,
                    },
                    Point {
                        x: 458,
                        y: 207,
                        width: 4,
                    },
                    Point {
                        x: 461,
                        y: 207,
                        width: 4,
                    },
                    Point {
                        x: 465,
                        y: 207,
                        width: 4,
                    },
                    Point {
                        x: 469,
                        y: 207,
                        width: 4,
                    },
                    Point {
                        x: 473,
                        y: 207,
                        width: 3,
                    },
                    Point {
                        x: 477,
                        y: 207,
                        width: 3,
                    },
                    Point {
                        x: 481,
                        y: 207,
                        width: 3,
                    },
                    Point {
                        x: 485,
                        y: 207,
                        width: 3,
                    },
                    Point {
                        x: 489,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 493,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 498,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 503,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 508,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 513,
                        y: 207,
                        width: 2,
                    },
                    Point {
                        x: 518,
                        y: 207,
                        width: 1,
                    },
                    Point {
                        x: 524,
                        y: 207,
                        width: 1,
                    },
                    Point {
                        x: 529,
                        y: 207,
                        width: 1,
                    },
                    Point {
                        x: 535,
                        y: 207,
                        width: 1,
                    },
                    Point {
                        x: 541,
                        y: 207,
                        width: 1,
                    },
                    Point {
                        x: 548,
                        y: 206,
                        width: 1,
                    },
                    Point {
                        x: 554,
                        y: 205,
                        width: 1,
                    },
                    Point {
                        x: 560,
                        y: 204,
                        width: 1,
                    },
                    Point {
                        x: 567,
                        y: 204,
                        width: 1,
                    },
                    Point {
                        x: 573,
                        y: 203,
                        width: 1,
                    },
                    Point {
                        x: 580,
                        y: 202,
                        width: 1,
                    },
                    Point {
                        x: 586,
                        y: 201,
                        width: 1,
                    },
                    Point {
                        x: 592,
                        y: 201,
                        width: 1,
                    },
                    Point {
                        x: 598,
                        y: 200,
                        width: 1,
                    },
                    Point {
                        x: 604,
                        y: 199,
                        width: 1,
                    },
                    Point {
                        x: 610,
                        y: 198,
                        width: 1,
                    },
                    Point {
                        x: 615,
                        y: 198,
                        width: 1,
                    },
                    Point {
                        x: 621,
                        y: 197,
                        width: 1,
                    },
                    Point {
                        x: 626,
                        y: 196,
                        width: 1,
                    },
                    Point {
                        x: 632,
                        y: 196,
                        width: 1,
                    },
                    Point {
                        x: 637,
                        y: 195,
                        width: 1,
                    },
                    Point {
                        x: 643,
                        y: 195,
                        width: 1,
                    },
                    Point {
                        x: 648,
                        y: 195,
                        width: 1,
                    },
                    Point {
                        x: 653,
                        y: 195,
                        width: 1,
                    },
                    Point {
                        x: 658,
                        y: 194,
                        width: 1,
                    },
                    Point {
                        x: 663,
                        y: 194,
                        width: 1,
                    },
                    Point {
                        x: 668,
                        y: 194,
                        width: 1,
                    },
                    Point {
                        x: 673,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 678,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 682,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 687,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 691,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 695,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 699,
                        y: 194,
                        width: 3,
                    },
                    Point {
                        x: 703,
                        y: 194,
                        width: 3,
                    },
                    Point {
                        x: 706,
                        y: 195,
                        width: 3,
                    },
                    Point {
                        x: 710,
                        y: 195,
                        width: 4,
                    },
                    Point {
                        x: 713,
                        y: 196,
                        width: 4,
                    },
                    Point {
                        x: 716,
                        y: 196,
                        width: 4,
                    },
                    Point {
                        x: 719,
                        y: 197,
                        width: 5,
                    },
                    Point {
                        x: 721,
                        y: 198,
                        width: 5,
                    },
                    Point {
                        x: 724,
                        y: 199,
                        width: 6,
                    },
                    Point {
                        x: 726,
                        y: 200,
                        width: 6,
                    },
                    Point {
                        x: 728,
                        y: 200,
                        width: 6,
                    },
                    Point {
                        x: 730,
                        y: 201,
                        width: 7,
                    },
                    Point {
                        x: 731,
                        y: 202,
                        width: 7,
                    },
                    Point {
                        x: 733,
                        y: 203,
                        width: 8,
                    },
                    Point {
                        x: 734,
                        y: 203,
                        width: 8,
                    },
                    Point {
                        x: 736,
                        y: 204,
                        width: 8,
                    },
                    Point {
                        x: 737,
                        y: 204,
                        width: 8,
                    },
                    Point {
                        x: 738,
                        y: 205,
                        width: 9,
                    },
                    Point {
                        x: 739,
                        y: 206,
                        width: 8,
                    },
                    Point {
                        x: 739,
                        y: 206,
                        width: 8,
                    },
                    Point {
                        x: 739,
                        y: 206,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 440,
                        y: 225,
                        width: 10,
                    },
                    Point {
                        x: 442,
                        y: 225,
                        width: 9,
                    },
                    Point {
                        x: 443,
                        y: 225,
                        width: 9,
                    },
                    Point {
                        x: 444,
                        y: 225,
                        width: 9,
                    },
                    Point {
                        x: 445,
                        y: 225,
                        width: 9,
                    },
                    Point {
                        x: 447,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 448,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 450,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 451,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 453,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 455,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 456,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 458,
                        y: 225,
                        width: 8,
                    },
                    Point {
                        x: 460,
                        y: 225,
                        width: 7,
                    },
                    Point {
                        x: 463,
                        y: 225,
                        width: 7,
                    },
                    Point {
                        x: 465,
                        y: 225,
                        width: 7,
                    },
                    Point {
                        x: 468,
                        y: 225,
                        width: 6,
                    },
                    Point {
                        x: 470,
                        y: 225,
                        width: 6,
                    },
                    Point {
                        x: 473,
                        y: 225,
                        width: 6,
                    },
                    Point {
                        x: 476,
                        y: 225,
                        width: 5,
                    },
                    Point {
                        x: 480,
                        y: 225,
                        width: 5,
                    },
                    Point {
                        x: 483,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 487,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 490,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 494,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 498,
                        y: 225,
                        width: 3,
                    },
                    Point {
                        x: 502,
                        y: 225,
                        width: 3,
                    },
                    Point {
                        x: 506,
                        y: 225,
                        width: 3,
                    },
                    Point {
                        x: 510,
                        y: 224,
                        width: 2,
                    },
                    Point {
                        x: 515,
                        y: 224,
                        width: 2,
                    },
                    Point {
                        x: 519,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 524,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 529,
                        y: 222,
                        width: 2,
                    },
                    Point {
                        x: 533,
                        y: 222,
                        width: 2,
                    },
                    Point {
                        x: 538,
                        y: 221,
                        width: 2,
                    },
                    Point {
                        x: 543,
                        y: 220,
                        width: 2,
                    },
                    Point {
                        x: 548,
                        y: 220,
                        width: 1,
                    },
                    Point {
                        x: 553,
                        y: 219,
                        width: 1,
                    },
                    Point {
                        x: 559,
                        y: 219,
                        width: 1,
                    },
                    Point {
                        x: 564,
                        y: 218,
                        width: 1,
                    },
                    Point {
                        x: 570,
                        y: 218,
                        width: 1,
                    },
                    Point {
                        x: 575,
                        y: 217,
                        width: 1,
                    },
                    Point {
                        x: 581,
                        y: 217,
                        width: 1,
                    },
                    Point {
                        x: 587,
                        y: 216,
                        width: 1,
                    },
                    Point {
                        x: 592,
                        y: 216,
                        width: 1,
                    },
                    Point {
                        x: 598,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 604,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 610,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 616,
                        y: 214,
                        width: 1,
                    },
                    Point {
                        x: 622,
                        y: 214,
                        width: 1,
                    },
                    Point {
                        x: 628,
                        y: 214,
                        width: 1,
                    },
                    Point {
                        x: 634,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 640,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 645,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 651,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 657,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 663,
                        y: 211,
                        width: 1,
                    },
                    Point {
                        x: 669,
                        y: 211,
                        width: 1,
                    },
                    Point {
                        x: 675,
                        y: 211,
                        width: 1,
                    },
                    Point {
                        x: 681,
                        y: 210,
                        width: 1,
                    },
                    Point {
                        x: 686,
                        y: 210,
                        width: 1,
                    },
                    Point {
                        x: 692,
                        y: 210,
                        width: 1,
                    },
                    Point {
                        x: 697,
                        y: 209,
                        width: 1,
                    },
                    Point {
                        x: 703,
                        y: 209,
                        width: 1,
                    },
                    Point {
                        x: 707,
                        y: 209,
                        width: 1,
                    },
                    Point {
                        x: 712,
                        y: 209,
                        width: 2,
                    },
                    Point {
                        x: 717,
                        y: 209,
                        width: 2,
                    },
                    Point {
                        x: 721,
                        y: 209,
                        width: 2,
                    },
                    Point {
                        x: 724,
                        y: 209,
                        width: 3,
                    },
                    Point {
                        x: 727,
                        y: 209,
                        width: 4,
                    },
                    Point {
                        x: 730,
                        y: 210,
                        width: 4,
                    },
                    Point {
                        x: 733,
                        y: 210,
                        width: 5,
                    },
                    Point {
                        x: 735,
                        y: 211,
                        width: 6,
                    },
                    Point {
                        x: 737,
                        y: 212,
                        width: 6,
                    },
                    Point {
                        x: 739,
                        y: 213,
                        width: 6,
                    },
                    Point {
                        x: 741,
                        y: 214,
                        width: 7,
                    },
                    Point {
                        x: 742,
                        y: 215,
                        width: 7,
                    },
                    Point {
                        x: 744,
                        y: 216,
                        width: 8,
                    },
                    Point {
                        x: 745,
                        y: 217,
                        width: 8,
                    },
                    Point {
                        x: 746,
                        y: 218,
                        width: 8,
                    },
                    Point {
                        x: 747,
                        y: 219,
                        width: 8,
                    },
                    Point {
                        x: 748,
                        y: 219,
                        width: 8,
                    },
                    Point {
                        x: 747,
                        y: 219,
                        width: 8,
                    },
                ],
                vec![
                    Point {
                        x: 442,
                        y: 238,
                        width: 10,
                    },
                    Point {
                        x: 444,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 446,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 447,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 448,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 449,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 450,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 452,
                        y: 238,
                        width: 9,
                    },
                    Point {
                        x: 453,
                        y: 238,
                        width: 8,
                    },
                    Point {
                        x: 455,
                        y: 238,
                        width: 8,
                    },
                    Point {
                        x: 457,
                        y: 238,
                        width: 8,
                    },
                    Point {
                        x: 459,
                        y: 238,
                        width: 8,
                    },
                    Point {
                        x: 461,
                        y: 237,
                        width: 7,
                    },
                    Point {
                        x: 463,
                        y: 237,
                        width: 7,
                    },
                    Point {
                        x: 466,
                        y: 237,
                        width: 6,
                    },
                    Point {
                        x: 468,
                        y: 237,
                        width: 6,
                    },
                    Point {
                        x: 471,
                        y: 237,
                        width: 6,
                    },
                    Point {
                        x: 474,
                        y: 237,
                        width: 6,
                    },
                    Point {
                        x: 477,
                        y: 236,
                        width: 5,
                    },
                    Point {
                        x: 480,
                        y: 236,
                        width: 5,
                    },
                    Point {
                        x: 483,
                        y: 236,
                        width: 5,
                    },
                    Point {
                        x: 486,
                        y: 236,
                        width: 5,
                    },
                    Point {
                        x: 489,
                        y: 235,
                        width: 4,
                    },
                    Point {
                        x: 492,
                        y: 235,
                        width: 4,
                    },
                    Point {
                        x: 496,
                        y: 235,
                        width: 4,
                    },
                    Point {
                        x: 499,
                        y: 235,
                        width: 4,
                    },
                    Point {
                        x: 503,
                        y: 235,
                        width: 4,
                    },
                    Point {
                        x: 507,
                        y: 234,
                        width: 3,
                    },
                    Point {
                        x: 511,
                        y: 234,
                        width: 3,
                    },
                    Point {
                        x: 516,
                        y: 234,
                        width: 3,
                    },
                    Point {
                        x: 520,
                        y: 234,
                        width: 2,
                    },
                    Point {
                        x: 525,
                        y: 233,
                        width: 2,
                    },
                    Point {
                        x: 530,
                        y: 233,
                        width: 2,
                    },
                    Point {
                        x: 535,
                        y: 233,
                        width: 2,
                    },
                    Point {
                        x: 540,
                        y: 232,
                        width: 1,
                    },
                    Point {
                        x: 545,
                        y: 232,
                        width: 1,
                    },
                    Point {
                        x: 550,
                        y: 231,
                        width: 1,
                    },
                    Point {
                        x: 555,
                        y: 230,
                        width: 1,
                    },
                    Point {
                        x: 560,
                        y: 230,
                        width: 1,
                    },
                    Point {
                        x: 565,
                        y: 229,
                        width: 1,
                    },
                    Point {
                        x: 570,
                        y: 229,
                        width: 1,
                    },
                    Point {
                        x: 575,
                        y: 228,
                        width: 1,
                    },
                    Point {
                        x: 579,
                        y: 228,
                        width: 2,
                    },
                    Point {
                        x: 584,
                        y: 227,
                        width: 2,
                    },
                    Point {
                        x: 589,
                        y: 227,
                        width: 2,
                    },
                    Point {
                        x: 593,
                        y: 227,
                        width: 2,
                    },
                    Point {
                        x: 598,
                        y: 226,
                        width: 2,
                    },
                    Point {
                        x: 602,
                        y: 226,
                        width: 2,
                    },
                    Point {
                        x: 607,
                        y: 225,
                        width: 2,
                    },
                    Point {
                        x: 611,
                        y: 225,
                        width: 2,
                    },
                    Point {
                        x: 616,
                        y: 225,
                        width: 2,
                    },
                    Point {
                        x: 620,
                        y: 224,
                        width: 2,
                    },
                    Point {
                        x: 624,
                        y: 224,
                        width: 2,
                    },
                    Point {
                        x: 629,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 633,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 638,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 642,
                        y: 222,
                        width: 2,
                    },
                    Point {
                        x: 647,
                        y: 222,
                        width: 2,
                    },
                    Point {
                        x: 651,
                        y: 222,
                        width: 2,
                    },
                    Point {
                        x: 655,
                        y: 221,
                        width: 2,
                    },
                    Point {
                        x: 659,
                        y: 221,
                        width: 2,
                    },
                    Point {
                        x: 664,
                        y: 221,
                        width: 3,
                    },
                    Point {
                        x: 667,
                        y: 221,
                        width: 3,
                    },
                    Point {
                        x: 671,
                        y: 221,
                        width: 3,
                    },
                    Point {
                        x: 675,
                        y: 221,
                        width: 3,
                    },
                    Point {
                        x: 679,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 682,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 686,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 689,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 692,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 695,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 699,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 702,
                        y: 221,
                        width: 4,
                    },
                    Point {
                        x: 705,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 708,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 711,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 714,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 717,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 721,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 724,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 727,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 730,
                        y: 221,
                        width: 5,
                    },
                    Point {
                        x: 732,
                        y: 220,
                        width: 6,
                    },
                    Point {
                        x: 734,
                        y: 220,
                        width: 6,
                    },
                    Point {
                        x: 736,
                        y: 220,
                        width: 7,
                    },
                    Point {
                        x: 737,
                        y: 220,
                        width: 8,
                    },
                    Point {
                        x: 736,
                        y: 220,
                        width: 7,
                    },
                ],
                vec![
                    Point {
                        x: 12,
                        y: 194,
                        width: 10,
                    },
                    Point {
                        x: 16,
                        y: 194,
                        width: 8,
                    },
                    Point {
                        x: 19,
                        y: 194,
                        width: 7,
                    },
                    Point {
                        x: 22,
                        y: 194,
                        width: 6,
                    },
                    Point {
                        x: 26,
                        y: 194,
                        width: 5,
                    },
                    Point {
                        x: 29,
                        y: 194,
                        width: 4,
                    },
                    Point {
                        x: 34,
                        y: 194,
                        width: 3,
                    },
                    Point {
                        x: 38,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 43,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 48,
                        y: 194,
                        width: 2,
                    },
                    Point {
                        x: 54,
                        y: 193,
                        width: 1,
                    },
                    Point {
                        x: 59,
                        y: 193,
                        width: 1,
                    },
                    Point {
                        x: 64,
                        y: 193,
                        width: 1,
                    },
                    Point {
                        x: 70,
                        y: 192,
                        width: 1,
                    },
                    Point {
                        x: 75,
                        y: 192,
                        width: 1,
                    },
                    Point {
                        x: 81,
                        y: 191,
                        width: 1,
                    },
                    Point {
                        x: 86,
                        y: 191,
                        width: 1,
                    },
                    Point {
                        x: 92,
                        y: 190,
                        width: 1,
                    },
                    Point {
                        x: 98,
                        y: 190,
                        width: 1,
                    },
                    Point {
                        x: 103,
                        y: 189,
                        width: 1,
                    },
                    Point {
                        x: 109,
                        y: 188,
                        width: 1,
                    },
                    Point {
                        x: 115,
                        y: 188,
                        width: 1,
                    },
                    Point {
                        x: 121,
                        y: 187,
                        width: 1,
                    },
                    Point {
                        x: 127,
                        y: 186,
                        width: 1,
                    },
                    Point {
                        x: 133,
                        y: 185,
                        width: 1,
                    },
                    Point {
                        x: 139,
                        y: 185,
                        width: 1,
                    },
                    Point {
                        x: 145,
                        y: 184,
                        width: 1,
                    },
                    Point {
                        x: 152,
                        y: 183,
                        width: 1,
                    },
                    Point {
                        x: 158,
                        y: 182,
                        width: 1,
                    },
                    Point {
                        x: 164,
                        y: 182,
                        width: 1,
                    },
                    Point {
                        x: 171,
                        y: 181,
                        width: 1,
                    },
                    Point {
                        x: 177,
                        y: 180,
                        width: 1,
                    },
                    Point {
                        x: 183,
                        y: 180,
                        width: 1,
                    },
                    Point {
                        x: 190,
                        y: 179,
                        width: 1,
                    },
                    Point {
                        x: 196,
                        y: 178,
                        width: 1,
                    },
                    Point {
                        x: 202,
                        y: 177,
                        width: 1,
                    },
                    Point {
                        x: 208,
                        y: 177,
                        width: 1,
                    },
                    Point {
                        x: 215,
                        y: 176,
                        width: 1,
                    },
                    Point {
                        x: 221,
                        y: 176,
                        width: 1,
                    },
                    Point {
                        x: 228,
                        y: 175,
                        width: 1,
                    },
                    Point {
                        x: 234,
                        y: 175,
                        width: 1,
                    },
                    Point {
                        x: 240,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 246,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 252,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 259,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 265,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 271,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 276,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 282,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 288,
                        y: 174,
                        width: 1,
                    },
                    Point {
                        x: 293,
                        y: 175,
                        width: 1,
                    },
                    Point {
                        x: 298,
                        y: 175,
                        width: 1,
                    },
                    Point {
                        x: 303,
                        y: 176,
                        width: 1,
                    },
                    Point {
                        x: 308,
                        y: 177,
                        width: 1,
                    },
                    Point {
                        x: 313,
                        y: 177,
                        width: 2,
                    },
                    Point {
                        x: 317,
                        y: 178,
                        width: 2,
                    },
                    Point {
                        x: 322,
                        y: 179,
                        width: 2,
                    },
                    Point {
                        x: 326,
                        y: 180,
                        width: 2,
                    },
                    Point {
                        x: 328,
                        y: 181,
                        width: 4,
                    },
                    Point {
                        x: 331,
                        y: 181,
                        width: 5,
                    },
                    Point {
                        x: 333,
                        y: 182,
                        width: 6,
                    },
                    Point {
                        x: 334,
                        y: 183,
                        width: 7,
                    },
                    Point {
                        x: 333,
                        y: 182,
                        width: 6,
                    },
                ],
                vec![
                    Point {
                        x: 20,
                        y: 209,
                        width: 10,
                    },
                    Point {
                        x: 23,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 25,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 27,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 28,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 30,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 32,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 34,
                        y: 209,
                        width: 8,
                    },
                    Point {
                        x: 36,
                        y: 208,
                        width: 8,
                    },
                    Point {
                        x: 38,
                        y: 208,
                        width: 7,
                    },
                    Point {
                        x: 40,
                        y: 208,
                        width: 7,
                    },
                    Point {
                        x: 43,
                        y: 207,
                        width: 6,
                    },
                    Point {
                        x: 45,
                        y: 207,
                        width: 6,
                    },
                    Point {
                        x: 48,
                        y: 206,
                        width: 6,
                    },
                    Point {
                        x: 51,
                        y: 206,
                        width: 5,
                    },
                    Point {
                        x: 54,
                        y: 206,
                        width: 5,
                    },
                    Point {
                        x: 58,
                        y: 205,
                        width: 4,
                    },
                    Point {
                        x: 62,
                        y: 205,
                        width: 4,
                    },
                    Point {
                        x: 66,
                        y: 205,
                        width: 3,
                    },
                    Point {
                        x: 70,
                        y: 204,
                        width: 3,
                    },
                    Point {
                        x: 74,
                        y: 204,
                        width: 2,
                    },
                    Point {
                        x: 79,
                        y: 204,
                        width: 2,
                    },
                    Point {
                        x: 83,
                        y: 204,
                        width: 2,
                    },
                    Point {
                        x: 88,
                        y: 204,
                        width: 2,
                    },
                    Point {
                        x: 93,
                        y: 204,
                        width: 2,
                    },
                    Point {
                        x: 98,
                        y: 203,
                        width: 2,
                    },
                    Point {
                        x: 103,
                        y: 203,
                        width: 2,
                    },
                    Point {
                        x: 108,
                        y: 203,
                        width: 1,
                    },
                    Point {
                        x: 113,
                        y: 203,
                        width: 1,
                    },
                    Point {
                        x: 119,
                        y: 203,
                        width: 1,
                    },
                    Point {
                        x: 124,
                        y: 203,
                        width: 1,
                    },
                    Point {
                        x: 130,
                        y: 202,
                        width: 1,
                    },
                    Point {
                        x: 136,
                        y: 202,
                        width: 1,
                    },
                    Point {
                        x: 143,
                        y: 201,
                        width: 1,
                    },
                    Point {
                        x: 149,
                        y: 201,
                        width: 1,
                    },
                    Point {
                        x: 156,
                        y: 200,
                        width: 1,
                    },
                    Point {
                        x: 162,
                        y: 199,
                        width: 1,
                    },
                    Point {
                        x: 169,
                        y: 199,
                        width: 1,
                    },
                    Point {
                        x: 175,
                        y: 198,
                        width: 1,
                    },
                    Point {
                        x: 182,
                        y: 197,
                        width: 1,
                    },
                    Point {
                        x: 189,
                        y: 196,
                        width: 1,
                    },
                    Point {
                        x: 195,
                        y: 195,
                        width: 1,
                    },
                    Point {
                        x: 202,
                        y: 194,
                        width: 1,
                    },
                    Point {
                        x: 209,
                        y: 193,
                        width: 1,
                    },
                    Point {
                        x: 215,
                        y: 192,
                        width: 1,
                    },
                    Point {
                        x: 222,
                        y: 191,
                        width: 1,
                    },
                    Point {
                        x: 228,
                        y: 190,
                        width: 1,
                    },
                    Point {
                        x: 234,
                        y: 190,
                        width: 1,
                    },
                    Point {
                        x: 239,
                        y: 189,
                        width: 1,
                    },
                    Point {
                        x: 245,
                        y: 188,
                        width: 1,
                    },
                    Point {
                        x: 250,
                        y: 187,
                        width: 1,
                    },
                    Point {
                        x: 256,
                        y: 186,
                        width: 1,
                    },
                    Point {
                        x: 261,
                        y: 186,
                        width: 1,
                    },
                    Point {
                        x: 266,
                        y: 185,
                        width: 1,
                    },
                    Point {
                        x: 271,
                        y: 185,
                        width: 1,
                    },
                    Point {
                        x: 276,
                        y: 184,
                        width: 2,
                    },
                    Point {
                        x: 280,
                        y: 184,
                        width: 2,
                    },
                    Point {
                        x: 285,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 289,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 294,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 298,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 302,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 306,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 310,
                        y: 183,
                        width: 2,
                    },
                    Point {
                        x: 314,
                        y: 183,
                        width: 3,
                    },
                    Point {
                        x: 318,
                        y: 183,
                        width: 3,
                    },
                    Point {
                        x: 322,
                        y: 183,
                        width: 4,
                    },
                    Point {
                        x: 325,
                        y: 183,
                        width: 4,
                    },
                    Point {
                        x: 328,
                        y: 183,
                        width: 4,
                    },
                    Point {
                        x: 331,
                        y: 183,
                        width: 5,
                    },
                    Point {
                        x: 333,
                        y: 183,
                        width: 6,
                    },
                    Point {
                        x: 335,
                        y: 184,
                        width: 6,
                    },
                    Point {
                        x: 337,
                        y: 185,
                        width: 7,
                    },
                    Point {
                        x: 337,
                        y: 185,
                        width: 8,
                    },
                    Point {
                        x: 337,
                        y: 185,
                        width: 7,
                    },
                ],
                vec![
                    Point {
                        x: 28,
                        y: 230,
                        width: 10,
                    },
                    Point {
                        x: 32,
                        y: 230,
                        width: 8,
                    },
                    Point {
                        x: 34,
                        y: 230,
                        width: 8,
                    },
                    Point {
                        x: 36,
                        y: 230,
                        width: 8,
                    },
                    Point {
                        x: 38,
                        y: 230,
                        width: 7,
                    },
                    Point {
                        x: 40,
                        y: 230,
                        width: 7,
                    },
                    Point {
                        x: 43,
                        y: 229,
                        width: 7,
                    },
                    Point {
                        x: 45,
                        y: 229,
                        width: 6,
                    },
                    Point {
                        x: 48,
                        y: 229,
                        width: 6,
                    },
                    Point {
                        x: 50,
                        y: 228,
                        width: 6,
                    },
                    Point {
                        x: 53,
                        y: 228,
                        width: 6,
                    },
                    Point {
                        x: 56,
                        y: 227,
                        width: 5,
                    },
                    Point {
                        x: 59,
                        y: 227,
                        width: 5,
                    },
                    Point {
                        x: 62,
                        y: 226,
                        width: 5,
                    },
                    Point {
                        x: 65,
                        y: 226,
                        width: 4,
                    },
                    Point {
                        x: 69,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 72,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 76,
                        y: 225,
                        width: 4,
                    },
                    Point {
                        x: 80,
                        y: 224,
                        width: 4,
                    },
                    Point {
                        x: 83,
                        y: 224,
                        width: 4,
                    },
                    Point {
                        x: 87,
                        y: 224,
                        width: 3,
                    },
                    Point {
                        x: 91,
                        y: 224,
                        width: 3,
                    },
                    Point {
                        x: 95,
                        y: 224,
                        width: 3,
                    },
                    Point {
                        x: 99,
                        y: 223,
                        width: 3,
                    },
                    Point {
                        x: 103,
                        y: 223,
                        width: 3,
                    },
                    Point {
                        x: 108,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 112,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 117,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 121,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 126,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 131,
                        y: 223,
                        width: 2,
                    },
                    Point {
                        x: 136,
                        y: 223,
                        width: 1,
                    },
                    Point {
                        x: 142,
                        y: 222,
                        width: 1,
                    },
                    Point {
                        x: 147,
                        y: 222,
                        width: 1,
                    },
                    Point {
                        x: 152,
                        y: 222,
                        width: 1,
                    },
                    Point {
                        x: 158,
                        y: 222,
                        width: 1,
                    },
                    Point {
                        x: 163,
                        y: 221,
                        width: 1,
                    },
                    Point {
                        x: 169,
                        y: 221,
                        width: 1,
                    },
                    Point {
                        x: 175,
                        y: 220,
                        width: 1,
                    },
                    Point {
                        x: 180,
                        y: 220,
                        width: 1,
                    },
                    Point {
                        x: 186,
                        y: 219,
                        width: 1,
                    },
                    Point {
                        x: 191,
                        y: 219,
                        width: 1,
                    },
                    Point {
                        x: 197,
                        y: 218,
                        width: 1,
                    },
                    Point {
                        x: 203,
                        y: 218,
                        width: 1,
                    },
                    Point {
                        x: 208,
                        y: 217,
                        width: 1,
                    },
                    Point {
                        x: 214,
                        y: 217,
                        width: 1,
                    },
                    Point {
                        x: 219,
                        y: 216,
                        width: 1,
                    },
                    Point {
                        x: 225,
                        y: 216,
                        width: 1,
                    },
                    Point {
                        x: 230,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 236,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 241,
                        y: 215,
                        width: 1,
                    },
                    Point {
                        x: 247,
                        y: 214,
                        width: 1,
                    },
                    Point {
                        x: 252,
                        y: 214,
                        width: 1,
                    },
                    Point {
                        x: 258,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 263,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 268,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 273,
                        y: 213,
                        width: 1,
                    },
                    Point {
                        x: 278,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 284,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 289,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 294,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 298,
                        y: 212,
                        width: 1,
                    },
                    Point {
                        x: 303,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 308,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 312,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 317,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 321,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 326,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 330,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 335,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 339,
                        y: 212,
                        width: 2,
                    },
                    Point {
                        x: 344,
                        y: 211,
                        width: 2,
                    },
                    Point {
                        x: 348,
                        y: 211,
                        width: 2,
                    },
                    Point {
                        x: 352,
                        y: 211,
                        width: 2,
                    },
                    Point {
                        x: 355,
                        y: 211,
                        width: 3,
                    },
                    Point {
                        x: 359,
                        y: 210,
                        width: 4,
                    },
                    Point {
                        x: 361,
                        y: 210,
                        width: 6,
                    },
                    Point {
                        x: 362,
                        y: 209,
                        width: 7,
                    },
                    Point {
                        x: 361,
                        y: 210,
                        width: 6,
                    },
                ],
            ],
        };

        assert_eq!(balloon, expected);
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
}
