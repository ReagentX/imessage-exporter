/*!
 Defines routines for converting image files.
*/

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_output_dir, run_command},
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
) -> std::io::Result<Option<MediaType<'static>>> {
    if matches!(mime_type, MediaType::Image("heic" | "HEIC")) {
        let output_type = ImageType::Jpeg;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_heic(from, &converted_path, converter, &output_type).is_some() {
            // If the conversion was successful, update the path
            *to = converted_path;
            return Ok(Some(MediaType::Image(output_type.to_str())));
        }
        eprintln!("Unable to convert {}", from.display());
    }

    // Fallback
    copy_raw(from, to)?;
    Ok(None)
}

/// Convert a HEIC image file to the provided format
///
/// This uses the macOS builtin `sips` program or `ImageMagick`
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
    ensure_output_dir(to)?;

    let args: Vec<&OsStr> = match converter {
        ImageConverter::Sips => vec![
            OsStr::new("-s"),
            OsStr::new("format"),
            OsStr::new(output_image_type.to_str()),
            from.as_os_str(),
            OsStr::new("-o"),
            to.as_os_str(),
        ],
        ImageConverter::Imagemagick => vec![from.as_os_str(), to.as_os_str()],
    };

    run_command(converter.name(), args)
}
