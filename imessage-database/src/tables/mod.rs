/*!
Representations of iMessage database tables as structs.

Many of these tables do not include all available columns. Even on the same versions
of macOS, the schema of the iMessage database can vary.
*/

pub mod attachment;
pub mod chat;
pub mod chat_handle;
pub mod handle;
pub mod messages;
pub mod table;
