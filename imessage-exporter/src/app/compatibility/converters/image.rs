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
    mime_type: MediaType,
) -> Option<MediaType<'static>> {
    if matches!(mime_type, MediaType::Image("heic" | "HEIC")) {
        let output_type = ImageType::Jpeg;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_heic(from, &converted_path, converter, &output_type).is_some() {
            // If the conversion was successful, update the path
            *to = converted_path;
            return Some(MediaType::Image(output_type.to_str()));
        }
        eprintln!("Unable to convert {from:?}");
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

    let args = match converter {
        ImageConverter::Sips => vec![
            "-s",
            "format",
            output_image_type.to_str(),
            from_path,
            "-o",
            to_path,
        ],
        ImageConverter::Imagemagick => vec![from_path, to_path],
    };

    run_command(converter.name(), args)
}
