/*!
 Placeholder for currently unsupported handwritten iMessages
*/

/// Placeholder for currently unsupported [handwritten](https://support.apple.com/en-us/HT206894) iMessages
pub struct HandwrittenMessage {}

impl HandwrittenMessage {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for HandwrittenMessage {
    fn default() -> Self {
        Self::new()
    }
}
