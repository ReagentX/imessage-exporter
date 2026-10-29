/*!
 Data structures and models used to parse and represent message data.
*/

pub use message::Message;

pub(crate) mod body;
pub(crate) mod columns;
mod message;
pub mod models;
pub(crate) mod query_parts;
#[cfg(test)]
mod tests;
