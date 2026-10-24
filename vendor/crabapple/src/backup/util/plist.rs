//! Plist utility functions.

use plist::{Value, dictionary::Dictionary};

use crate::error::{BackupError, Result};

/// Convert a `plist::Value` to a dictionary reference.
///
/// # Arguments
/// * `plist_data` - The plist value expected to be a dictionary at the top level.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the top-level plist is not a dictionary.
pub(crate) fn as_dictionary(plist_data: &Value) -> Result<&Dictionary> {
    plist_data
        .as_dictionary()
        .ok_or_else(|| BackupError::PlistParseError("Top-level plist is not a dictionary".into()))
}

/// Get a string value from a plist dictionary for the specified key.
///
/// # Arguments
/// * `dict` - The plist dictionary to query.
/// * `key` - The key whose associated value should be a string.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the key is missing or the value is not a [`String`].
pub(crate) fn get_key_as_string(dict: &Dictionary, key: &str) -> Result<String> {
    dict.get(key)
        .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} not found in plist!")))
        .and_then(|v| {
            v.as_string()
                .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} is not a string!")))
        })
        .map(std::string::ToString::to_string)
}

/// Get data bytes from a plist dictionary for the specified key.
///
/// # Arguments
/// * `dict` - The plist dictionary to query.
/// * `key` - The key whose associated value should be raw data bytes.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the key is missing or the value is not data.
pub(crate) fn get_key_as_data(dict: &Dictionary, key: &str) -> Result<Vec<u8>> {
    dict.get(key)
        .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} not found in plist!")))
        .and_then(|v| {
            v.as_data()
                .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} is not data!")))
        })
        .map(<[u8]>::to_vec)
}

/// Get a boolean value from a plist dictionary for the specified key.
///
/// # Arguments
/// * `dict` - The plist dictionary to query.
/// * `key` - The key whose associated value should be a boolean.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the key is missing or the value is not [`bool`].
pub(crate) fn get_key_as_boolean(dict: &Dictionary, key: &str) -> Result<bool> {
    dict.get(key)
        .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} not found in plist!")))
        .and_then(|v| {
            v.as_boolean()
                .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} is not boolean!")))
        })
        .map(|b| b.to_owned())
}

/// Get an unsigned integer value from a plist dictionary for the specified key.
///
/// # Arguments
/// * `dict` - The plist dictionary to query.
/// * `key` - The key whose associated value should be an unsigned integer.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the key is missing or the value is not a [`u64`].
pub(crate) fn get_key_as_uint(dict: &Dictionary, key: &str) -> Result<u64> {
    dict.get(key)
        .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} not found in plist!")))
        .and_then(|v| {
            v.as_unsigned_integer()
                .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} is not uint!")))
        })
        .map(|u| u.to_owned())
}

/// Get a signed integer value from a plist dictionary for the specified key.
///
/// # Arguments
/// * `dict` - The plist dictionary to query.
/// * `key` - The key whose associated value should be a signed integer.
///
/// # Errors
/// Returns [`BackupError::PlistParseError`] if the key is missing or the value is not a [`i64`].
pub(crate) fn get_key_as_int(dict: &Dictionary, key: &str) -> Result<i64> {
    dict.get(key)
        .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} not found in plist!")))
        .and_then(|v| {
            v.as_signed_integer()
                .ok_or_else(|| BackupError::PlistParseError(format!("Key {key} is not int!")))
        })
        .map(|i| i.to_owned())
}
