/*!
 Sticker image conversion.
*/

use std::{
    env::temp_dir,
    ffi::OsStr,
    fs::{create_dir_all, remove_dir_all},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use imessage_database::tables::attachment::MediaType;

use crate::app::compatibility::{
    converters::common::{copy_raw, ensure_output_dir, run_command},
    models::{Converter, ImageConverter, ImageType, VideoConverter},
};

/// Copy a sticker, converting HEIC/HEICS files when possible.
///
/// - Sticker `HEIC` files convert to `PNG`
/// - Sticker `HEICS` files convert to `GIF`
/// - Otherwise copy the original format.
pub(crate) fn sticker_copy_convert(
    from: &Path,
    to: &mut PathBuf,
    image_converter: &ImageConverter,
    video_converter: Option<&VideoConverter>,
    mime_type: &MediaType,
) -> std::io::Result<Option<MediaType<'static>>> {
    // Determine the output type of the sticker
    let output_type: Option<ImageType> = match mime_type {
        // Static stickers convert to PNG; animated sticker sequences convert to GIF.
        MediaType::Image("heic" | "HEIC") => Some(ImageType::Png),
        MediaType::Image("heics" | "HEICS" | "heic-sequence") => Some(ImageType::Gif),
        _ => None,
    };

    if let Some(output_type) = output_type {
        // Update extension for conversion
        let mut converted_path = to.clone();
        converted_path.set_extension(output_type.to_str());

        // Animated stickers use the HEICS video streams when ffmpeg is available.
        if matches!(output_type, ImageType::Gif)
            && let Some(video_converter) = video_converter
        {
            if convert_heics(from, &converted_path, video_converter).is_some() {
                *to = converted_path;
                return Ok(Some(MediaType::Image(output_type.to_str())));
            }
            eprintln!("Unable to convert {}", from.display());
        }

        // Static conversion also handles animated stickers when ffmpeg is unavailable.
        if convert_heic(from, &converted_path, image_converter, &output_type).is_some() {
            *to = converted_path;
            return Ok(Some(MediaType::Image(output_type.to_str())));
        }
        eprintln!("Unable to convert {}", from.display());
    }

    // Fallback
    copy_raw(from, to)?;
    Ok(None)
}

/// Convert a HEIC sticker file to the provided format.
///
/// This uses the macOS built-in `sips` program or ImageMagick.
///
/// If `to` contains a directory that does not exist, i.e. `/fake/out.jpg`, instead
/// of failing, `sips` will create a file called `fake` in `/`. Subsequent writes
/// by `sips` to the same location will not fail, but since it is a file instead
/// of a directory, this will fail for non-`sips` copies.
///
/// Sticker HEIC files contain 5 images: 320x320, 160x160, 96x96, 64x64, and 40x40
///
/// `magick` attempts to extract all frames; this code selects the highest
/// resolution frame to match `sips`.
fn convert_heic(
    from: &Path,
    to: &Path,
    converter: &ImageConverter,
    output_image_type: &ImageType,
) -> Option<()> {
    ensure_output_dir(to)?;

    match converter {
        ImageConverter::Sips => {
            let args: Vec<&OsStr> = vec![
                OsStr::new("-s"),
                OsStr::new("format"),
                OsStr::new(output_image_type.to_str()),
                from.as_os_str(),
                OsStr::new("-o"),
                to.as_os_str(),
            ];
            run_command(converter.name(), args)
        }
        ImageConverter::Imagemagick => {
            // ImageMagick selects the first frame of a multi-frame HEIC with the
            // `path[0]` syntax. Build the suffixed argument as `OsString` so we
            // don't need the source path to round-trip through UTF-8.
            let mut formatted_from = from.as_os_str().to_owned();
            formatted_from.push("[0]");
            let args: Vec<&OsStr> = vec![&formatted_from, to.as_os_str()];
            run_command(converter.name(), args)
        }
    }
}

fn convert_heics(from: &Path, to: &Path, video_converter: &VideoConverter) -> Option<()> {
    let tmp_path = unique_tmp_dir()?;
    let result = convert_heics_with_tmp(from, to, video_converter, &tmp_path);
    // Best-effort cleanup of the per-call scratch directory, regardless of outcome.
    let _ = remove_dir_all(&tmp_path);
    result
}

