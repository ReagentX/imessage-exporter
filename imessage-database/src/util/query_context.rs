/*!
 SQL filter state for message queries.
*/
use std::collections::BTreeSet;

use chrono::TimeZone;

use crate::{
    error::query_context::QueryContextError,
    util::dates::{MESSAGES_EPOCH_UNIX_SECS, TIMESTAMP_FACTOR},
};

#[derive(Debug, Default, PartialEq, Eq)]
/// Filters applied to message count and stream queries.
pub struct QueryContext {
    /// Inclusive lower-bound timestamp for message dates.
    pub start: Option<i64>,
    /// Inclusive upper-bound timestamp for message dates.
    pub end: Option<i64>,
    /// Handle row IDs included in the query.
    pub selected_handle_ids: Option<BTreeSet<i32>>,
    /// Chat row IDs included in the query.
    pub selected_chat_ids: Option<BTreeSet<i32>>,
}

impl QueryContext {
    /// Set the inclusive lower-bound date from `YYYY-MM-DD`.
    ///
    /// Use midnight on that date in `zone` as the bound.
    ///
    /// # Example:
    ///
    /// ```
    /// use chrono::Utc;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_start("2023-01-01", &Utc);
    /// ```
    pub fn set_start<Tz: TimeZone>(
        &mut self,
        start: &str,
        zone: &Tz,
    ) -> Result<(), QueryContextError> {
        let timestamp = QueryContext::sanitize_date(start, zone)
            .ok_or(QueryContextError::InvalidDate(start.to_string()))?;
        self.start = Some(timestamp);
        Ok(())
    }

    /// Set the inclusive upper-bound date from `YYYY-MM-DD`.
    ///
    /// Use midnight on that date in `zone` as the bound.
    ///
    /// # Example:
    ///
    /// ```
    /// use chrono::Utc;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_end("2023-01-01", &Utc);
    /// ```
    pub fn set_end<Tz: TimeZone>(&mut self, end: &str, zone: &Tz) -> Result<(), QueryContextError> {
        let timestamp = QueryContext::sanitize_date(end, zone)
            .ok_or(QueryContextError::InvalidDate(end.to_string()))?;
        self.end = Some(timestamp);
        Ok(())
    }

    /// Set the handle row IDs included in the query.
    ///
    /// # Example:
    ///
    /// ```
    /// use std::collections::BTreeSet;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_selected_handle_ids(BTreeSet::from([1, 2, 3]));
    /// ```
    pub fn set_selected_handle_ids(&mut self, selected_handle_ids: BTreeSet<i32>) {
        self.selected_handle_ids = (!selected_handle_ids.is_empty()).then_some(selected_handle_ids);
    }

    /// Set the chat row IDs included in the query.
    ///
    /// # Example:
    ///
    /// ```
    /// use std::collections::BTreeSet;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
    /// ```
    pub fn set_selected_chat_ids(&mut self, selected_chat_ids: BTreeSet<i32>) {
        self.selected_chat_ids = (!selected_chat_ids.is_empty()).then_some(selected_chat_ids);
    }

    /// Parse `YYYY-MM-DD` as the Messages timestamp of midnight in `zone`.
    fn sanitize_date<Tz: TimeZone>(date: &str, zone: &Tz) -> Option<i64> {
        if date.len() < 9 {
            return None;
        }

        let year = date.get(0..4)?.parse::<i32>().ok()?;

        if !date.get(4..5)?.eq("-") {
            return None;
        }

        let month = date.get(5..7)?.parse::<u32>().ok()?;
        if month > 12 {
            return None;
        }

        if !date.get(7..8)?.eq("-") {
            return None;
        }

        let day = date.get(8..)?.parse::<u32>().ok()?;
        if day > 31 {
            return None;
        }

        let midnight = zone.with_ymd_and_hms(year, month, day, 0, 0, 0).single()?;
        let stamp = midnight.timestamp_nanos_opt().unwrap_or(0);

        Some(stamp - (MESSAGES_EPOCH_UNIX_SECS * TIMESTAMP_FACTOR))
    }

