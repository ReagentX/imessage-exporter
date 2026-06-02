/*!
 Contains data structures used to describe file converters and associated types.
*/

use std::{
    env,
    fmt::{Display, Formatter, Result},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(target_family = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(not(target_family = "windows"))]
fn command(name: &str) -> Command {
    Command::new(resolve_program(name).unwrap_or_else(|| PathBuf::from(name)))
}

#[cfg(target_family = "windows")]
fn bundled_command(name: &str) -> Option<Command> {
    let mut command = Command::new(resolve_program(name)?);
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Some(command)
}

/// Environment variable that can point at a portable converter tool directory.
///
/// Release builds also search beside the executable, but this hook keeps tests
/// and custom portable bundles deterministic without touching PATH.
pub const TOOLS_DIR_ENV: &str = "IMESSAGE_EXPORTER_TOOLS_DIR";

/// Resolve a converter executable from the portable tool bundle locations.
///
/// On Windows, converter detection uses these bundled locations only so the GUI
/// is self-contained and reproducible.
pub fn resolve_program(name: &str) -> Option<PathBuf> {
    resolve_program_from_roots(name, bundled_tool_roots())
}

fn resolve_program_from_roots(
    name: &str,
    roots: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    let exe_name = executable_name(name);

    for root in roots {
        for candidate in direct_candidates(&root, name, &exe_name) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        if let Some(candidate) = nested_candidate(&root, &exe_name) {
            return Some(candidate);
        }
    }

    None
}

fn bundled_tool_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(dir) = env::var(TOOLS_DIR_ENV) {
        roots.push(PathBuf::from(dir));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        push_roots(&mut roots, dir);
    }

    if let Ok(dir) = env::current_dir() {
        push_roots(&mut roots, &dir);
    }

    roots
}

fn push_roots(roots: &mut Vec<PathBuf>, dir: &Path) {
    roots.push(dir.to_path_buf());
    roots.push(dir.join("tools"));
    roots.push(dir.join("converters"));
}

fn direct_candidates(root: &Path, name: &str, exe_name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![
        root.join(exe_name),
        root.join("bin").join(exe_name),
        root.join(name).join(exe_name),
        root.join(name).join("bin").join(exe_name),
    ];

    match name {
        "ffmpeg" => {
            candidates.push(root.join("ffmpeg").join(exe_name));
            candidates.push(root.join("ffmpeg").join("bin").join(exe_name));
            candidates.push(root.join("FFmpeg").join(exe_name));
            candidates.push(root.join("FFmpeg").join("bin").join(exe_name));
        }
        "magick" => {
            candidates.push(root.join("imagemagick").join(exe_name));
            candidates.push(root.join("imagemagick").join("bin").join(exe_name));
            candidates.push(root.join("ImageMagick").join(exe_name));
            candidates.push(root.join("ImageMagick").join("bin").join(exe_name));
        }
        _ => {}
    }

    candidates
}

