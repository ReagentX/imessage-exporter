use std::io::{stdout, Write};


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
    stdout().flush().unwrap();
}
