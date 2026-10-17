# imessage-database

This library provides interfaces to interact with iMessage Databases.

## Installation

This library is available on [crates.io](https://crates.io/crates/imessage-database).

## Documentation

Documentation is available on [docs.rs](https://docs.rs/imessage-database/).

## Example

```rust,no_run
use imessage_database::{
    error::table::TableError,
    tables::{capabilities::Capabilities, messages::Message, table::get_connection},
    util::{dirs::default_db_path, query_context::QueryContext},
};

fn iter_messages() -> Result<(), TableError> {
    // Create a read-only connection to an iMessage database
    let db = get_connection(&default_db_path())?;

    // Probe the schema and use the result to compose every message query
    let capabilities = Capabilities::determine(&db)?;
    let context = QueryContext::default();

    // Prepare the message query and iterate over its rows
    let mut statement = Message::stream_rows(&db, &capabilities, &context)?;
    for message_result in Message::rows(&mut statement, [])? {
        match message_result {
            Ok(message) => {
                println!("Text: {:?}", message.text);
                println!("Body components: {:#?}", message.components);
                println!("Edit history: {:#?}", message.edited_parts);
            }
            Err(e) => eprintln!("Error: {:?}", e),
        }
    }

    Ok(())
}
```
