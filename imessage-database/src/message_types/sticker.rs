/*!
These are [sticker messages](https://support.apple.com/guide/iphone/send-stickers-iph37b0bfe7b/ios), either from the user's sticker library or sticker apps.
*/

use std::fmt::Display;

/// Bytes for `stickerEffect:type="`
const STICKER_EFFECT_PREFIX: [u8; 20] = [
    115, 116, 105, 99, 107, 101, 114, 69, 102, 102, 101, 99, 116, 58, 116, 121, 112, 101, 61, 34,
];
/// Bytes for `"/>`
const STICKER_EFFECT_SUFFIX: [u8; 3] = [34, 47, 62];

/// Represents different types of [sticker effects](https://www.macrumors.com/how-to/add-effects-to-stickers-in-messages/) that can be applied to sticker iMessage balloons.
#[derive(Debug, PartialEq, Eq)]
pub enum StickerEffect {
    /// Sticker sent with no effect
    Normal,
    /// Internally referred to as `stroke`
    Outline,
    Comic,
    Puffy,
    /// Internally referred to as `iridescent`
    Shiny,
    Other(String),
}

impl StickerEffect {
    /// Determine the type of a sticker from parsed `HEIC` `EXIF` data
    fn from_exif(sticker_effect_type: &str) -> Self {
        match sticker_effect_type {
            "stroke" => Self::Outline,
            "comic" => Self::Comic,
            "puffy" => Self::Puffy,
            "iridescent" => Self::Shiny,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl Display for StickerEffect {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StickerEffect::Normal => write!(fmt, "Normal"),
            StickerEffect::Outline => write!(fmt, "Outline"),
            StickerEffect::Comic => write!(fmt, "Comic"),
            StickerEffect::Puffy => write!(fmt, "Puffy"),
            StickerEffect::Shiny => write!(fmt, "Shiny"),
            StickerEffect::Other(name) => write!(fmt, "{name}"),
        }
    }
}

impl Default for StickerEffect {
    fn default() -> Self {
        Self::Normal
    }
}

/// Parse the sticker effect type from the EXIF data of a HEIC blob
pub fn get_sticker_effect(mut heic_data: Vec<u8>) -> StickerEffect {
    // Find the start index and drain
    for idx in 0..heic_data.len() {
        if idx + STICKER_EFFECT_PREFIX.len() < heic_data.len() {
            let part = &heic_data[idx..idx + STICKER_EFFECT_PREFIX.len()];
            if part == STICKER_EFFECT_PREFIX {
                // Remove the start pattern from the blob
                heic_data.drain(..idx + STICKER_EFFECT_PREFIX.len());
                break;
            }
        } else {
            return StickerEffect::Normal;
        }
    }

    // Find the end index and truncate
    for idx in 1..heic_data.len() {
        if idx >= heic_data.len() - 2 {
            return StickerEffect::Other("Unknown".to_string());
        }
        let part = &heic_data[idx..idx + STICKER_EFFECT_SUFFIX.len()];

        if part == STICKER_EFFECT_SUFFIX {
            // Remove the end pattern from the string
            heic_data.truncate(idx);
            break;
        }
    }
    StickerEffect::from_exif(&String::from_utf8_lossy(&heic_data))
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;

    use crate::message_types::sticker::{get_sticker_effect, StickerEffect};

    #[test]
    fn test_parse_sticker_normal() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/no_effect.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(bytes);

        assert_eq!(effect, StickerEffect::Normal);
    }

    #[test]
    fn test_parse_sticker_outline() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/outline.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(bytes);

        assert_eq!(effect, StickerEffect::Outline);
    }

    #[test]
    fn test_parse_sticker_comic() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/comic.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(bytes);

        assert_eq!(effect, StickerEffect::Comic);
    }

    #[test]
    fn test_parse_sticker_puffy() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/puffy.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(bytes);

        assert_eq!(effect, StickerEffect::Puffy);
    }

    #[test]
    fn test_parse_sticker_shiny() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/shiny.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(bytes);

        assert_eq!(effect, StickerEffect::Shiny);
    }
}
