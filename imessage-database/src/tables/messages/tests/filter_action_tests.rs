#[cfg(test)]
mod filter_action_mapping_tests {
    use crate::tables::messages::{Message, models::FilterAction};

    #[test]
    fn can_map_every_documented_code() {
        let cases = [
            (0, FilterAction::Unfiltered),
            (1, FilterAction::Allow),
            (2, FilterAction::Junk),
            (3, FilterAction::Promotion),
            (4, FilterAction::Transaction),
        ];

        for (code, expected) in cases {
            assert_eq!(FilterAction::from_code(Some(code)), Some(expected));
        }
    }

    #[test]
    fn can_map_unknown_code() {
        assert_eq!(
            FilterAction::from_code(Some(5)),
            Some(FilterAction::Unknown(5))
        );
    }

    #[test]
    fn absent_column_is_not_unfiltered() {
        assert_eq!(FilterAction::from_code(None), None);
    }

    #[test]
    fn can_identify_filtered_categories() {
        assert!(FilterAction::Junk.is_filtered());
        assert!(FilterAction::Promotion.is_filtered());
        assert!(FilterAction::Transaction.is_filtered());

        assert!(!FilterAction::Unfiltered.is_filtered());
        assert!(!FilterAction::Allow.is_filtered());
        assert!(!FilterAction::Unknown(5).is_filtered());
    }

    #[test]
    fn can_display_filter_action() {
        assert_eq!(FilterAction::Junk.to_string(), "Junk");
        assert_eq!(FilterAction::Unfiltered.to_string(), "Unfiltered");
        assert_eq!(FilterAction::Unknown(5).to_string(), "Unknown (5)");
    }

    #[test]
    fn blank_message_has_no_filter_action() {
        assert_eq!(Message::blank().filter_action(), None);
    }
}

#[cfg(test)]
mod filter_action_query_tests {
    use rusqlite::Connection;

    use crate::tables::messages::{Message, models::FilterAction};

    /// Build the minimum schema consumed by the message query. `macos_27`
    /// controls whether the filter columns are present and which query can prepare.
    fn schema_db(macos_27: bool) -> Connection {
        let filter_columns = if macos_27 {
            ", filter_action INTEGER DEFAULT 0, filter_sub_action INTEGER DEFAULT 0"
        } else {
            ""
        };

        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "
            CREATE TABLE message (
                ROWID INTEGER PRIMARY KEY,
                guid TEXT,
                text TEXT,
                service TEXT,
                handle_id INTEGER,
                destination_caller_id TEXT,
                subject TEXT,
                date INTEGER,
                date_read INTEGER,
                date_delivered INTEGER,
                is_from_me INTEGER,
                is_read INTEGER,
                item_type INTEGER,
                other_handle INTEGER,
                share_status INTEGER,
                share_direction INTEGER,
                group_title TEXT,
                group_action_type INTEGER,
                associated_message_guid TEXT,
                associated_message_type INTEGER,
                balloon_bundle_id TEXT,
                expressive_send_style_id TEXT,
                thread_originator_guid TEXT,
                thread_originator_part TEXT,
                date_edited INTEGER,
                associated_message_emoji TEXT{filter_columns}
            );
            CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
            CREATE TABLE message_attachment_join (attachment_id INTEGER, message_id INTEGER);
            CREATE TABLE chat_recoverable_message_join (chat_id INTEGER, message_id INTEGER, delete_date INTEGER);
            "
        ))
        .unwrap();
        db
    }

    fn insert(db: &Connection, guid: &str, filter_action: Option<i32>) {
        match filter_action {
            Some(action) => db.execute(
                "INSERT INTO message (guid, date, is_from_me, filter_action, filter_sub_action) VALUES (?1, 0, 0, ?2, 0)",
                rusqlite::params![guid, action],
            ),
            None => db.execute(
                "INSERT INTO message (guid, date, is_from_me) VALUES (?1, 0, 0)",
                rusqlite::params![guid],
            ),
        }
        .unwrap();
    }

    #[test]
    fn can_read_filter_action_from_macos_27_schema() {
        let db = schema_db(true);
        insert(&db, "junk", Some(2));

        let message = Message::from_guid("junk", &db).unwrap();

        assert_eq!(message.filter_action, Some(2));
        assert_eq!(message.filter_action(), Some(FilterAction::Junk));
    }

    #[test]
    fn can_read_every_category_from_macos_27_schema() {
        let db = schema_db(true);
        let cases = [
            ("unfiltered", 0, FilterAction::Unfiltered),
            ("allow", 1, FilterAction::Allow),
            ("junk", 2, FilterAction::Junk),
            ("promotion", 3, FilterAction::Promotion),
            ("transaction", 4, FilterAction::Transaction),
        ];

        for (guid, code, _) in cases {
            insert(&db, guid, Some(code));
        }

        for (guid, _, expected) in cases {
            let message = Message::from_guid(guid, &db).unwrap();
            assert_eq!(message.filter_action(), Some(expected), "guid: {guid}");
        }
    }

    #[test]
    fn older_schema_reports_no_filter_action() {
        let db = schema_db(false);
        insert(&db, "old", None);

        let message = Message::from_guid("old", &db).unwrap();

        // The compatible query head pads both missing filter columns with `NULL`.
        // `None` remains distinct from `Unfiltered`.
        assert_eq!(message.filter_action, None);
        assert_eq!(message.filter_action(), None);
    }

    #[test]
    fn older_schema_still_reads_the_rest_of_the_row() {
        let db = schema_db(false);
        insert(&db, "old", None);

        let message = Message::from_guid("old", &db).unwrap();

        assert_eq!(message.guid, "old");
        assert_eq!(message.date, 0);
        assert!(!message.is_from_me);
    }

    #[test]
    fn filter_sub_action_is_read_raw() {
        let db = schema_db(true);
        db.execute(
            "INSERT INTO message (guid, date, is_from_me, filter_action, filter_sub_action) VALUES ('sub', 0, 0, 4, 2)",
            [],
        )
        .unwrap();

        let message = Message::from_guid("sub", &db).unwrap();

        assert_eq!(message.filter_action(), Some(FilterAction::Transaction));
        assert_eq!(message.filter_sub_action, Some(2));
    }
}
