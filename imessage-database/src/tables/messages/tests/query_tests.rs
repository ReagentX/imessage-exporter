#[cfg(test)]
mod exclude_recoverable_tests {
    use std::collections::BTreeSet;

    use crate::{tables::messages::Message, util::query_context::QueryContext};

    #[test]
    fn can_generate_filter_statement_empty() {
        let context = QueryContext::default();

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_generate_filter_statement_start() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        let start_ns = context.start.unwrap();

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, format!("WHERE  m.date >= {start_ns}"));
    }

    #[test]
    fn can_generate_filter_statement_end() {
        let mut context = QueryContext::default();
        context.set_end("2020-01-01").unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, format!("WHERE  m.date <= {end_ns}"));
    }

    #[test]
    fn can_generate_filter_statement_start_end() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2020-02-02").unwrap();
        let start_ns = context.start.unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(
            statement,
            format!("WHERE  m.date >= {start_ns} AND  m.date <= {end_ns}")
        );
    }

    #[test]
    fn can_generate_filter_statement_chat_ids() {
        let mut context = QueryContext::default();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, "WHERE  c.chat_id IN (1, 2, 3)");
    }

    #[test]
    fn can_generate_filter_statement_start_end_chat_ids() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2020-02-02").unwrap();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        let start_ns = context.start.unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(
            statement,
            format!(
                "WHERE  m.date >= {start_ns} AND  m.date <= {end_ns} AND  c.chat_id IN (1, 2, 3)"
            )
        );
    }

    #[test]
    fn can_create_invalid_start() {
        let mut context = QueryContext::default();
        assert!(context.set_start("2020-13-32").is_err());
        assert!(!context.has_filters());

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_create_invalid_end() {
        let mut context = QueryContext::default();
        assert!(context.set_end("fake").is_err());
        assert!(!context.has_filters());

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_generate_filter_statement_with_empty_chat_ids() {
        let mut context = QueryContext::default();
        context.set_selected_chat_ids(BTreeSet::new());

        let statement = Message::generate_filter_statement(&context, false);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_generate_filter_statement_boundary_dates() {
        let mut context = QueryContext::default();
        context.set_start("1800-01-01").unwrap();
        context.set_end("2200-01-01").unwrap();

        let statement = Message::generate_filter_statement(&context, false);
        assert!(statement.contains("m.date >= "));
        assert!(statement.contains("m.date <= "));
    }
}

#[cfg(test)]
mod include_recoverable_tests {
    use std::collections::BTreeSet;

    use crate::{tables::messages::Message, util::query_context::QueryContext};

    #[test]
    fn can_generate_filter_statement_empty() {
        let context = QueryContext::default();

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_generate_filter_statement_start() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        let start_ns = context.start.unwrap();

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(statement, format!("WHERE  m.date >= {start_ns}"));
    }

    #[test]
    fn can_generate_filter_statement_end() {
        let mut context = QueryContext::default();
        context.set_end("2020-01-01").unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(statement, format!("WHERE  m.date <= {end_ns}"));
    }

    #[test]
    fn can_generate_filter_statement_start_end() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2020-02-02").unwrap();
        let start_ns = context.start.unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(
            statement,
            format!("WHERE  m.date >= {start_ns} AND  m.date <= {end_ns}")
        );
    }

    #[test]
    fn can_generate_filter_statement_chat_ids() {
        let mut context = QueryContext::default();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(
            statement,
            "WHERE  (c.chat_id IN (1, 2, 3) OR d.chat_id IN (1, 2, 3))"
        );
    }

    #[test]
    fn can_generate_filter_statement_start_end_chat_ids() {
        let mut context = QueryContext::default();
        context.set_start("2020-01-01").unwrap();
        context.set_end("2020-02-02").unwrap();
        context.set_selected_chat_ids(BTreeSet::from([1, 2, 3]));
        let start_ns = context.start.unwrap();
        let end_ns = context.end.unwrap();

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(
            statement,
            format!(
                "WHERE  m.date >= {start_ns} AND  m.date <= {end_ns} AND  (c.chat_id IN (1, 2, 3) OR d.chat_id IN (1, 2, 3))"
            )
        );
    }

    #[test]
    fn can_create_invalid_start() {
        let mut context = QueryContext::default();
        assert!(context.set_start("2020-13-32").is_err());
        assert!(!context.has_filters());

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(statement, "");
    }

    #[test]
    fn can_create_invalid_end() {
        let mut context = QueryContext::default();
        assert!(context.set_end("fake").is_err());
        assert!(!context.has_filters());

        let statement = Message::generate_filter_statement(&context, true);
        assert_eq!(statement, "");
    }
}

#[cfg(test)]
mod guid_query_tests {
    use std::env::current_dir;

    use crate::tables::{capabilities::Capabilities, messages::Message, table::get_connection};

    #[test]
    fn test_cant_query_bad_guid() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let conn = get_connection(&db_path).unwrap();

        let capabilities = Capabilities::determine(&conn).unwrap();

        let message = Message::from_guid("fake-guid", &conn, &capabilities);

        assert!(message.is_err());
    }

    #[test]
    fn test_can_query_guid() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let conn = get_connection(&db_path).unwrap();

        let capabilities = Capabilities::determine(&conn).unwrap();

        let mut message =
            Message::from_guid("0355C6E1-D0C8-4212-AA87-DD8AE4FD1203", &conn, &capabilities)
                .unwrap();

        let body = message.parse_body(&conn).unwrap();
        message.apply_body(body);

        println!("{message:#?}");
        assert!(!message.components.is_empty());
    }

    #[test]
    fn test_empty_guid() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let conn = get_connection(&db_path).unwrap();

        let capabilities = Capabilities::determine(&conn).unwrap();

        let message = Message::from_guid("", &conn, &capabilities);
        assert!(message.is_err());
    }

    #[test]
    fn test_malformed_guid() {
        let db_path = current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let conn = get_connection(&db_path).unwrap();

        let capabilities = Capabilities::determine(&conn).unwrap();

        let message = Message::from_guid("not-a-valid-guid-format", &conn, &capabilities);
        assert!(message.is_err());
    }
}
