use tables::attachment::Attachment;

mod tables;
mod util;

use {
    tables::table::{DEFAULT_PATH, OPTION_COPY, OPTION_PATH, OPTION_DIAGNOSTIC},
    util::options::from_command_line,
};

fn main() {
    // Get options from command line
    let options = from_command_line();
    let user_path = options.value_of(OPTION_PATH);
    let no_copy = options.is_present(OPTION_COPY);
    let diag = options.is_present(OPTION_DIAGNOSTIC);

    // Get the local database connection string
    let db_path = user_path.unwrap_or(DEFAULT_PATH);

    // Create app state and runtime
    let app = util::runtime::State::new(db_path.to_owned(), no_copy).unwrap();

    if diag {
        app.run_diagnostic();
    } else {
        // Run some app methods
        // app.iter_threads();
        // app.iter_messages();
        app.iter_attachments();
        // TODO: implement cache for attachment
    
        // Theoretically: start app
        app.start();
    }
}
