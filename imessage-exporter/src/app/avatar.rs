/*!
 Convert raw image bytes (from AddressBook / iMessage attachment) into a base64
 Data URL suitable for ChatLab's `members[].avatar` and `meta.groupAvatar` fields.
*/

/// Recognized image MIME types.  Anything not in this list returns `None` from `sniff_mime`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ImageMime {
    Jpeg,
    Png,
    Gif,
    Webp,
    Heic,
    Tiff,
}

impl ImageMime {
    /// The `image/...` string used in Data URLs.
    pub fn as_str(self) -> &'static str {
        match self {
            ImageMime::Jpeg => "image/jpeg",
            ImageMime::Png  => "image/png",
            ImageMime::Gif  => "image/gif",
            ImageMime::Webp => "image/webp",
            ImageMime::Heic => "image/heic",
            ImageMime::Tiff => "image/tiff",
        }
    }

    /// True when major browsers render the format inline via `<img>`.
    pub fn is_browser_renderable(self) -> bool {
        matches!(self, ImageMime::Jpeg | ImageMime::Png | ImageMime::Gif | ImageMime::Webp)
    }
}

/// Detect the image format from the leading magic bytes.  Returns `None` for unknown formats.
pub fn sniff_mime(bytes: &[u8]) -> Option<ImageMime> {
    if bytes.len() < 12 {
        return None;
    }
    // JPEG: FF D8 FF
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageMime::Jpeg);
    }
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageMime::Png);
    }
    // GIF: 47 49 46 38 (matches GIF87a and GIF89a)
    if bytes.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
        return Some(ImageMime::Gif);
    }
    // WebP: RIFF .... WEBP
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(ImageMime::Webp);
    }
    // HEIC: ?? ?? ?? ?? "ftyp" then brand at offset 8
    if bytes.get(4..8) == Some(b"ftyp") {
        if let Some(brand) = bytes.get(8..12) {
            if matches!(brand, b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx"
                              | b"mif1" | b"msf1" | b"avif")
            {
                return Some(ImageMime::Heic);
            }
        }
    }
    // TIFF: little-endian or big-endian
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(ImageMime::Tiff);
    }
    None
}

/// Encode bytes as a `data:image/...;base64,...` URL.  No transcoding.
fn encode_data_url(mime: ImageMime, bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    format!("data:{};base64,{}", mime.as_str(), STANDARD.encode(bytes))
}

use crate::app::compatibility::models::ImageConverter;

/// Convert raw image bytes to a base64 Data URL.  Returns `None` if the format is unrecognized
/// or HEIC/TIFF input is given without a converter.  Can transcode HEIC/TIFF input to JPEG via
/// the system image converter.  Returns `None` if the format is unrecognized, or
/// HEIC/TIFF input is given without a converter available, or transcoding fails.
pub fn bytes_to_data_url_with_converter(
    bytes: &[u8],
    converter: Option<&ImageConverter>,
) -> Option<String> {
    let mime = sniff_mime(bytes)?;
    if mime.is_browser_renderable() {
        return Some(encode_data_url(mime, bytes));
    }
    // HEIC / TIFF — needs transcode
    let converter = converter?;
    let transcoded = transcode_to_jpeg(bytes, mime, converter)?;
    Some(encode_data_url(ImageMime::Jpeg, &transcoded))
}

/// Write input bytes to a temp file, invoke the converter to produce a JPEG, read the result.
/// Cleans up both temp files before returning.
fn transcode_to_jpeg(
    bytes: &[u8],
    src_mime: ImageMime,
    converter: &ImageConverter,
) -> Option<Vec<u8>> {
    use std::fs::{remove_file, write, File};
    use std::io::Read;

    // Pick a stable temp-dir location
    let tmp_dir = std::env::temp_dir();
    let stem = format!("imex-avatar-{}", std::process::id());
    let src_ext = match src_mime {
        ImageMime::Heic => "heic",
        ImageMime::Tiff => "tiff",
        _ => return None,
    };
    let src_path = tmp_dir.join(format!("{stem}.{src_ext}"));
    let dst_path = tmp_dir.join(format!("{stem}.jpg"));

    if write(&src_path, bytes).is_err() {
        return None;
    }

    // Reuse the existing convert helper from the image module.
    // It runs `sips` or `imagemagick` to produce a JPEG at dst_path.
    let ok = crate::app::compatibility::converters::image::convert_to_jpeg_for_avatar(
        &src_path, &dst_path, converter,
    );

    let result = if ok {
        let mut buf = Vec::new();
        File::open(&dst_path).ok()?.read_to_end(&mut buf).ok()?;
        Some(buf)
    } else {
        None
    };

    let _ = remove_file(&src_path);
    let _ = remove_file(&dst_path);
    result
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_jpeg_returns_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Jpeg));
    }

    #[test]
    fn sniff_png_returns_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Png));
    }

    #[test]
    fn sniff_gif_returns_gif() {
        let bytes = [0x47, 0x49, 0x46, 0x38, b'9', b'a', 0, 0, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Gif));
    }

    #[test]
    fn sniff_webp_returns_webp() {
        let bytes = *b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Webp));
    }

    #[test]
    fn sniff_heic_returns_heic() {
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Heic));
    }

    #[test]
    fn sniff_tiff_little_endian_returns_tiff() {
        let bytes = [0x49, 0x49, 0x2A, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(sniff_mime(&bytes), Some(ImageMime::Tiff));
    }

    #[test]
    fn sniff_unknown_returns_none() {
        let bytes = [0u8; 32];
        assert_eq!(sniff_mime(&bytes), None);
    }

    #[test]
    fn sniff_short_input_returns_none() {
        assert_eq!(sniff_mime(&[1, 2, 3]), None);
    }

    #[test]
    fn bytes_to_data_url_jpeg_round_trips() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        let url = bytes_to_data_url_with_converter(&bytes, None).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        // Decode the base64 portion and verify it matches the input
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let b64 = url.strip_prefix("data:image/jpeg;base64,").unwrap();
        let decoded = STANDARD.decode(b64).unwrap();
        assert_eq!(&decoded[..], &bytes[..]);
    }

    #[test]
    fn bytes_to_data_url_heic_returns_none_without_transcoder() {
        // Until Task 4 lifts this limit, HEIC bytes can't become a Data URL.
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(bytes_to_data_url_with_converter(&bytes, None), None);
    }

    #[test]
    fn bytes_to_data_url_unknown_returns_none() {
        let bytes = [0u8; 16];
        assert_eq!(bytes_to_data_url_with_converter(&bytes, None), None);
    }

    #[test]
    fn bytes_to_data_url_with_converter_none_for_heic_returns_none() {
        let bytes = *b"\x00\x00\x00\x18ftypheic";
        assert_eq!(bytes_to_data_url_with_converter(&bytes, None), None);
    }

    #[test]
    fn bytes_to_data_url_with_converter_passes_through_jpeg() {
        // Even with a converter available, JPEG should not be transcoded — direct encode.
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0];
        // We pass None for the converter because for JPEG we never reach the conversion path.
        let url = bytes_to_data_url_with_converter(&bytes, None).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }
}
