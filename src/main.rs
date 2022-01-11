mod tables;
mod util;

use util::options::from_command_line;

fn main() {
    let options = from_command_line();
    let user_path = options.value_of("db-path");
    let no_copy = options.is_present("no-copy");

    let db_path = user_path.unwrap_or("/Users/chris/Library/Messages/chat.db");

    // Create app state and runtime
    let app = util::runtime::State::new(db_path.to_owned(), no_copy).unwrap();
    app.iter_threads();
    app.iter_messages();
}
