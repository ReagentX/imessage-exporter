use std::env::var;

use crate::tables::table::DEFAULT_PATH;

pub fn home() -> String {
    match var("HOME") {
        Ok(path) => path,
        Err(why) => panic!("Unable to resolve user home directory: {why}"),
    }
}

pub fn default_db_path() -> String {
    let h = home();
    format!("{h}/{DEFAULT_PATH}")
}
