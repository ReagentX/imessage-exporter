use std::io::{stdout, Write};

pub fn processing() {
    print!("\rProcessing...");
    stdout().flush().unwrap();
}
