use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PROFILE_DIR_FROM_OUT_DIR_DEPTH: usize = 3;
const VERSION_ARG: &str = "-version";

fn main() {
    if env::var("PROFILE").as_deref() != Ok("release")
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
    {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let profile_dir = out_dir
        .ancestors()
        .nth(PROFILE_DIR_FROM_OUT_DIR_DEPTH)
        .expect("Cargo OUT_DIR is under target/<profile>/build/<package>/out");

    let magick = profile_dir
        .join("tools")
        .join("imagemagick")
        .join("magick.exe");
    let ffmpeg = profile_dir
        .join("tools")
        .join("ffmpeg")
        .join("bin")
        .join("ffmpeg.exe");

    println!("cargo:rerun-if-changed={}", magick.display());
    println!("cargo:rerun-if-changed={}", ffmpeg.display());

    require_executable("ImageMagick", &magick, VERSION_ARG);
    require_executable("ffmpeg", &ffmpeg, VERSION_ARG);
}

fn require_executable(name: &str, path: &Path, version_arg: &str) {
    assert!(
        path.is_file(),
        "{name} must be bundled at {} for release GUI builds",
        path.display()
    );

    let mut command = Command::new(path);
    command.arg(version_arg);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().unwrap_or_else(|err| {
        panic!(
            "could not execute bundled {name} at {}: {err}",
            path.display()
        )
    });

    assert!(
        output.status.success(),
        "bundled {name} at {} failed {version_arg} with status {}",
        path.display(),
        output.status
    );
}
