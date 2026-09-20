#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::{Connection, types::Value};

    use crate::{
        tables::{
            capabilities::Capabilities,
            messages::{Message, models::BubbleComponent},
            table::Table,
        },
        test_support::schema_db,
        util::query_context::QueryContext,
    };

    fn body_db() -> Connection {
        let db = schema_db(true, true, true);
        db.execute_batch(
            "ALTER TABLE message ADD COLUMN attributedBody BLOB;
             ALTER TABLE message ADD COLUMN message_summary_info BLOB;
             ALTER TABLE message ADD COLUMN payload_data BLOB;",
        )
        .unwrap();
        db
    }

    fn insert_body(db: &Connection, id: i32, body: Value, text: Option<&str>) {
        db.execute(
            "INSERT INTO message (rowid, guid, date, is_from_me, attributedBody, text)
             VALUES (?1, ?2, ?1, 1, ?3, ?4)",
            rusqlite::params![id, id.to_string(), body, text],
        )
        .unwrap();
    }

    fn messages(db: &Connection) -> Vec<Message> {
        let capabilities = Capabilities::determine(db).unwrap();
        let context = QueryContext::default();
        let mut statement = Message::stream_rows(db, &capabilities, &context).unwrap();
        Message::rows(&mut statement, [])
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn rows_preserve_null_numeric_text_and_malformed_body_values() {
        let db = body_db();
        for (idx, body) in [
            Value::Null,
            Value::Integer(123),
            Value::Real(1.5),
            Value::Text("invalid body".to_string()),
            Value::Text(String::new()),
            Value::Blob(vec![]),
            Value::Blob(vec![0, 1, 2]),
        ]
        .into_iter()
        .enumerate()
        {
            insert_body(&db, idx as i32 * 2, body.clone(), None);
            insert_body(&db, idx as i32 * 2 + 1, body, Some("plain text fallback"));
        }

        for message in messages(&db) {
            if message.rowid % 2 == 1 {
                assert_eq!(message.text.as_deref(), Some("plain text fallback"));
                assert!(!message.components.is_empty());
            } else {
                assert!(message.text.is_none());
                assert!(message.components.is_empty());
            }
            assert_eq!(message.guid, message.rowid.to_string());
            assert!(message.is_from_me);
        }
    }

    #[test]
    fn projected_inputs_preserve_utf16_storage_bytes() {
        for (encoding, encoded_text) in [
            ("UTF-16le", [0x63, 0, 0x61, 0, 0x66, 0, 0xe9, 0]),
            ("UTF-16be", [0, 0x63, 0, 0x61, 0, 0x66, 0, 0xe9]),
        ] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch(&format!(
                "PRAGMA encoding = '{encoding}';
                 CREATE TABLE message (
                     rowid INTEGER PRIMARY KEY, guid TEXT, date INTEGER,
                     is_from_me INTEGER, attributedBody BLOB, text TEXT,
                     message_summary_info BLOB
                 );
                 CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
                 CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);"
            ))
            .unwrap();
            insert_body(&db, 1, Value::Text("café".to_string()), None);
            insert_body(&db, 2, Value::Blob(vec![0, 128, 255]), None);
            insert_body(&db, 3, Value::Integer(123), None);
            db.execute_batch("UPDATE message SET message_summary_info = attributedBody")
                .unwrap();
            let capabilities = Capabilities::determine(&db).unwrap();
            let context = QueryContext::default();
            let mut statement = Message::stream_rows(&db, &capabilities, &context).unwrap();
            let projected = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i32>("rowid")?,
                        row.get::<_, Vec<u8>>("attributedBody").ok(),
                        row.get::<_, Vec<u8>>("message_summary_info").ok(),
                    ))
                })
                .unwrap();
            for row in projected {
                let (id, body, summary) = row.unwrap();
                let expected = match id {
                    1 => Some(encoded_text.as_slice()),
                    2 => Some([0, 128, 255].as_slice()),
                    _ => None,
                };
                assert_eq!(body.as_deref(), expected);
                assert_eq!(summary.as_deref(), expected);
            }
        }
    }

    #[test]
    fn rows_preserve_edits_with_missing_or_malformed_bodies() {
        for (fixture, fully_unsent) in [("EditedAndUnsent", false), ("Deleted", true)] {
            let db = body_db();
            let summary = fs::read(format!("test_data/edited_message/{fixture}.plist")).unwrap();
            for (id, body, text) in [
                (1, Value::Null, None),
                (2, Value::Blob(vec![0, 1, 2]), None),
                (3, Value::Blob(vec![0, 1, 2]), Some("plain text fallback")),
            ] {
                insert_body(&db, id, body, text);
                db.execute(
                    "UPDATE message SET date_edited = 1, message_summary_info = ?1 WHERE rowid = ?2",
                    rusqlite::params![summary, id],
                )
                .unwrap();
            }
            let mut rows = messages(&db);
            let mut statement = db
                .prepare("SELECT m.*, 0 as num_attachments, 0 as num_replies FROM message m")
                .unwrap();
            rows.extend(
                statement
                    .query_map([], Message::from_row)
                    .unwrap()
                    .map(Result::unwrap),
            );
            assert_eq!(rows.len(), 6);
            for message in rows {
                assert_eq!(
                    message.text.as_deref(),
                    (message.rowid == 3).then_some("plain text fallback")
                );
                assert_eq!(
                    message.edited_parts.as_ref().unwrap().parts.len(),
                    if fully_unsent { 1 } else { 4 }
                );
                assert_eq!(message.is_fully_unsent(), fully_unsent);
                assert_eq!(message.is_part_edited(2), !fully_unsent);
                if message.rowid != 1 {
                    assert_eq!(message.components, [BubbleComponent::Retracted]);
                }
            }
        }
    }

    #[test]
    fn rows_preserve_url_payload_non_nullness() {
        let db = body_db();
        let bytes = fs::read("test_data/typedstream/SingleLink").unwrap();
        for (idx, payload) in [
            Value::Null,
            Value::Blob(vec![]),
            Value::Integer(123),
            Value::Text(String::new()),
        ]
        .into_iter()
        .enumerate()
        {
            let id = idx as i32;
            insert_body(&db, id, Value::Blob(bytes.clone()), None);
            db.execute(
                "UPDATE message SET payload_data = ?1 WHERE rowid = ?2",
                rusqlite::params![payload, id],
            )
            .unwrap();
        }
        let mut rows = messages(&db);
        let mut statement = db
            .prepare("SELECT m.*, 0 as num_attachments, 0 as num_replies FROM message m")
            .unwrap();
        rows.extend(
            statement
                .query_map([], Message::from_row)
                .unwrap()
                .map(Result::unwrap),
        );
        for message in rows {
            assert_eq!(
                message.balloon_bundle_id.as_deref(),
                (message.rowid != 0).then_some("com.apple.messages.URLBalloonProvider")
            );
            if message.rowid != 0 {
                assert_eq!(message.components, [BubbleComponent::App]);
            }
        }
    }
}
