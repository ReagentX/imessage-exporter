/*!
 Contains functions that emit a loading message while we do other work.
*/

use std::io::{Write, stdout};

/// Write to the CLI while something is working so that we can overwrite it later
///
/// # Example:
///
/// ```
/// use imessage_database::util::output::processing;
///
/// processing();
/// println!("Done working!");
/// ```
pub fn processing() {
    print!("\rProcessing...");
    stdout().flush().unwrap_or_default();
}

/// Overwrite the CLI when something is done working so that we can write cleanly later
///
/// # Example:
///
/// ```
/// use imessage_database::util::output::{processing, done_processing};
///
/// processing();
/// done_processing();
/// ```
pub fn done_processing() {
    print!("\r");
    stdout().flush().unwrap_or_default();
}
