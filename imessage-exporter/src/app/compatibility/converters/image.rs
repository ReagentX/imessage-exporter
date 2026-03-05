/*!
 Defines routines for converting image files.
*/

use std::path::{Path, PathBuf};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_paths, run_command},
    models::{Converter, ImageConverter, ImageType},
};

/// Copy an image file, converting if possible
///
/// - Attachment `HEIC` files convert to `JPEG`
/// - Fallback to the original format
pub(crate) fn image_copy_convert(
    from: &Path,
    to: &mut PathBuf,
    converter: &ImageConverter,
    mime_type: &MediaType,
) -> Option<MediaType<'static>> {
    if is_heic_attachment(mime_type, from) {
        let output_type = ImageType::Jpeg;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_heic(from, &converted_path, converter, &output_type).is_some() {
            // If the conversion was successful, update the path
            *to = converted_path;
            return Some(MediaType::Image(output_type.to_str()));
        }
        eprintln!("Unable to convert {}", from.display());
    }

    // Fallback
    copy_raw(from, to);
    None
}

/// Convert a HEIC image file to the provided format
///
/// This uses the macOS builtin `sips` program
///
/// Docs: <https://www.unix.com/man-page/osx/1/sips/> (or `man sips`)
///
/// If `to` contains a directory that does not exist, i.e. `/fake/out.jpg`, instead
/// of failing, `sips` will create a file called `fake` in `/`. Subsequent writes
/// by `sips` to the same location will not fail, but since it is a file instead
/// of a directory, this will fail for non-`sips` copies.
fn convert_heic(
    from: &Path,
    to: &Path,
    converter: &ImageConverter,
    output_image_type: &ImageType,
) -> Option<()> {
    let (from_path, to_path) = ensure_paths(from, to)?;

    match converter {
        ImageConverter::Sips => run_command(
            converter.name(),
            vec![
                "-s",
                "format",
                output_image_type.to_str(),
                from_path,
                "-o",
                to_path,
            ],
        ),
        ImageConverter::Imagemagick(_) => {
            let formatted_from = format!("{from_path}[0]");
            let formatted_to = format!("{}:{to_path}", output_image_type.to_str());
            run_command(
                converter.name(),
                vec![&formatted_from, "-auto-orient", &formatted_to],
            )
        }
    }
}

fn is_heic_attachment(mime_type: &MediaType, path: &Path) -> bool {
    let is_heic_mime = matches!(
        mime_type,
        MediaType::Image(subtype)
            if subtype.eq_ignore_ascii_case("heic") || subtype.eq_ignore_ascii_case("heif")
    );
    let is_heic_ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("heic") || ext.eq_ignore_ascii_case("heif"));

    is_heic_mime || is_heic_ext
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use imessage_database::tables::attachment::MediaType;

    use crate::app::compatibility::converters::image::is_heic_attachment;

    #[test]
    fn detects_heic_mime_type() {
        assert!(is_heic_attachment(
            &MediaType::Image("heic"),
            Path::new("file.jpg")
        ));
        assert!(is_heic_attachment(
            &MediaType::Image("HEIF"),
            Path::new("file.jpg")
        ));
    }

    #[test]
    fn detects_heic_extension() {
        assert!(is_heic_attachment(
            &MediaType::Image("jpeg"),
            Path::new("file.HEIC")
        ));
        assert!(is_heic_attachment(
            &MediaType::Unknown,
            Path::new("file.heif")
        ));
    }

    #[test]
    fn ignores_non_heic_files() {
        assert!(!is_heic_attachment(
            &MediaType::Image("png"),
            Path::new("file.png")
        ));
    }
}
