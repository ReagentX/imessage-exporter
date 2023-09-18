use std::{
    fs::create_dir_all,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug)]
pub enum Converter {
    Sips,
    Imagemagick,
}

impl Converter {
    /// Determine the converter type for the current shell environment
    pub fn determine() -> Option<Converter> {
        if exists("sips") {
            return Some(Converter::Sips);
        }
        if exists("convert") {
            return Some(Converter::Imagemagick);
        }
        eprintln!("No HEIC converter found, attachments will not be converted!");
        None
    }
}

/// Determine if a shell program exists on the system
fn exists(name: &str) -> bool {
    if let Ok(process) = Command::new("type")
        .args(&vec![name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        if let Ok(output) = process.wait_with_output() {
            return output.status.success();
        }
    };
    false
}

/// Convert a HEIC image file to a JPEG
///
/// This uses the macOS builtin `sips` program
/// Docs: <https://www.unix.com/man-page/osx/1/sips/> (or `man sips`)
///
/// If `to` contains a directory that does not exist, i.e. `/fake/out.jpg`, instead
/// of failing, `sips` will create a file called `fake` in `/`. Subsequent writes
/// by `sips` to the same location will not fail, but since it is a file instead
/// of a directory, this will fail for non-`sips` copies.
pub fn heic_to_jpeg(from: &Path, to: &Path, converter: &Converter) -> Option<()> {
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

    match converter {
        Converter::Sips => {
            // Build the command
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
        Converter::Imagemagick => {
            // Build the command
            match Command::new("convert")
                .args(&vec![from_path, to_path])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .spawn()
            {
                Ok(mut convert) => match convert.wait() {
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
    };

    Some(())
}

#[cfg(test)]
mod test {
    use super::exists;

    #[test]
    fn can_find_program() {
        assert!(exists("ls"))
    }

    #[test]
    fn can_miss_program() {
        assert!(!exists("fake_name"))
    }
}
