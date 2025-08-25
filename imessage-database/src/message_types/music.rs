/*!
 These are the link previews that iMessage generates when sending Apple Music links.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::plist::{get_string_from_dict, get_string_from_nested_dict, get_value_from_dict},
};

/// This struct is not documented by Apple, but represents messages displayed as
/// `com.apple.messages.URLBalloonProvider` but from the Music app
#[derive(Debug, PartialEq, Eq)]
pub struct MusicMessage<'a> {
    /// URL in Apple Music
    pub url: Option<&'a str>,
    /// URL pointing to the track preview stream
    pub preview: Option<&'a str>,
    /// Artist name
    pub artist: Option<&'a str>,
    /// Album name
    pub album: Option<&'a str>,
    /// Track name
    pub track_name: Option<&'a str>,
    /// Included lyrics, if any
    pub lyrics: Option<Vec<&'a str>>,
}

impl<'a> BalloonProvider<'a> for MusicMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        if let Ok((music_metadata, body)) = MusicMessage::get_body_and_url(payload) {
            // Ensure the message is a Music message
            if get_string_from_dict(music_metadata, "album").is_none() {
                return Err(PlistParseError::WrongMessageType);
            }

            return Ok(Self {
                url: get_string_from_nested_dict(body, "URL"),
                preview: get_string_from_nested_dict(music_metadata, "previewURL"),
                artist: get_string_from_dict(music_metadata, "artist"),
                album: get_string_from_dict(music_metadata, "album"),
                track_name: get_string_from_dict(music_metadata, "name"),
                lyrics: get_value_from_dict(music_metadata, "lyricExcerpt")
                    .and_then(|l| get_string_from_dict(l, "lyrics"))
                    .map(|lyrics| lyrics.split('\n').collect()),
            });
        }
        Err(PlistParseError::NoPayload)
    }
}

impl<'a> MusicMessage<'a> {
    /// Extract the main dictionary of data from the body of the payload
    ///
    /// Apple Music stores the URL under `richLinkMetadata` like a normal URL, but has some
    /// extra data stored under `specialization` that contains the track information.
    fn get_body_and_url(payload: &'a Value) -> Result<(&'a Value, &'a Value), PlistParseError> {
        let base = payload
            .as_dictionary()
            .ok_or_else(|| {
                PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
            })?
            .get("richLinkMetadata")
            .ok_or_else(|| PlistParseError::MissingKey("richLinkMetadata".to_string()))?;
        Ok((
            base.as_dictionary()
                .ok_or_else(|| {
                    PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
                })?
                .get("specialization")
                .ok_or_else(|| PlistParseError::MissingKey("specialization".to_string()))?,
            base,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{music::MusicMessage, variants::BalloonProvider},
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_apple_music() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/music_message/AppleMusic.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = MusicMessage::from_map(&parsed).unwrap();
        let expected = MusicMessage {
            url: Some(
                "https://music.apple.com/us/album/%D0%BF%D0%B5%D1%81%D0%BD%D1%8C-1/1539641998?i=1539641999",
            ),
            preview: Some(
                "https://audio-ssl.itunes.apple.com/itunes-assets/AudioPreview115/v4/b2/65/b3/b265b31f-facb-3ea3-e6bc-91a8d01c9b2f/mzaf_18233159060539450284.plus.aac.ep.m4a",
            ),
            artist: Some("БАТЮШКА"),
            album: Some("Панихида"),
            track_name: Some("Песнь 1"),
            lyrics: None,
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_apple_music_lyrics() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/music_message/AppleMusicLyrics.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        println!("{parsed:#?}");

        let balloon = MusicMessage::from_map(&parsed).unwrap();
        let expected = MusicMessage {
            url: Some(
                "https://music.apple.com/us/lyrics/1329891623?ts=11.108&te=16.031&l=en&tk=2.v1.VsuX9f%2BaT1PyrgMgIT7ANQ%3D%3D&itsct=sharing_msg_lyrics&itscg=50401",
            ),
            preview: None,
            artist: Some("Dual Core"),
            album: Some("Downtime"),
            track_name: Some("Another Chapter"),
            lyrics: Some(vec![
                "I remember when it all started, something from a dream",
                "Addicted to the black and green letters on my screen",
            ]),
        };

        assert_eq!(balloon, expected);
    }
}
