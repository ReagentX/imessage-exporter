use std::{
    fs::create_dir_all,
    path::Path,
    process::{Command, Stdio},
};

/// Convert a HEIC image file to a JPEG
///
/// This uses the MacOS builtin `sips` program
/// Docs: <https://www.unix.com/man-page/osx/1/sips/> (or `man sips`)
///
/// If `to` contains a directory that does not exist, i.e. `/fake/out.jpg`, instead
/// of failing, `sips` will create a file called `fake` in `/`. Subsequent writes
/// by `sips` to the same location will not fail, but since it is a file instead
/// of a directory, this will fail for non-`sips` copies.
pub fn heic_to_jpeg(from: &Path, to: &Path) -> Option<()> {
    // Get the path we want to copy from
    let from_path = from.to_str()?;

    // Get the path we want to write to
    let to_path = to.to_str()?;

    // Ensure the directory tree exists
    if let Some(folder) = to.parent() {
        if !folder.exists() {
            if let Err(why) = create_dir_all(folder) {
                eprintln!("Unable to create {folder:?}: {why}");
                return None;
            }
        }
    }

    // Build the comment
    match Command::new("sips")
        .args(&vec!["-s", "format", "jpeg", from_path, "-o", to_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(mut sips) => match sips.wait() {
            Ok(_) => Some(()),
            Err(why) => {
                eprintln!("Conversion failed: {why}");
                None
            }
        },
        Err(why) => {
            eprintln!("Conversion failed: {why}");
            None
        }
    }
}
