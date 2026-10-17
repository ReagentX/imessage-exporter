# Custom message projections

Custom queries now determine which body inputs reach the decoder. A query that previously selected metadata and fetched the body later must include the body columns in its projection.

The six required result-column names remain unchanged from 4.3.0: `rowid`, `guid`, `date`, `is_from_me`, `num_attachments`, and `num_replies`. Column order does not matter. Missing required columns produce `TableError::QueryError`; other omitted fields take their defaults.

Select these inputs when the corresponding data is needed:

| Result column | Purpose |
| --- | --- |
| `text` | Plain-text fallback when the attributed body supplies no text |
| `attributedBody` | Encoded body text, formatting, and body components |
| `message_summary_info` | Edit and unsent history; also retain `date_edited`, because edit parsing requires the message to be marked edited |
| `balloon_bundle_id` | Stored app-message classification |
| `has_payload_data` | Payload presence used to infer URL previews when a single-link body has no stored balloon bundle ID |

Compute `has_payload_data` as `payload_data IS NOT NULL`. A custom `m.*` projection can supply raw `payload_data` instead. If both result columns exist, the flag takes precedence, including when it is false. Omitting both means the decoder treats the payload as absent.

SQLite transcodes text-valued columns when reading them through the normal text interface. For body and edit-summary inputs stored as text in a UTF-16 database, that changes the bytes sent to the parser. Cast those text values to `BLOB` to preserve their storage bytes. The built-in queries already do this.

For a schema containing the body, edit, payload, and reply columns below, a custom query can use this projection:

```sql
SELECT
    m.ROWID AS rowid,
    m.guid,
    m.date,
    m.is_from_me,
    m.text,
    m.date_edited,
    m.balloon_bundle_id,
    CASE WHEN typeof(m.attributedBody) = 'text'
        THEN CAST(m.attributedBody AS BLOB)
        ELSE m.attributedBody
    END AS attributedBody,
    CASE WHEN typeof(m.message_summary_info) = 'text'
        THEN CAST(m.message_summary_info AS BLOB)
        ELSE m.message_summary_info
    END AS message_summary_info,
    (m.payload_data IS NOT NULL) AS has_payload_data,
    (SELECT COUNT(*) FROM message_attachment_join a
        WHERE a.message_id = m.ROWID) AS num_attachments,
    (SELECT COUNT(*) FROM message r
        WHERE r.thread_originator_guid = m.guid) AS num_replies
FROM message m
ORDER BY m.date;
```

This query loads body inputs and relationship counts. Add other metadata columns your application reads. For older schemas, gate references to optional columns and tables on their availability; substituting `NULL` or zero is appropriate when the source schema cannot contain the corresponding data. `Message::stream_rows()` handles these schema differences for the built-in query.

For scans that only need metadata, pass `capabilities.without_body_inputs()` to query preparation. This omits `attributedBody`, `message_summary_info`, and payload presence from the projection, avoiding their reads and parsing. Selected `text` can still produce plain-text components. Fetch the message again with body inputs if you later need its rich body or edit history; the public API no longer provides a separate hydration step.

[Back to the migration guide](README.md).
