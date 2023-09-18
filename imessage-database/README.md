# imessage-database

This library provides interfaces to interact with iMessage Databases.

## Installation

This library is available on [crates.io](https://crates.io/crates/imessage-database).

## Documentation

Documentation is available on [docs.rs](https://docs.rs/imessage-database/).

## Example

```rust
use imessage_database::{
    error::table::TableError,
    tables::{
        messages::Message,
        table::{get_connection, Table},
    },
    util::dirs::default_db_path,
};

fn iter_messages() -> Result<(), TableError> {
    /// Create a read-only connection to an iMessage database
    let db = get_connection(&default_db_path()).unwrap();

    /// Create SQL statement
    let mut statement = Message::get(&db)?;

    /// Execute statement
    let messages = statement
        .query_map([], |row| Ok(Message::from_row(row)))
        .unwrap();

    /// Iterate over each row
    for message in messages {
        let mut msg = Message::extract(message)?;

        /// Parse message body if it was sent from macOS 13.0 or newer
        msg.gen_text(&db);

        /// Emit debug info for each message
        println!("{:?}", msg)
    }

    Ok(())
}
```
