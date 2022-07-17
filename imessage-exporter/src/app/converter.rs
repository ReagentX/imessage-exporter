use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

/// Convert a HEIC image file to a JPEG
///
/// This uses the MacOS builtin `sips` program
/// Docs: https://www.unix.com/man-page/osx/1/sips/ (or `man sips`)
pub fn heic_to_jpeg(from: &PathBuf, to: &PathBuf) -> Option<()> {
    // Get the path we want to copy from
    let from_path = if let Some(from_path) = from.as_path().to_str() {
        from_path
    } else {
        return None;
    };

    // Get the path we want to write to
    let to_path = if let Some(to_path) = to.as_path().to_str() {
        to_path
    } else {
        return None;
    };

    // Build the comment
    match Command::new("sips")
        .args(&vec!["-s", "format", "jpeg", from_path, "-o", to_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        // TODO: make this log stuff
        Ok(mut sips) => match sips.wait() {
            Ok(success) => Some(()),
            Err(why) => None,
        },
        Err(why) => None,
    }
}
