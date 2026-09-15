/*!
 Data structures and models used to parse and represent message data.
*/

pub use message::{Message, ParsedBody};

pub(crate) mod body;
pub(crate) mod columns;
pub mod message;
pub mod models;
pub(crate) mod query_parts;
mod tests;
