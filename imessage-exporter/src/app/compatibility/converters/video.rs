/*!
 Video attachment conversion.
*/

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_output_dir, run_command},
    models::{Converter, HardwareEncoder, VideoConverter, VideoType},
};

/// Copy a video file, converting MOV/QuickTime files when possible.
///
/// - Attachment `MOV` files convert to `MP4`
/// - Otherwise copy the original format.
pub(crate) fn video_copy_convert(
    from: &Path,
    to: &mut PathBuf,
    converter: &VideoConverter,
    hardware_encoder: &Option<HardwareEncoder>,
    mime_type: &MediaType,
) -> std::io::Result<Option<MediaType<'static>>> {
    if matches!(mime_type, MediaType::Video("mov" | "MOV" | "quicktime")) {
        let output_type = VideoType::Mp4;

        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        if convert_mov(from, &converted_path, converter, hardware_encoder.as_ref()).is_some() {
            *to = converted_path;
            return Ok(Some(MediaType::Video(output_type.to_str())));
        }
        eprintln!("Unable to convert {from:?}");
    }

    // Fallback
    copy_raw(from, to)?;
    Ok(None)
}

/// Build ffmpeg arguments for remuxing without re-encoding.
fn build_remux_args<'a>(from: &'a Path, to: &'a Path) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("-i"),
        from.as_os_str(),
        OsStr::new("-c"),
        OsStr::new("copy"),
        OsStr::new("-f"),
        OsStr::new(VideoType::Mp4.to_str()),
        to.as_os_str(),
    ]
}

// Build ffmpeg arguments for encoding with optional hardware acceleration.
fn build_encode_args<'a>(
    from: &'a Path,
    to: &'a Path,
    hw: Option<&'a HardwareEncoder>,
) -> Vec<&'a OsStr> {
    let mut args: Vec<&OsStr> = vec![OsStr::new("-i"), from.as_os_str()];
    if let Some(hw) = hw {
        args.extend([
            OsStr::new("-c:v"),
            OsStr::new(hw.codec_name()),
            OsStr::new("-preset"),
            OsStr::new("fast"),
        ]);
    } else {
        args.extend([
            OsStr::new("-c:v"),
            OsStr::new("libx264"),
            OsStr::new("-preset"),
            OsStr::new("fast"),
        ]);
    }
    args.extend([
        OsStr::new("-c:a"),
        OsStr::new("copy"),
        OsStr::new("-movflags"),
        OsStr::new("+faststart"),
        to.as_os_str(),
    ]);
    args
}

// Convert by remuxing first, then re-encoding when the remux fails.
fn convert_mov(
    from: &Path,
    to: &Path,
    converter: &VideoConverter,
    hardware_encoder: Option<&HardwareEncoder>,
) -> Option<()> {
    ensure_output_dir(to)?;

    // First, try remuxing into MP4 container without re-encoding
    let remux_args = build_remux_args(from, to);
    if run_command(converter.name(), remux_args).is_some() {
        return Some(());
    }

    // Remux failed; fallback to re-encoding
    let encode_args = build_encode_args(from, to, hardware_encoder);
    run_command(converter.name(), encode_args)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, path::Path};

    use crate::app::compatibility::{
        converters::video::{build_encode_args, build_remux_args},
        models::{HardwareEncoder, VideoType},
    };

    #[test]
    fn test_build_remux_args() {
        let from = Path::new("input.mov");
        let to = Path::new("output.mp4");
        let actual = build_remux_args(from, to);
        let expected: Vec<&OsStr> = vec![
            OsStr::new("-i"),
            from.as_os_str(),
            OsStr::new("-c"),
            OsStr::new("copy"),
            OsStr::new("-f"),
            OsStr::new(VideoType::Mp4.to_str()),
            to.as_os_str(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_build_encode_args_hw() {
        let from = Path::new("in.mov");
        let to = Path::new("out.mp4");
        let actual = build_encode_args(from, to, Some(&HardwareEncoder::Nvenc));
        let expected: Vec<&OsStr> = vec![
            OsStr::new("-i"),
            from.as_os_str(),
            OsStr::new("-c:v"),
            OsStr::new("h264_nvenc"),
            OsStr::new("-preset"),
            OsStr::new("fast"),
            OsStr::new("-c:a"),
            OsStr::new("copy"),
            OsStr::new("-movflags"),
            OsStr::new("+faststart"),
            to.as_os_str(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_build_encode_args_sw() {
        let from = Path::new("in.mov");
        let to = Path::new("out.mp4");
        let actual = build_encode_args(from, to, None);
        let expected: Vec<&OsStr> = vec![
            OsStr::new("-i"),
            from.as_os_str(),
            OsStr::new("-c:v"),
            OsStr::new("libx264"),
            OsStr::new("-preset"),
            OsStr::new("fast"),
            OsStr::new("-c:a"),
            OsStr::new("copy"),
            OsStr::new("-movflags"),
            OsStr::new("+faststart"),
            to.as_os_str(),
        ];
        assert_eq!(actual, expected);
    }
}
