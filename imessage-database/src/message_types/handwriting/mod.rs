/*!
[Handwritten](https://support.apple.com/en-us/HT206894) messages are animated doodles or messages sent in your own handwriting.

A writeup about the reverse engineering of this data can be found [here](https://github.com/trymoose/handwriting2svg/blob/0eb56cf458207bb1c2ceea48cf4b6b6510fa7b13/DISCOVERY.md).
*/

pub use models::HandwrittenMessage;

pub(crate) mod handwriting_proto;
pub mod models;
