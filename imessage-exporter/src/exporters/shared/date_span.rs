use std::path::Path;

use crate::app::file_times::{set_file_times, unix_to_system_time};

/// A message date as `(unix_seconds, nanoseconds)`.
///
/// Lexicographic order on the pair is chronological order: `nanoseconds` is
/// always a positive offset from `unix_seconds`, on both sides of the epoch.
pub(crate) type Timestamp = (i64, u32);

/// The earliest and latest message date written into one output file during
/// the current export.
#[derive(Clone, Copy)]
pub(crate) struct DateSpan {
    earliest: Timestamp,
    latest: Timestamp,
}

impl DateSpan {
    /// Open a span on a single message.
    pub(crate) fn new(at: Timestamp) -> Self {
        Self {
            earliest: at,
            latest: at,
        }
    }

    /// Widen both bounds to cover `at`; callers may supply dates in any order.
    pub(crate) fn extend(&mut self, at: Timestamp) {
        if at < self.earliest {
            self.earliest = at;
        }
        if at > self.latest {
            self.latest = at;
        }
    }

    /// Stamp `path` with creation time from the earliest message and
    /// modification time from the latest. Access time remains unchanged.
    ///
    /// [`set_file_times`] reports metadata failures without failing the export.
    /// A bound that overflows [`SystemTime`](std::time::SystemTime) is omitted.
    /// Creation time applies only on macOS and Windows.
    pub(crate) fn apply(&self, path: &Path) {
        set_file_times(
            path,
            unix_to_system_time(self.earliest.0, self.earliest.1),
            unix_to_system_time(self.latest.0, self.latest.1),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::DateSpan;

    #[test]
    fn can_span_out_of_order_dates() {
        let mut span = DateSpan::new((100, 0));
        span.extend((300, 0));
        span.extend((50, 0));
        span.extend((200, 0));

        assert_eq!(span.earliest, (50, 0));
        assert_eq!(span.latest, (300, 0));
    }

    #[test]
    fn can_span_dates_within_one_second() {
        // Negative seconds carry a positive nanosecond offset, so the pair
        // still orders chronologically before the epoch.
        let mut span = DateSpan::new((-5, 500));
        span.extend((-5, 900));
        span.extend((-5, 100));

        assert_eq!(span.earliest, (-5, 100));
        assert_eq!(span.latest, (-5, 900));
    }
}
