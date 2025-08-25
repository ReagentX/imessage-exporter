/*!
 Defines routines for converting audio files.
*/

use std::path::{Path, PathBuf};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_paths, run_command},
    models::{AudioConverter, AudioType, Converter},
};

/// Copy an audio file, converting if possible
///
/// - Attachment `CAF` files convert to `MP4`
/// - Fallback to the original format
pub(crate) fn audio_copy_convert(
    from: &Path,
    to: &mut PathBuf,
    converter: &AudioConverter,
    mime_type: MediaType,
) -> Option<MediaType<'static>> {
    if matches!(
        mime_type,
        MediaType::Audio("caf" | "CAF" | "x-caf; codecs=opus" | "amr" | "AMR")
    ) {
        let output_type = AudioType::Mp4;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_caf(from, &converted_path, converter).is_some() {
            // If the conversion was successful, update the path
            *to = converted_path;
            return Some(MediaType::Audio(output_type.to_str()));
        }
        eprintln!("Unable to convert {from:?}");
    }

    // Fallback
    copy_raw(from, to);
    None
}

fn convert_caf(from: &Path, to: &Path, converter: &AudioConverter) -> Option<()> {
    let (from_path, to_path) = ensure_paths(from, to)?;

    let args = match converter {
        AudioConverter::AfConvert => vec!["-f", "mp4f", "-d", "aac", "-v", from_path, to_path],
        AudioConverter::Ffmpeg => vec!["-i", from_path, to_path],
    };

    run_command(converter.name(), args)
}
