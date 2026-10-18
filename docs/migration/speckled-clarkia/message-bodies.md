# Message bodies decode with their rows

In 4.3.0, reading a `Message` and parsing its body were separate operations. A caller fetched the row, called `parse_body()` to read and decode additional database columns, then called `apply_body()` to copy the result into the message:

```rust,ignore
let mut message = Message::from_guid(guid, db, capabilities)?;
if let Ok(body) = message.parse_body(db) {
    message.apply_body(body);
}
```

In 5.0, row decoding populates `text`, `components`, `edited_parts`, and the resolved `balloon_bundle_id`. Read those fields directly:

```rust,no_run
use imessage_database::{
    error::table::TableError,
    tables::{capabilities::Capabilities, messages::Message},
};
use imessage_database::rusqlite::Connection;

fn inspect_message(
    db: &Connection,
    capabilities: &Capabilities,
    guid: &str,
) -> Result<(), TableError> {
    let message = Message::from_guid(guid, db, capabilities)?;
    println!("Text: {:?}", message.text);
    println!("Body components: {:#?}", message.components);
    println!("Edit history: {:#?}", message.edited_parts);
    Ok(())
}
```

Remove calls to `Message::parse_body()`, `Message::apply_body()`, and `Message::generate_text_legacy()`. Remove imports of `ParsedBody`, including imports through `tables::messages::message`. Body parsing still exists internally, but callers no longer invoke it separately. Only those methods produced `MessageError::NoText` and `MessageError::StreamTypedParseError`, so both variants are removed along with the `From<StreamTypedError>` conversion; remove any match arms for them.

Parsing is best effort: a failed text recovery preserves the row's metadata and any recovered components or edit history. Code that previously matched on the result of `parse_body()` must instead inspect the decoded fields. Row-fetch and required-column decoding errors still return `TableError`.

The decoder only uses the columns selected by the query. Built-in message queries include the available body inputs; custom queries must select them explicitly. The decoder does not fetch missing inputs afterward. See [Custom message projections](message-projections.md) before migrating a custom query.

Related data still has its own loading operations: attachments, replies, poll votes, and app payloads are fetched separately. Decoding a message's body does not load its entire conversation or attachment data.

[Back to the migration guide](README.md).
