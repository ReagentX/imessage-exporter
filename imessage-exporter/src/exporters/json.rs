use std::{
    fs::File,
    io::BufWriter,
};

use imessage_database::tables::messages::Message;

use crate::app::{error::RuntimeError, runtime::Config};
use crate::exporters::exporter::Exporter;

pub struct JSON<'a> {
    pub config: &'a Config,
}

impl<'a> Exporter<'a> for JSON<'a> {
    fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(JSON { config })
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        todo!("JSON export not yet implemented — coming in Task 3")
    }

    fn get_or_create_file(
        &mut self,
        _message: &Message,
    ) -> Result<&mut BufWriter<File>, RuntimeError> {
        unreachable!("JSON exporter does not use per-message file handles")
    }
}
