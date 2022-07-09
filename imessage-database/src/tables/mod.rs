/*!
Representations of iMessage database tables as structs.

Many of these tables do not include all availalbe columns. Even on the same versions
of MacOS, the schema of the iMessage database can vary. Out of an abundance of caution,
this library only attempts to get columns that are safe. This also serves to preserve
backwards compatibility with older versions of MacOS.
*/

pub mod attachment;
pub mod chat;
pub mod chat_handle;
pub mod handle;
pub mod messages;
pub mod table;
