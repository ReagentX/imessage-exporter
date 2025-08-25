/*!
 Contains logic for handling query filter configurations.
*/
use std::collections::BTreeSet;

use chrono::prelude::*;

use crate::{
    error::query_context::QueryContextError,
    util::dates::{TIMESTAMP_FACTOR, get_offset},
};

#[derive(Debug, Default, PartialEq, Eq)]
/// Represents filter configurations for a SQL query.
pub struct QueryContext {
    /// The start date filter. Only messages sent on or after this date will be included.
    pub start: Option<i64>,
    /// The end date filter. Only messages sent before this date will be included.
    pub end: Option<i64>,
    /// Selected handle IDs
    pub selected_handle_ids: Option<BTreeSet<i32>>,
    /// Selected chat IDs
    pub selected_chat_ids: Option<BTreeSet<i32>>,
}

impl QueryContext {
    /// Populate a [`QueryContext`] with a start date
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_start("2023-01-01");
    /// ```
    pub fn set_start(&mut self, start: &str) -> Result<(), QueryContextError> {
        let timestamp = QueryContext::sanitize_date(start)
            .ok_or(QueryContextError::InvalidDate(start.to_string()))?;
        self.start = Some(timestamp);
        Ok(())
    }

    /// Populate a [`QueryContext`] with an end date
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_end("2023-01-01");
    /// ```
    pub fn set_end(&mut self, end: &str) -> Result<(), QueryContextError> {
        let timestamp = QueryContext::sanitize_date(end)
            .ok_or(QueryContextError::InvalidDate(end.to_string()))?;
        self.end = Some(timestamp);
        Ok(())
    }

    /// Populate a [`QueryContext`] with a list of handle IDs to select
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

    /// Populate a [`QueryContext`] with a list of chat IDs to select
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

    /// Ensure a date string is valid
    fn sanitize_date(date: &str) -> Option<i64> {
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

        let local = Local.with_ymd_and_hms(year, month, day, 0, 0, 0).single()?;
        let stamp = local.timestamp_nanos_opt().unwrap_or(0);

        Some(stamp - (get_offset() * TIMESTAMP_FACTOR))
    }

    /// Determine if the current `QueryContext` has any filters present
    ///
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// assert!(!context.has_filters());
    /// context.set_start("2023-01-01");
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
        dates::{TIMESTAMP_FACTOR, format, get_offset},
        query_context::QueryContext,
    };

    #[test]
    fn can_create() {
        let context = QueryContext::default();
        assert!(context.start.is_none());
        assert!(context.end.is_none());
        assert!(!context.has_filters());
    }

    #[test]
    fn can_create_start() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();

        let from_timestamp = DateTime::from_timestamp(
            (context.start.unwrap() / TIMESTAMP_FACTOR) + get_offset(),
            0,
        )
        .unwrap()
        .naive_utc();
        let local = Local.from_utc_datetime(&from_timestamp);

        assert_eq!(format(&Ok(local)), "Jan 01, 2020 12:00:00 AM");
        assert!(context.start.is_some());
        assert!(context.end.is_none());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_end() {
        let mut context = QueryContext::default();
        context.set_end("2020-01-01").unwrap();

        let from_timestamp =
            DateTime::from_timestamp((context.end.unwrap() / TIMESTAMP_FACTOR) + get_offset(), 0)
                .unwrap()
                .naive_utc();
        let local = Local.from_utc_datetime(&from_timestamp);

        assert_eq!(format(&Ok(local)), "Jan 01, 2020 12:00:00 AM");
        assert!(context.start.is_none());
        assert!(context.end.is_some());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_both() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2020-02-02").unwrap();

        let from_timestamp = DateTime::from_timestamp(
            (context.start.unwrap() / TIMESTAMP_FACTOR) + get_offset(),
            0,
        )
        .unwrap()
        .naive_utc();
        let local_start = Local.from_utc_datetime(&from_timestamp);

        let from_timestamp =
            DateTime::from_timestamp((context.end.unwrap() / TIMESTAMP_FACTOR) + get_offset(), 0)
                .unwrap()
                .naive_utc();
        let local_end = Local.from_utc_datetime(&from_timestamp);

        assert_eq!(format(&Ok(local_start)), "Jan 01, 2020 12:00:00 AM");
        assert_eq!(format(&Ok(local_end)), "Feb 02, 2020 12:00:00 AM");
        assert!(context.start.is_some());
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
    fn test_can_set_selected_chat_ids_empty() {
        let mut qc = QueryContext::default();
        qc.set_selected_chat_ids(BTreeSet::new());

        assert_eq!(qc.selected_chat_ids, None);
        assert!(!qc.has_filters());
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
    fn test_can_set_selected_handle_ids_empty() {
        let mut qc = QueryContext::default();
        qc.set_selected_handle_ids(BTreeSet::new());

        assert_eq!(qc.selected_handle_ids, None);
        assert!(!qc.has_filters());
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
    use crate::util::query_context::QueryContext;

    #[test]
    fn can_sanitize_good() {
        let res = QueryContext::sanitize_date("2020-01-01");
        assert!(res.is_some());
    }

    #[test]
    fn can_reject_bad_short() {
        let res = QueryContext::sanitize_date("1-1-20");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_order() {
        let res = QueryContext::sanitize_date("01-01-2020");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_month() {
        let res = QueryContext::sanitize_date("2020-31-01");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_day() {
        let res = QueryContext::sanitize_date("2020-01-32");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_bad_data() {
        let res = QueryContext::sanitize_date("2020-AB-CD");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_wrong_hyphen() {
        let res = QueryContext::sanitize_date("2020–01–01");
        assert!(res.is_none());
    }
}
