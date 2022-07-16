use imessage_database::{Attachment, Message};

use crate::{
    app::runtime::Config,
    exporters::{
        exporter::{Exporter, Writer},
        html::HTML,
    },
};

pub struct PDF<'a> {
    html_builder: HTML<'a>,
}

impl<'a> Exporter<'a> for PDF<'a> {
    fn new(config: &'a Config) -> Self {
        Self {
            html_builder: HTML::new(config),
        }
    }

    fn iter_messages(&mut self) {
        todo!()
    }

    fn get_or_create_file(&mut self, message: &Message) -> &std::path::Path {
        todo!()
    }
}

impl<'a> Writer<'a> for PDF<'a> {
    fn format_message(&self, msg: &Message, indent: usize) -> String {
        todo!()
    }

    fn format_attachment(&self, msg: &'a Attachment) -> Result<&'a str, &'a str> {
        todo!()
    }

    fn format_app(&self, msg: &'a Message) -> &'a str {
        todo!()
    }

    fn format_reaction(&self, msg: &Message) -> String {
        todo!()
    }

    fn format_expressive(&self, msg: &'a Message) -> &'a str {
        todo!()
    }

    fn format_annoucement(&self, msg: &'a Message) -> String {
        todo!()
    }

    fn write_to_file(file: &std::path::Path, text: &str) {
        todo!()
    }
}
