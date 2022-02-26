mod app;
use app::{
    options::{from_command_line, OPTION_COPY, OPTION_DIAGNOSTIC, OPTION_PATH},
    runtime::State,
};
use imessage_database::util::dirs::default_db_path;

fn main() {
    // Get options from command line
    let options = from_command_line();
    let user_path = options.value_of(OPTION_PATH);
    let no_copy = options.is_present(OPTION_COPY);
    let diag = options.is_present(OPTION_DIAGNOSTIC);

    // Get the local database connection string
    let default = default_db_path();
    let db_path = user_path.unwrap_or(&default);

    // Create app state and runtime
    let app = State::new(db_path.to_owned(), no_copy).unwrap();

    if diag {
        app.run_diagnostic();
    } else {
        // Run some app methods
        // app.iter_threads();
        app.iter_messages();
        // app.iter_attachments();

        // Theoretically: start app
        app.start();
    }
}
