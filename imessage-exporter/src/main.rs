mod app;
use app::{
    options::{from_command_line, Options},
    runtime::State,
};

fn main() {
    // Get args from command line
    let args = from_command_line();
    // Create application options
    let options = Options::from_args(&args);

    // Create app state and start
    let app = State::new(options).unwrap();
    app.start()
}