fn nested_candidate(root: &Path, exe_name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        for candidate in [path.join(exe_name), path.join("bin").join(exe_name)] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn executable_name(name: &str) -> String {
    #[cfg(target_family = "windows")]
    {
        if name.ends_with(".exe") {
            name.to_string()
        } else {
            format!("{name}.exe")
        }
    }

    #[cfg(not(target_family = "windows"))]
    {
        name.to_string()
    }
}

fn display_name(name: &str) -> String {
    if resolve_program(name).is_some() {
        format!("bundled {name}")
    } else {
        name.to_string()
    }
}

pub trait Converter {
    /// Determine the converter type for the current shell environment
    fn determine() -> Option<Self>
    where
        Self: Sized;

    /// The name of the program the current variant represents
    fn name(&self) -> &'static str
    where
        Self: Sized;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImageType {
    Jpeg,
    Gif,
    Png,
}

impl ImageType {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VideoType {
    Mp4,
}

impl VideoType {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AudioType {
    Mp4,
}

impl AudioType {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
/// Program used to convert/encode images
pub enum ImageConverter {
    /// macOS Builtin
    Sips,
    Imagemagick,
}

impl Converter for ImageConverter {
    fn determine() -> Option<ImageConverter> {
        if exists(ImageConverter::Sips.name()) {
            return Some(ImageConverter::Sips);
        }
        if exists(ImageConverter::Imagemagick.name()) {
            return Some(ImageConverter::Imagemagick);
        }
        eprintln!("No HEIC converter found, image attachments will not be converted!");
        None
    }

    fn name(&self) -> &'static str {
        match self {
            ImageConverter::Sips => "sips",
            ImageConverter::Imagemagick => "magick",
        }
    }
}

impl Display for ImageConverter {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", display_name(self.name()))
    }
}

#[derive(Debug, PartialEq, Eq)]
/// Program used to convert/encode audio
pub enum AudioConverter {
    /// macOS Builtin
    AfConvert,
    Ffmpeg,
}

impl Converter for AudioConverter {
    fn determine() -> Option<AudioConverter> {
        if exists(AudioConverter::AfConvert.name()) {
            return Some(AudioConverter::AfConvert);
        }
        if exists(AudioConverter::Ffmpeg.name()) {
            return Some(AudioConverter::Ffmpeg);
        }
        eprintln!("No CAF converter found, audio attachments will not be converted!");
        None
    }

    fn name(&self) -> &'static str {
        match self {
            AudioConverter::AfConvert => "afconvert",
            AudioConverter::Ffmpeg => "ffmpeg",
        }
    }
}

impl Display for AudioConverter {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", display_name(self.name()))
    }
}

#[derive(Debug, PartialEq, Eq)]
/// Program used to convert/encode videos
pub enum VideoConverter {
    Ffmpeg,
}

impl Converter for VideoConverter {
    fn determine() -> Option<VideoConverter> {
        if exists(VideoConverter::Ffmpeg.name()) {
            return Some(VideoConverter::Ffmpeg);
        }
        eprintln!("No MOV converter found, video attachments will not be converted!");
        None
    }

    fn name(&self) -> &'static str {
        match self {
            VideoConverter::Ffmpeg => "ffmpeg",
        }
    }
}

impl Display for VideoConverter {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", display_name(self.name()))
    }
}

/// Define supported hardware-based H.264 encoders
#[derive(Debug, PartialEq, Eq)]
pub enum HardwareEncoder {
    /// NVIDIA GPU-accelerated H.264 encoder (`NVENC`)
    Nvenc,
    /// Intel Quick Sync Video H.264 encoder (`QSV`)
    Qsv,
    /// Apple `VideoToolbox` H.264 encoder on macOS
    VideoToolbox,
}

impl HardwareEncoder {
    /// Detect best available hardware encoder in priority order
    pub fn detect() -> Option<Self> {
        #[cfg(target_family = "windows")]
        let mut command = bundled_command("ffmpeg")?;

        #[cfg(not(target_family = "windows"))]
        let mut command = command("ffmpeg");

        if let Ok(output) = command.args(["-hide_banner", "-encoders"]).output() {
            let out = String::from_utf8_lossy(&output.stdout);
            if out.contains("h264_nvenc") {
                return Some(Self::Nvenc);
            }
            if out.contains("h264_qsv") {
                return Some(Self::Qsv);
            }
            if out.contains("h264_videotoolbox") {
                return Some(Self::VideoToolbox);
            }
        }
        None
    }

    /// The name used by ffmpeg for this encoder
    pub fn codec_name(&self) -> &'static str {
        match self {
            HardwareEncoder::Nvenc => "h264_nvenc",
            HardwareEncoder::Qsv => "h264_qsv",
            HardwareEncoder::VideoToolbox => "h264_videotoolbox",
        }
    }
}

/// Determine if a shell program exists on the system
#[cfg(not(target_family = "windows"))]
fn exists(name: &str) -> bool {
    if resolve_program(name).is_some() {
        return true;
    }

    command("which")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Determine if a shell program exists on the system
#[cfg(target_family = "windows")]
fn exists(name: &str) -> bool {
    resolve_program(name).is_some()
}

#[cfg(test)]
mod test {
    use super::{executable_name, exists, resolve_program_from_roots};
    use std::{env, fs};

    #[test]
    fn can_find_bundled_program() {
        let root = env::temp_dir().join(format!(
            "imessage-exporter-tool-test-{}",
            std::process::id()
        ));
        let tool_dir = root.join("tools").join("ffmpeg").join("bin");
        fs::create_dir_all(&tool_dir).unwrap();

        let tool = tool_dir.join(executable_name("ffmpeg"));
        fs::write(&tool, []).unwrap();

        let resolved = resolve_program_from_roots("ffmpeg", [root.join("tools")]);
        assert_eq!(resolved.as_deref(), Some(tool.as_path()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn can_miss_program() {
        assert!(!exists("fake_name"));
    }
}
