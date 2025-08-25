/*!
 Defines routines for converting video files.
*/

use std::path::{Path, PathBuf};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_paths, run_command},
    models::{Converter, HardwareEncoder, VideoConverter, VideoType},
};

/// Copy a video file, converting if possible
///
/// - Attachment `MOV` files convert to `MP4`
/// - Fallback to the original format
pub(crate) fn video_copy_convert(
    from: &Path,
    to: &mut PathBuf,
    converter: &VideoConverter,
    hardware_encoder: &Option<HardwareEncoder>,
    mime_type: MediaType,
) -> Option<MediaType<'static>> {
    if matches!(mime_type, MediaType::Video("mov" | "MOV" | "quicktime")) {
        let output_type = VideoType::Mp4;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_mov(from, &converted_path, converter, hardware_encoder).is_some() {
            *to = converted_path;
            return Some(MediaType::Video(output_type.to_str()));
        }
        eprintln!("Unable to convert {from:?}");
    }

    // Fallback
    copy_raw(from, to);
    None
}

/// Build ffmpeg arguments for remuxing without re-encoding
fn build_remux_args<'a>(from_path: &'a str, to_path: &'a str) -> Vec<&'a str> {
    vec![
        "-i",
        from_path,
        "-c",
        "copy",
        "-f",
        VideoType::Mp4.to_str(),
        to_path,
    ]
}

// Build ffmpeg arguments for encoding with optional hardware acceleration
fn build_encode_args<'a>(
    from_path: &'a str,
    to_path: &'a str,
    hw: Option<&HardwareEncoder>,
) -> Vec<&'a str> {
    let mut args = vec!["-i", from_path];
    if let Some(hw) = hw {
        args.extend(&["-c:v", hw.codec_name(), "-preset", "fast"]);
    } else {
        args.extend(&["-c:v", "libx264", "-preset", "fast"]);
    }
    args.extend(&["-c:a", "copy", "-movflags", "+faststart", to_path]);
    args
}

// Convert a video file by attempting remuxing, falling back to hardware-accelerated or software re-encode
fn convert_mov(
    from: &Path,
    to: &Path,
    converter: &VideoConverter,
    hardware_encoder: &Option<HardwareEncoder>,
) -> Option<()> {
    let (from_path, to_path) = ensure_paths(from, to)?;

    // First, try remuxing into MP4 container without re-encoding
    let remux_args = build_remux_args(from_path, to_path);
    if run_command(converter.name(), remux_args).is_some() {
        return Some(());
    }

    // Remux failed; fallback to re-encoding
    let encode_args = build_encode_args(from_path, to_path, hardware_encoder.as_ref());
    run_command(converter.name(), encode_args)
}

#[cfg(test)]
mod tests {
    use crate::app::compatibility::{
        converters::video::{build_encode_args, build_remux_args},
        models::{HardwareEncoder, VideoType},
    };

    #[test]
    fn test_build_remux_args() {
        let from = "input.mov";
        let to = "output.mp4";
        let args = build_remux_args(from, to);
        let expected: Vec<&str> = vec!["-i", from, "-c", "copy", "-f", VideoType::Mp4.to_str(), to];
        assert_eq!(args, expected);
    }

    #[test]
    fn test_build_encode_args_hw() {
        let from = "in.mov";
        let to = "out.mp4";
        let args = build_encode_args(from, to, Some(&HardwareEncoder::Nvenc));
        let expected: Vec<&str> = vec![
            "-i",
            from,
            "-c:v",
            "h264_nvenc",
            "-preset",
            "fast",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
            to,
        ];
        assert_eq!(args, expected);
    }

    #[test]
    fn test_build_encode_args_sw() {
        let from = "in.mov";
        let to = "out.mp4";
        let args = build_encode_args(from, to, None);
        let expected: Vec<&str> = vec![
            "-i",
            from,
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
            to,
        ];
        assert_eq!(args, expected);
    }
}
