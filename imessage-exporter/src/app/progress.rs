use indicatif::{ProgressBar, ProgressStyle};

pub fn build_progress_bar(total_messages: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_messages);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed}] [{bar:.blue}] {pos}/{len} {percent}% ({per_sec}, {eta})")
        .progress_chars("#>-")
    );
    pb.set_position(0);
    pb
}
