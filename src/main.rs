use rusqlite::{Connection, OpenFlags};

mod tables;
mod util;

use tables::{handle::Handle, table::Table};
use util::dates::format;

fn main() {
    let db_path = "/Users/chris/Library/Messages/chat.db";
    let db = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(res) => res,
        Err(why) => panic!("Unable to read from chat database: {}\nEnsure full disk access is enabled for your terminal emulator in System Preferences > Security and Privacy > Full Disk Access", why),
    };

    let mut statement = db
        .prepare("SELECT * from message ORDER BY date LIMIT 10")
        .unwrap();
    let messages = statement
        .query_map([], |row| Ok(tables::messages::Message::from_row(row)))
        .unwrap();
    for message in messages {
        let msg = message.unwrap().unwrap();
        println!("{:?}: {:?}", format(&msg.date()), msg.text);
    }

    let contacts = Handle::get_map(&db);
    println!("{:?}", contacts);
}
