/*!
Parser for [Digital Touch](https://support.apple.com/guide/ipod-touch/send-a-digital-touch-effect-iph3fadba219/ios) iMessages.
This message type is not documented by Apple, but represents messages displayed as `com.apple.DigitalTouchBalloonProvider`.
*/

use crate::message_types::digital_touch::digital_touch_proto::{
    BaseMessage, TouchKind as DigitalTouch,
};

use protobuf::Message;

/// Converts a raw byte payload from the database into a [`DigitalTouch`].
#[must_use]
pub fn from_payload(payload: &[u8]) -> Option<DigitalTouch> {
    let msg = BaseMessage::parse_from_bytes(payload).ok()?;

    Some(msg.TouchKind.enum_value_or_default())
}

#[cfg(test)]
mod tests {
    use crate::message_types::digital_touch::{DigitalTouch, from_payload};

    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn can_parse_tap() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/tap.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Tap));
    }

    #[test]
    fn can_parse_heartbeat() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/heartbeat.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Heartbeat));
    }

    #[test]
    fn can_parse_heartbreak() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/heartbreak.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Heartbeat));
    }

    #[test]
    fn can_parse_sketch() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/sketch.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Sketch));
    }

    #[test]
    fn can_parse_kiss() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/kiss.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Kiss));
    }

    #[test]
    fn can_parse_fireball() {
        let protobuf_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/digital_touch_message/fireball.bin");
        let mut proto_data = File::open(protobuf_path).unwrap();
        let mut data = vec![];
        proto_data.read_to_end(&mut data).unwrap();

        let expected = from_payload(&data);
        assert_eq!(expected, Some(DigitalTouch::Fireball));
    }
}