fn convert_heics_with_tmp(
    from: &Path,
    to: &Path,
    video_converter: &VideoConverter,
    tmp_path: &Path,
) -> Option<()> {
    ensure_output_dir(to)?;

    // Frames per second in the original sticker.
    let fps = 10;

    match video_converter {
        VideoConverter::Ffmpeg => {
            let frame_pattern = tmp_path.join("frame_%04d.png");
            let alpha_pattern = tmp_path.join("alpha_%04d.png");
            let merged_pattern = tmp_path.join("merged_%04d.png");
            let palette_path = tmp_path.join("palette.png");
            let first_merged = tmp_path.join("merged_0000.png");
            let paletteuse = format!("fps={fps},paletteuse=alpha_threshold=128");

            // HEICS stickers contain four video streams. The first two are
            // still-frame image/alpha streams.
            // Stream #0:0[0x1]: Video: hevc (Main) (hvc1 / 0x31637668), yuv420p(tv, smpte170m/unknown/unknown), 524x600, 1 fps, 1 tbr, 1 tbn (default)
            // Stream #0:1[0x2]: Video: hevc (Rext) (hvc1 / 0x31637668), gray(pc), 524x600, 1 fps, 1 tbr, 1 tbn

            // The third stream is the animated video data.
            // Stream #0:2[0x1](und): Video: hevc (Main) (hvc1 / 0x31637668), yuv420p(tv, smpte170m/unknown/unknown), 524x600, 1370 kb/s, 22.98 fps, 30 tbr, 600 tbn (default)
            let frames_args: Vec<&OsStr> = vec![
                OsStr::new("-i"),
                from.as_os_str(),
                OsStr::new("-map"),
                OsStr::new("0:2"),
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-y"),
                frame_pattern.as_os_str(),
            ];
            run_command(video_converter.name(), frames_args)?;

            // The fourth stream is the animated alpha mask.
            // Stream #0:3[0x2](und): Video: hevc (Rext) (hvc1 / 0x31637668), gray(pc), 524x600, 426 kb/s, 22.98 fps, 30 tbr, 600 tbn (default)
            let alpha_args: Vec<&OsStr> = vec![
                OsStr::new("-i"),
                from.as_os_str(),
                OsStr::new("-map"),
                OsStr::new("0:3"),
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-y"),
                alpha_pattern.as_os_str(),
            ];
            run_command(video_converter.name(), alpha_args)?;

            // Apply the transparency mask to every frame in a single ffmpeg
            // invocation. The image2 demuxer reads both sequences in lockstep
            // and `alphamerge` pairs frame N with alpha N, so we avoid the
            // per-frame process-spawn cost that previously made animated
            // stickers take several seconds. Every image2 sequence read and
            // write in this function pins `-start_number 0` so the pipeline
            // does not depend on the muxer (default 1) and demuxer (default
            // 0) defaults agreeing across ffmpeg builds.
            let merge_args: Vec<&OsStr> = vec![
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-i"),
                frame_pattern.as_os_str(),
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-i"),
                alpha_pattern.as_os_str(),
                OsStr::new("-filter_complex"),
                OsStr::new(
                    "[1:v]format=gray,geq=lum='p(X,Y)':a='p(X,Y)'[mask];[0:v][mask]alphamerge",
                ),
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-y"),
                merged_pattern.as_os_str(),
            ];
            run_command(video_converter.name(), merge_args)?;

            // Build a transparency palette from the first merged frame.
            let palette_args: Vec<&OsStr> = vec![
                OsStr::new("-i"),
                first_merged.as_os_str(),
                OsStr::new("-vf"),
                OsStr::new("palettegen=reserve_transparent=1"),
                palette_path.as_os_str(),
            ];
            run_command(video_converter.name(), palette_args)?;

            // Encode the transparent frames as a GIF.
            let gif_args: Vec<&OsStr> = vec![
                OsStr::new("-start_number"),
                OsStr::new("0"),
                OsStr::new("-i"),
                merged_pattern.as_os_str(),
                OsStr::new("-i"),
                palette_path.as_os_str(),
                OsStr::new("-lavfi"),
                OsStr::new(&paletteuse),
                OsStr::new("-gifflags"),
                OsStr::new("-offsetting"),
                to.as_os_str(),
            ];
            run_command(video_converter.name(), gif_args)?;

            Some(())
        }
    }
}

/// Build a unique scratch directory under the platform temp dir.
///
/// Names include the process ID and a nanosecond timestamp so that
/// concurrent invocations do not collide on filesystem state.
fn unique_tmp_dir() -> Option<PathBuf> {
    let suffix = format!(
        "imessage-stickers-{}-{}",
        process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos(),
    );
    let path = temp_dir().join(suffix);
    if let Err(why) = create_dir_all(&path) {
        eprintln!("Unable to create {}: {why}", path.display());
        return None;
    }
    Some(path)
}
