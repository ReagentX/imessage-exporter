//! Hexadecimal encoding and decoding functions.

use std::fmt::Write;

use crate::error::{BackupError, Result};

/// Decode a hexadecimal string into a byte vector.
///
/// # Arguments
///
/// * `hex_string` - String slice containing hex characters (even length).
///
/// # Errors
/// Returns [`BackupError::HexDecode`] if the string has odd length or invalid chars.
pub(crate) fn hex_decode(hex_string: &str) -> Result<Vec<u8>> {
    if !hex_string.len().is_multiple_of(2) {
        return Err(BackupError::HexDecode(
            "Input string has odd length".to_string(),
        ));
    }

    (0..hex_string.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_string[i..i + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|e| BackupError::HexDecode(format!("Invalid hex character: {e}")))
}

/// Encode a slice of bytes as a lowercase hexadecimal string.
///
/// # Arguments
///
/// * `bytes` - Byte slice to encode.
///
/// # Returns
/// A [`String`] of hex digits (two chars per input byte).
#[must_use]
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, &b| {
            write!(&mut s, "{b:02x}").unwrap();
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BackupError;

    #[test]
    fn test_hex_encode_roundtrip() {
        let data = b"rust";
        let encoded = hex_encode(data);
        assert_eq!(encoded, "72757374");
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_decode_odd_length() {
        let err = hex_decode("abc").unwrap_err();
        match err {
            BackupError::HexDecode(msg) => assert!(msg.contains("odd length")),
            _ => panic!("Expected HexDecode error"),
        }
    }

    #[test]
    fn test_hex_decode_invalid_char() {
        let err = hex_decode("zz").unwrap_err();
        match err {
            BackupError::HexDecode(msg) => assert!(msg.contains("Invalid hex character")),
            _ => panic!("Expected HexDecode error"),
        }
    }

    #[test]
    fn test_hex_decode_uppercase() {
        // Uppercase hex should decode correctly
        let data = vec![0u8, 0xAB, 0xCD, 0xEF];
        let hex = "00ABCDEF";
        let decoded = hex_decode(hex).unwrap();
        assert_eq!(decoded, data);
    }
}
