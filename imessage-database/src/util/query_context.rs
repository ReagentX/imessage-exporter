/*!
 Contains logic for handling query filter configurations.
*/
use chrono::prelude::*;

use crate::{
    error::query_context::QueryContextError,
    util::dates::{get_offset, TIMESTAMP_FACTOR},
};

#[derive(Debug, Default, PartialEq, Eq)]
/// Represents filter configurations for a SQL query.
pub struct QueryContext {
    /// The start date filter. Only messages sent on or after this date will be included.
    pub start: Option<i64>,
    /// The end date filter. Only messages sent before this date will be included.
    pub end: Option<i64>,
    /// Telephone filter. Only messages including this number will be included.
    pub telephone: Option<String>,
}

impl QueryContext {
    /// Generate a `QueryContext` with a start date
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

    /// Generate a `QueryContext` with an end date
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

    /// Generate a `QueryContext` with a telephone number.
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_telephone("16468885555");
    /// ```
    pub fn set_telephone(&mut self, telephone: &str) -> Result<(), QueryContextError> {
        let tele = QueryContext::sanitize_telephone(telephone)
            .ok_or(QueryContextError::InvalidTelephone(telephone.to_string()))?;

        let mut number = String::from(tele);
        number.insert_str(0, "+");
        self.telephone = Some(number);
        Ok(())
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

    /// Ensure a telephone string is valid
    fn sanitize_telephone(telephone: &str) -> Option<&str> {
        if telephone.len() < 11 {
            return None;
        }
        Some(telephone)
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
    pub fn has_filters(&self) -> bool {
        [self.start, self.end].iter().any(Option::is_some)
    }

    /// Generate the SQL `WHERE` clause described by this `QueryContext`
    /// # Example:
    ///
    /// ```
    /// use imessage_database::util::query_context::QueryContext;
    ///
    /// let mut context = QueryContext::default();
    /// context.set_start("2023-01-01");
    /// let filters = context.generate_filter_statement("field_name");
    /// ```
    pub fn generate_filter_statement(&self, field: &str) -> String {
        let mut filters = String::new();
        if let Some(start) = self.start {
            filters.push_str(&format!("    {field} >= {start}"));
        }
        if let Some(end) = self.end {
            if !filters.is_empty() {
                filters.push_str(" AND ");
            }
            filters.push_str(&format!("    {field} <= {end}"));
        }

        if !filters.is_empty() {
            return format!(
                " WHERE
                 {filters}"
            );
        }
        filters
    }
}

#[cfg(test)]
mod use_tests {
    use std::env::set_var;

    use chrono::prelude::*;

    use crate::util::{
        dates::{format, get_offset, TIMESTAMP_FACTOR},
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
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

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
        assert_eq!(
            context.generate_filter_statement("m.date"),
            " WHERE\n                     m.date >= 599558400000000000"
        );
        assert!(context.start.is_some());
        assert!(context.end.is_none());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_end() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

        let mut context = QueryContext::default();
        context.set_end("2020-01-01").unwrap();

        let from_timestamp =
            DateTime::from_timestamp((context.end.unwrap() / TIMESTAMP_FACTOR) + get_offset(), 0)
                .unwrap()
                .naive_utc();
        let local = Local.from_utc_datetime(&from_timestamp);

        assert_eq!(format(&Ok(local)), "Jan 01, 2020 12:00:00 AM");
        assert_eq!(
            context.generate_filter_statement("m.date"),
            " WHERE\n                     m.date <= 599558400000000000"
        );
        assert!(context.start.is_none());
        assert!(context.end.is_some());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_both() {
        // Set timezone to PST for consistent Local time
        set_var("TZ", "PST");

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
        assert_eq!(
            context.generate_filter_statement("m.date"),
            " WHERE\n                     m.date >= 599558400000000000 AND     m.date <= 602323200000000000"
        );
        assert!(context.start.is_some());
        assert!(context.end.is_some());
        assert!(context.has_filters());
    }

    #[test]
    fn can_create_invalid_start() {
        let mut context = QueryContext::default();
        assert!(context.set_start("2020-13-32").is_err());
        assert!(!context.has_filters());
        assert_eq!(context.generate_filter_statement("m.date"), "");
    }

    #[test]
    fn can_create_invalid_end() {
        let mut context = QueryContext::default();
        assert!(context.set_end("fake").is_err());
        assert!(!context.has_filters());
        assert_eq!(context.generate_filter_statement("m.date"), "");
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

    #[test]
    fn can_sanitize_good_telephone() {
        let res = QueryContext::sanitize_telephone("16468885555");
        assert!(res.is_some());
    }

    #[test]
    fn can_reject_bad_short_telephone() {
        let res = QueryContext::sanitize_telephone("1646888555");
        assert!(res.is_none());
    }

    #[test]
    fn can_reject_typo_telephone() {
        let res = QueryContext::sanitize_telephone("1646-8885555");
        assert!(res.is_none());
    }
}
