# Row iteration replaces callback streaming

`Message` no longer implements `Table`. Use its inherent `rows()` and `row()` methods with a prepared statement. Replace the old entry points as follows:

| 4.3.0 | 5.0 |
| --- | --- |
| `Message::stream(&db, callback)` | Prepare with `Message::stream_rows()`, then iterate with `Message::rows()` |
| `Message::get(&db)` | `Message::stream_rows(&db, &capabilities, &context)` |
| `stmt.query_map(params, Message::from_row)` | `Message::rows(&mut stmt, params)` |
| `stmt.query_row(params, Message::from_row)` | `Message::row(&mut stmt, params)` |

Probe the schema once, then pass the resulting `Capabilities` to query preparation. A default `QueryContext` requests an unfiltered message stream:

```rust,no_run
use imessage_database::{
    error::table::TableError,
    tables::{capabilities::Capabilities, messages::Message},
    util::query_context::QueryContext,
};
use rusqlite::Connection;

fn read_messages(db: &Connection) -> Result<(), TableError> {
    let capabilities = Capabilities::determine(db)?;
    let context = QueryContext::default();
    let mut statement = Message::stream_rows(db, &capabilities, &context)?;

    for message in Message::rows(&mut statement, [])? {
        let message = message?;
        println!("Text: {:?}", message.text);
        println!("Body components: {:#?}", message.components);
    }
    Ok(())
}
```

`stream_rows()` prepares the statement; `rows()` returns the iterator. Both cached and ordinary `rusqlite` statements work with `rows()` and `row()`. An existing call to `Message::rows()` or `Message::row()` keeps the same syntax, but no longer needs a `Table` import.

`Table` now provides only `from_row()`, `rows()`, and `row()`. Remove `get()` from custom `Table` implementations and put statement preparation on the concrete type or in the calling code. Generic functions that use `T::get()` or `T::stream()` must accept a prepared statement or move query preparation to their callers. Generic functions bounded by `T: Table` need a separate path for `Message`.

For other table types, keep `Table` in scope to use its row methods. For example, replace a callback-based handle scan with:

```rust,no_run
use imessage_database::{
    error::table::TableError,
    tables::{handle::Handle, table::Table},
};
use rusqlite::Connection;

fn read_handles(db: &Connection) -> Result<(), TableError> {
    let mut statement = Handle::get(db)?;
    for handle in Handle::rows(&mut statement, [])? {
        let handle = handle?;
        println!("Handle: {}", handle.id);
    }
    Ok(())
}
```

The loop now owns callback control flow. Use `break` for early termination, and propagate application errors from the loop through the enclosing function's return type. To continue after a row error, match each iterator item rather than applying `?` to it.

[Back to the migration guide](README.md).
