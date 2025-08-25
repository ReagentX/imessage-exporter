/*!
 Defines the export progress bar.
*/

use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

const TEMPLATE_DEFAULT: &str =
    "{spinner:.green} [{elapsed}] [{bar:.blue}] {human_pos}/{human_len} ({per_sec}, ETA: {eta})";
const TEMPLATE_BUSY: &str =
    "{spinner:.green} [{elapsed}] [{bar:.blue}] {human_pos}/{human_len} (ETA: N/A) {msg}";

/// Wrapper around indicatif's `ProgressBar` with specialized functionality
pub struct ExportProgress {
    pub bar: ProgressBar,
}

impl ExportProgress {
    /// Creates a new hidden progress bar with default style
    pub fn new() -> Self {
        let bar = ProgressBar::hidden();
        bar.set_style(
            ProgressStyle::default_bar()
                .template(TEMPLATE_DEFAULT)
                .unwrap()
                .progress_chars("#>-"),
        );

        Self { bar }
    }

    /// Starts the progress bar with the specified total length
    pub fn start(&self, length: u64) {
        self.bar.set_position(0);
        self.bar.enable_steady_tick(Duration::from_millis(100));
        self.bar.set_length(length);
        self.bar.set_draw_target(ProgressDrawTarget::stdout());
    }

    /// Sets the progress bar to default style
    pub fn set_default_style(&self) {
        self.bar.set_style(
            ProgressStyle::default_bar()
                .template(TEMPLATE_DEFAULT)
                .unwrap()
                .progress_chars("#>-"),
        );
        self.bar.enable_steady_tick(Duration::from_millis(100));
    }

    /// Sets the progress bar to busy style with a message
    pub fn set_busy_style(&self, message: String) {
        self.bar.set_style(
            ProgressStyle::default_bar()
                .template(TEMPLATE_BUSY)
                .unwrap()
                .progress_chars("#>-"),
        );
        self.bar.set_message(message);
        self.bar.enable_steady_tick(Duration::from_millis(250));
    }

    /// Sets the position of the progress bar
    pub fn set_position(&self, pos: u64) {
        self.bar.set_position(pos);
    }

    /// Finishes the progress bar
    pub fn finish(&self) {
        self.bar.finish();
    }
}

impl Default for ExportProgress {
    fn default() -> Self {
        Self::new()
    }
}