    /// `true` when any filter is set.
    ///
    /// # Example:
    ///
    /// ```
    /// use chrono::Utc;
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// assert!(!context.has_filters());
    /// context.set_start("2023-01-01", &Utc);
    /// assert!(context.has_filters());
    /// ```
    #[must_use]
    pub fn has_filters(&self) -> bool {
        self.start.is_some()
            || self.end.is_some()
            || self.selected_chat_ids.is_some()
            || self.selected_handle_ids.is_some()
    }
}

#[cfg(test)]
mod use_tests {
    use chrono::prelude::*;

    use crate::util::{
        dates::{MESSAGES_EPOCH_UNIX_SECS, TIMESTAMP_FACTOR, format},
        query_context::QueryContext,
    };

    /// Use +05:30 to check midnight in the supplied zone rather than UTC or
    /// the host zone.
    fn zone() -> FixedOffset {
        FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap()
    }

    #[test]
    fn can_create_start() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01", &zone()).unwrap();

        let instant = DateTime::from_timestamp(
            (context.start.unwrap() / TIMESTAMP_FACTOR) + MESSAGES_EPOCH_UNIX_SECS,
            0,
        )
        .unwrap();

        assert_eq!(
            format(&instant.with_timezone(&zone())),
            "Jan 01, 2020 12:00:00 AM"
        );
        assert!(context.start.is_some());
        assert!(context.end.is_none());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_end() {
        let mut context = QueryContext::default();
        context.set_end("2020-01-01", &zone()).unwrap();

        let instant = DateTime::from_timestamp(
            (context.end.unwrap() / TIMESTAMP_FACTOR) + MESSAGES_EPOCH_UNIX_SECS,
            0,
        )
        .unwrap();

        assert_eq!(
            format(&instant.with_timezone(&zone())),
            "Jan 01, 2020 12:00:00 AM"
        );
        assert!(context.start.is_none());
        assert!(context.end.is_some());
        assert!(context.has_filters());
    }
}

#[cfg(test)]
mod id_tests {
    use std::collections::BTreeSet;

    use crate::util::query_context::QueryContext;

    #[test]
    fn test_can_set_selected_chat_ids() {
        let mut qc = QueryContext::default();
        qc.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));

        assert_eq!(qc.selected_chat_ids, Some(BTreeSet::from([1, 2, 3])));
        assert!(qc.has_filters());
    }

    #[test]
    fn test_can_overwrite_selected_chat_ids_empty() {
        let mut qc = QueryContext::default();
        qc.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        qc.set_selected_chat_ids(BTreeSet::new());

        assert_eq!(qc.selected_chat_ids, None);
        assert!(!qc.has_filters());
    }

    #[test]
    fn test_can_set_selected_handle_ids() {
        let mut qc = QueryContext::default();
        qc.set_selected_handle_ids(BTreeSet::from([1, 2, 3]));

        assert_eq!(qc.selected_handle_ids, Some(BTreeSet::from([1, 2, 3])));
        assert!(qc.has_filters());
    }

    #[test]
    fn test_can_overwrite_selected_handle_ids_empty() {
        let mut qc = QueryContext::default();
        qc.set_selected_handle_ids(BTreeSet::from([1, 2, 3]));
        qc.set_selected_handle_ids(BTreeSet::new());

        assert_eq!(qc.selected_handle_ids, None);
        assert!(!qc.has_filters());
    }
}

#[cfg(test)]
mod sanitize_tests {
    use chrono::Utc;

    use crate::util::query_context::QueryContext;

    #[test]
    fn can_reject_bad_short() {
        let res = QueryContext::sanitize_date("1-1-20", &Utc);
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_order() {
        let res = QueryContext::sanitize_date("01-01-2020", &Utc);
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_month() {
        let res = QueryContext::sanitize_date("2020-31-01", &Utc);
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_day() {
        let res = QueryContext::sanitize_date("2020-01-32", &Utc);
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_data() {
        let res = QueryContext::sanitize_date("2020-AB-CD", &Utc);
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_wrong_hyphen() {
        let res = QueryContext::sanitize_date("2020–01–01", &Utc);
        assert!(res.is_none());
    }
}
