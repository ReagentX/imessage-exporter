/*!
 Errors surfaced anywhere from CLI startup through per-message formatting.
*/

use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
    io::Error as IoError,
    path::PathBuf,
};

use crabapple::error::BackupError;
use imessage_database::{
    error::{message::MessageError, table::TableError},
    util::size::format_file_size,
};

use crate::app::options::OPTION_BYPASS_FREE_SPACE_CHECK;

/// Errors that can happen during the application's runtime
#[derive(Debug)]
pub enum RuntimeError {
    InvalidOptions(String),
    DiskError(IoError),
    DatabaseError(TableError),
    MessageError(MessageError),
    BackupError(BackupError),
    CallLogError(String),
    PdfError(String),
    NotEnoughAvailableSpace(u64, u64),
    FileNameError { path: PathBuf, reason: &'static str },
}

impl Display for RuntimeError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            RuntimeError::InvalidOptions(why) => write!(fmt, "Invalid options!\n{why}"),
            RuntimeError::DiskError(why) => write!(fmt, "{why}"),
            RuntimeError::DatabaseError(why) => write!(fmt, "{why}"),
            RuntimeError::MessageError(why) => write!(fmt, "{why}"),
            RuntimeError::CallLogError(why) => write!(fmt, "{why}"),
            RuntimeError::NotEnoughAvailableSpace(estimated_bytes, available_bytes) => {
                write!(
                    fmt,
                    "Not enough free disk space!\nEstimated export size: {}\nDisk space available: {}\nPass --{OPTION_BYPASS_FREE_SPACE_CHECK} to ignore\n",
                    format_file_size(*estimated_bytes),
                    format_file_size(*available_bytes),
                )
            }
            RuntimeError::BackupError(why) => write!(fmt, "{why}"),
            RuntimeError::FileNameError { path, reason } => {
                write!(fmt, "Invalid file name at {}: {reason}", path.display())
            }
            RuntimeError::PdfError(why) => write!(fmt, "{why}"),
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RuntimeError::DiskError(why) => Some(why),
            RuntimeError::DatabaseError(why) => Some(why),
            RuntimeError::MessageError(why) => Some(why),
            RuntimeError::BackupError(why) => Some(why),
            RuntimeError::InvalidOptions(_)
            | RuntimeError::CallLogError(_)
            | RuntimeError::PdfError(_)
            | RuntimeError::NotEnoughAvailableSpace(_, _)
            | RuntimeError::FileNameError { .. } => None,
        }
    }
}

impl From<TableError> for RuntimeError {
    fn from(err: TableError) -> Self {
        RuntimeError::DatabaseError(err)
    }
}

impl From<MessageError> for RuntimeError {
    fn from(err: MessageError) -> Self {
        RuntimeError::MessageError(err)
    }
}

impl From<BackupError> for RuntimeError {
    fn from(err: BackupError) -> Self {
        RuntimeError::BackupError(err)
    }
}

impl From<IoError> for RuntimeError {
    fn from(err: IoError) -> Self {
        RuntimeError::DiskError(err)
    }
}

impl From<rusqlite::Error> for RuntimeError {
    fn from(err: rusqlite::Error) -> Self {
        RuntimeError::DatabaseError(TableError::from(err))
    }
}
