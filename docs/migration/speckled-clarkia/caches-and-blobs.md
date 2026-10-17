# Replace Cacheable and database helpers

The `Cacheable` trait is removed. Each concrete type now names the map it builds:

| 4.3.0 | 5.0 |
| --- | --- |
| `Chat::cache(&db)` | `Chat::by_rowid(&db)` |
| `ChatToHandle::cache(&db)` | `ChatToHandle::participants_by_chat(&db)` |
| `Handle::cache(&db)` | Load rows with `Handle::load(&db)`, then consume them with `Handle::ids_by_rowid(rows)` |
| `Message::cache(&db)` | `Message::tapbacks_by_target(&db, &capabilities)` |

Remove `Cacheable` imports and replace generic `T: Cacheable` helpers with the appropriate concrete operation. `Chat`, `Handle`, and `ChatToHandle` retain inherent `get()` methods. `Attachment::get()` is removed: prepare an attachment query yourself, or use `Attachment::from_message()` to load a message's attachments.

`Handle::ids_by_rowid()` preserves each handle's stored address and adds the synthetic local-user entry at row ID `0`. Unlike the old cache, it does not replace addresses with combined strings for handles sharing `person_centric_id`. Use a separate handle-to-group map for identity comparisons, as described in [Participant grouping and chat deduplication](participant-grouping.md).

Database blob helpers also move out of `Table`:

| Removed API | Replacement |
| --- | --- |
| `Table::get_blob()` | `tables::table::open_blob(db, table, column, rowid)` |
| `Table::has_blob()` | Select `column IS NOT NULL` in your query, or issue a separate presence query |
| `Message::attributed_body()` | Select `attributedBody` in a custom query, or read it through `open_blob()` |

`open_blob()` returns an optional read-only `rusqlite::blob::Blob`. If you need an owned byte buffer, read the blob into that buffer explicitly. To access the encoded body, pass table `message`, column `attributedBody`, and the message row ID. To read decoded content, use the message's body fields.

[Back to the migration guide](README.md).
