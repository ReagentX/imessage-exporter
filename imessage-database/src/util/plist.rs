/*!
 This module contains logic to parse text from plist payload data
*/

use plist::{Dictionary, Value};

use crate::error::plist::PlistParseError;

/// Serialize a message's `payload_data` BLOB from the `NSKeyedArchiver` format to a native Dictionary
/// that follows the references in the XML document's UID pointers. First, we find the root of the
/// document, then walk the structure, promoting the end values to the places where their pointers are stored.
///
/// For example, a document with a root pointing to some XML like
///
/// ```xml
/// <array>
///     <dict>
///         <key>link</key>
///         <dict>
///              <key>CF$UID</key>
///              <integer>2</integer>
///         </dict>
///     </dict>
///     <string>https://chrissardegna.com</string>
/// </array>
/// ```
///
/// Will be serialized to a dictionary that looks like:
///
/// ```json
/// {
///     link: https://chrissardegna.com
/// }
/// ```
///
/// Some detail on this format is described [here](https://en.wikipedia.org/wiki/Property_list#Serializing_to_plist):
///
/// > Internally, `NSKeyedArchiver` somewhat recapitulates the binary plist format by
/// storing an object table array called $objects in the dictionary. Everything else,
/// including class information, is referenced by a UID pointer. A $top entry under
/// the dict points to the top-level object the programmer was meaning to encode.
pub fn parse_plist(plist: &Value) -> Result<Value, PlistParseError> {
    let body = plist.as_dictionary().ok_or_else(|| {
        PlistParseError::InvalidType("body".to_string(), "dictionary".to_string())
    })?;
    let objects = extract_array_key(body, "$objects")?;

    // Index of root object
    let root = extract_uid_key(extract_dictionary(body, "$top")?, "root")?;

    follow_uid(objects, root, None, None)
}

/// Recursively follows pointers in an `NSKeyedArchiver` format, promoting the values
/// to the positions where the pointers live
fn follow_uid<'a>(
    objects: &'a Vec<Value>,
    root: usize,
    parent: Option<String>,
    item: Option<&'a Value>,
) -> Result<Value, PlistParseError> {
    let item = match item {
        Some(item) => item,
        None => objects
            .get(root)
            .ok_or(PlistParseError::NoValueAtIndex(root))?,
    };

    match item {
        Value::Array(arr) => {
            let mut array = vec![];
            for item in arr {
                if let Some(idx) = item.as_uid() {
                    array.push(follow_uid(
                        objects,
                        idx.get() as usize,
                        parent.to_owned(),
                        None,
                    )?);
                }
            }
            Ok(plist::Value::Array(array))
        }
        Value::Dictionary(dict) => {
            let mut dictionary = Dictionary::new();
            // Handle where type is a Dictionary that points to another single value
            if let Some(relative) = dict.get("NS.relative") {
                if let Some(idx) = relative.as_uid() {
                    if let Some(p) = &parent {
                        dictionary.insert(
                            p.to_string(),
                            follow_uid(objects, idx.get() as usize, Some(p.to_string()), None)?,
                        );
                    }
                }
            }
            // Handle the NSDictionary and NSMutableDictionary types
            else if dict.contains_key("NS.keys") && dict.contains_key("NS.objects") {
                let keys = extract_array_key(dict, "NS.keys")?;
                // These are the values in the objects list
                let values = extract_array_key(dict, "NS.objects")?;
                // Die here if the data is invalid
                if keys.len() != values.len() {
                    return Err(PlistParseError::InvalidDictionarySize(
                        keys.len(),
                        values.len(),
                    ));
                }

                for idx in 0..keys.len() {
                    let key_index = extract_uid_idx(keys, idx)?;
                    let value_index = extract_uid_idx(values, idx)?;
                    let key = extract_string_idx(objects, key_index)?;

                    dictionary.insert(
                        key.to_string(),
                        follow_uid(objects, value_index, Some(key.to_string()), None)?,
                    );
                }
            }
            // Handle a normal `{key: value}` style dictionary
            else {
                for (key, val) in dict {
                    // Handle skips
                    if key == "$class" {
                        continue;
                    }
                    // If the value is a pointer, follow it
                    if let Some(idx) = val.as_uid() {
                        dictionary.insert(
                            key.to_owned(),
                            follow_uid(objects, idx.get() as usize, Some(key.to_string()), None)?,
                        );
                    }
                    // If the value is not a pointer, try and follow the data itself
                    else if let Some(p) = &parent {
                        dictionary.insert(
                            p.to_owned(),
                            follow_uid(objects, root, Some(p.to_string()), Some(val))?,
                        );
                    }
                }
            }
            Ok(plist::Value::Dictionary(dictionary))
        }
        Value::Uid(uid) => follow_uid(objects, uid.get() as usize, None, None),
        _ => Ok(item.to_owned()),
    }
}

/// Extract a dictionary from a specific key in a collection
pub fn extract_dictionary<'a>(
    body: &'a Dictionary,
    key: &str,
) -> Result<&'a Dictionary, PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_dictionary()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "dictionary".to_string()))
}

/// Extract an array from a specific key in a collection
fn extract_array_key<'a>(
    body: &'a Dictionary,
    key: &str,
) -> Result<&'a Vec<Value>, PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_array()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "array".to_string()))
}

/// Extract a Uid from a specific key in a collection
fn extract_uid_key(body: &Dictionary, key: &str) -> Result<usize, PlistParseError> {
    Ok(body
        .get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_uid()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "uid".to_string()))?
        .get() as usize)
}

/// Extract bytes from a specific key in a collection
pub fn extract_bytes_key<'a>(body: &'a Dictionary, key: &str) -> Result<&'a [u8], PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_data()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "data".to_string()))
}

/// Extract an int from a specific key in a collection
pub fn extract_int_key(body: &Dictionary, key: &str) -> Result<i64, PlistParseError> {
    Ok(body
        .get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_real()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "int".to_string()))?
        as i64)
}

/// Extract a Uid from a specific index in a collection
fn extract_uid_idx(body: &[Value], idx: usize) -> Result<usize, PlistParseError> {
    Ok(body
        .get(idx)
        .ok_or(PlistParseError::NoValueAtIndex(idx))?
        .as_uid()
        .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "uid".to_string()))?
        .get() as usize)
}

/// Extract a string from a specific index in a collection
fn extract_string_idx(body: &[Value], idx: usize) -> Result<&str, PlistParseError> {
    body.get(idx)
        .ok_or(PlistParseError::NoValueAtIndex(idx))?
        .as_string()
        .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "string".to_string()))
}

/// Extract a string from a key-value pair that looks like {key: String("value")}
pub fn get_string_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// Extract a bool from a key-value pair that looks like {key: true}
pub fn get_bool_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<bool> {
    payload.as_dictionary()?.get(key)?.as_boolean()
}

/// Extract a string from a key-value pair that looks like {key: {key: String("value")}}
pub fn get_string_from_nested_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_dictionary()?
        .get(key)?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// Extract a float from a key-value pair that looks like {key: {key: 1.2}}
pub fn get_float_from_nested_dict<'a>(payload: &'a Value, key: &'a str) -> Option<f64> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_dictionary()?
        .get(key)?
        .as_real()
}
