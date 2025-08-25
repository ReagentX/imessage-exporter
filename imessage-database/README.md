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
    tables::{
        messages::Message,
        table::{get_connection, Table},
    },
    util::dirs::default_db_path,
};

fn iter_messages() -> Result<(), TableError> {
    // Create a read-only connection to an iMessage database
    let db = get_connection(&default_db_path()).unwrap();

    // Iterate over a stream of messages
    Message::stream(&db, |message_result| {
        match message_result {
            Ok(mut message) => {
                // Deserialize the message body
                message.generate_text(&db);

                // Emit debug info for each message
                println!("Message: {:#?}", message)
            },
            Err(e) => eprintln!("Error: {:?}", e),
        };

        // You can substitute your own closure error type
        Ok::<(), TableError>(())
    })?;

    Ok(())
}
```
