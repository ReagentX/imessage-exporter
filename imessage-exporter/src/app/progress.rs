use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

pub fn build_progress_bar_export(total_messages: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_messages);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed}] [{bar:.blue}] {pos}/{len} ({per_sec}, ETA: {eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_position(0);
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
