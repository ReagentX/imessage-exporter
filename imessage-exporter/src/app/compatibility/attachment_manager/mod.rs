/*!
 Defines routines for how attachments should be handled.
*/

mod manager;
mod mode;
mod plan;
mod target;

pub use manager::AttachmentManager;
pub use mode::AttachmentManagerMode;
pub(crate) use plan::{CopiedFile, Plan};
