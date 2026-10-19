/*!
[Digital Touch](https://support.apple.com/guide/ipod-touch/send-a-digital-touch-effect-iph3fadba219/ios) messages are animated sketches, taps, fireballs, kisses, and heartbeats, as well as still photos with optional drawing overlays and videos.

Parse a `payload_data` blob with [`DigitalTouchMessage::from_payload`], then
render it with [`render_svg`](DigitalTouchMessage::render_svg) or
[`render_text`](DigitalTouchMessage::render_text).
*/

pub use crate::message_types::digital_touch::models::{
    Color, DigitalTouchMessage, ImageBackdrop, Point,
};

pub(crate) mod digital_touch_proto;
pub mod fireball;
pub mod heartbeat;
pub mod kiss;
pub mod media;
mod models;
pub mod sketch;
mod svg;
pub mod tap;
