/*!
 Helpers for reading message property-list payloads.

 The main entry point is [`parse_ns_keyed_archiver()`]. For normal property lists, use [`plist_as_dictionary()`].

 ## Overview

 The `NSKeyedArchiver` format is a property list-based serialization protocol used by Apple's Foundation framework.
 It stores object graphs in a keyed format, allowing for more flexible deserialization and better handling of
 object references compared to the older typedstream format.

 ## Origin

 Introduced in Mac OS X 10.2 as part of the Foundation framework, `NSKeyedArchiver` replaced `NSArchiver`
 ([`typedstream`](crate::util::typedstream)) system as Apple's primary object serialization mechanism.

 ## Features

 - Pure Rust implementation for efficient and safe deserialization
 - Support for both XML and binary property list formats
 - No dependencies on Apple frameworks
 - Robust error handling for malformed or invalid archives
*/

use plist::{Dictionary, Value};

use crate::error::plist::PlistParseError;

/// Maximum depth of UID-reference resolution before bailing out.
const MAX_UID_DEPTH: usize = 256;
/// The object-table value NSKeyedArchiver uses for an unset field.
const NULL_PLACEHOLDER: &str = "$null";

/// Deserialize an `NSKeyedArchiver` property list by resolving UID references.
///
/// The archive stores objects in `$objects` and points `$top.root` at the root
/// object. This walks those references and returns the reconstructed value.
///
/// For example, a document with a root pointing to some `XML` like
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
/// parses into a dictionary that looks like:
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
/// > storing an object table array called `$objects` in the dictionary. Everything else,
/// > including class information, is referenced by a UID pointer. A `$top` entry under
/// > the dict points to the top-level object the programmer was meaning to encode.
///
/// # Data Source
///
/// The source plist data generally comes from [`Message::payload_data()`](crate::tables::messages::message::Message::payload_data).
pub fn parse_ns_keyed_archiver(plist: &Value) -> Result<Value, PlistParseError> {
    let body = plist_as_dictionary(plist)?;
    let objects = extract_array_key(body, "$objects")?;

    // Index of root object
    let root = extract_uid_key(extract_dictionary(body, "$top")?, "root")?;

    // `encode(nil, forKey: root)` emits `$top.root -> UID 0`: an archive with
    // no object in it.
    follow_uid(objects, root, None, None, 0)?.ok_or(PlistParseError::NoPayload)
}

/// Resolve one archived object and any UID references it contains.
///
/// `None` is a reference to the archiver's null placeholder. Nested nulls are
/// omitted from their containing dictionary or array; a null root is
/// [`PlistParseError::NoPayload`] in [`parse_ns_keyed_archiver`].
fn follow_uid<'a>(
    objects: &'a [Value],
    root: usize,
    parent: Option<&'a Value>,
    item: Option<&'a Value>,
    depth: usize,
) -> Result<Option<Value>, PlistParseError> {
    if depth >= MAX_UID_DEPTH {
        return Err(PlistParseError::RecursionLimit);
    }
    if item.is_none() && is_null_placeholder(objects, root) {
        return Ok(None);
    }
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
                if let Some(idx) = item.as_uid()
                    && let Some(value) =
                        follow_uid(objects, uid_to_index(idx)?, parent, None, depth + 1)?
                {
                    array.push(value);
                }
            }
            Ok(Some(plist::Value::Array(array)))
        }
        Value::Dictionary(dict) => {
            let mut dictionary = Dictionary::new();
            // Handle where type is a Dictionary that points to another single value
            if let Some(relative) = dict.get("NS.relative") {
                if let Some(idx) = relative.as_uid()
                    && let Some(p) = parent
                    && let Some(value) =
                        follow_uid(objects, uid_to_index(idx)?, Some(p), None, depth + 1)?
                {
                    dictionary.insert(value_to_key_string(p), value);
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
                    let Some(key) = follow_uid(objects, key_index, None, None, depth + 1)? else {
                        continue;
                    };
                    let Some(value) =
                        follow_uid(objects, value_index, Some(&key), None, depth + 1)?
                    else {
                        continue;
                    };

                    dictionary.insert(value_to_key_string(&key), value);
                }
            }
            // Handle a normal `{key: value}` style dictionary
            else {
                for (key, val) in dict {
                    // Skip class names; we don't need them
                    if key == "$class" {
                        continue;
                    }
                    // If the value is a pointer, follow it
                    if let Some(idx) = val.as_uid() {
                        let key_value = Value::String(key.clone());
                        if let Some(value) = follow_uid(
                            objects,
                            uid_to_index(idx)?,
                            Some(&key_value),
                            None,
                            depth + 1,
                        )? {
                            dictionary.insert(key.clone(), value);
                        }
                    }
                    // If the value is not a pointer, try and follow the data itself
                    else if let Some(p) = parent
                        && let Some(value) =
                            follow_uid(objects, root, Some(p), Some(val), depth + 1)?
                    {
                        dictionary.insert(value_to_key_string(p), value);
                    }
                }
            }
            Ok(Some(plist::Value::Dictionary(dictionary)))
        }
        Value::Uid(uid) => follow_uid(objects, uid_to_index(uid)?, None, None, depth + 1),
        _ => Ok(Some(item.to_owned())),
    }
}

/// Identify the archiver's null object without conflating it with a literal
/// string whose contents happen to be `"$null"`.
fn is_null_placeholder(objects: &[Value], index: usize) -> bool {
    index == 0
        && objects
            .first()
            .and_then(Value::as_string)
            .is_some_and(|value| value == NULL_PLACEHOLDER)
}

/// Convert a plist value into a dictionary key.
fn value_to_key_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Date(d) => format!("{d:?}"),
        Value::Data(_) => "data".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Dictionary(_) => "dict".to_string(),
        Value::Uid(u) => u.get().to_string(),
        _ => "unknown".to_string(),
    }
}

/// Extract a dictionary from table `plist` data.
pub fn plist_as_dictionary(plist: &Value) -> Result<&Dictionary, PlistParseError> {
    plist
        .as_dictionary()
        .ok_or_else(|| PlistParseError::InvalidType("body".to_string(), "dictionary".to_string()))
}

/// Extract the shared `richLinkMetadata` payload and one nested metadata value.
///
/// Returns `(richLinkMetadata, nested_value)`.
pub fn rich_link_metadata_and_nested<'a>(
    payload: &'a Value,
    nested_key: &str,
) -> Result<(&'a Value, &'a Value), PlistParseError> {
    let base = payload
        .as_dictionary()
        .ok_or_else(|| PlistParseError::InvalidType("root".to_string(), "dictionary".to_string()))?
        .get("richLinkMetadata")
        .ok_or_else(|| PlistParseError::MissingKey("richLinkMetadata".to_string()))?;

    let nested = base
        .as_dictionary()
        .ok_or_else(|| {
            PlistParseError::InvalidType("richLinkMetadata".to_string(), "dictionary".to_string())
        })?
        .get(nested_key)
        .ok_or_else(|| PlistParseError::MissingKey(nested_key.to_string()))?;

    Ok((base, nested))
}

/// Extract a dictionary from a collection key.
pub fn extract_dictionary<'a>(
    body: &'a Dictionary,
    key: &str,
) -> Result<&'a Dictionary, PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_dictionary()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "dictionary".to_string()))
}

/// Extract an array from a collection key.
pub fn extract_array_key<'a>(
    body: &'a Dictionary,
    key: &str,
) -> Result<&'a Vec<Value>, PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_array()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "array".to_string()))
}

/// Extract a `UID` from a collection key.
fn extract_uid_key(body: &Dictionary, key: &str) -> Result<usize, PlistParseError> {
    let uid = body
        .get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_uid()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "uid".to_string()))?;
    uid_to_index(uid)
}

/// Extract bytes from a collection key.
pub fn extract_bytes_key<'a>(body: &'a Dictionary, key: &str) -> Result<&'a [u8], PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_data()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "data".to_string()))
}

/// Extract a real value from a collection key and coerce it to `i64`.
pub fn extract_int_key(body: &Dictionary, key: &str) -> Result<i64, PlistParseError> {
    Ok(body
        .get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_real()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "real".to_string()))?
        as i64)
}

/// Extract a string slice from a collection key.
pub fn extract_string_key<'a>(body: &'a Dictionary, key: &str) -> Result<&'a str, PlistParseError> {
    body.get(key)
        .ok_or_else(|| PlistParseError::MissingKey(key.to_string()))?
        .as_string()
        .ok_or_else(|| PlistParseError::InvalidType(key.to_string(), "string".to_string()))
}

/// Extract a UID from a collection index.
fn extract_uid_idx(body: &[Value], idx: usize) -> Result<usize, PlistParseError> {
    let uid = body
        .get(idx)
        .ok_or(PlistParseError::NoValueAtIndex(idx))?
        .as_uid()
        .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "uid".to_string()))?;
    uid_to_index(uid)
}

/// Convert a plist UID into an object-table index without narrowing.
fn uid_to_index(uid: &plist::Uid) -> Result<usize, PlistParseError> {
    usize::try_from(uid.get()).map_err(|_| PlistParseError::UidOutOfRange(uid.get()))
}

/// Extract a dictionary from a collection index.
pub fn extract_dict_idx(body: &[Value], idx: usize) -> Result<&Dictionary, PlistParseError> {
    body.get(idx)
        .ok_or(PlistParseError::NoValueAtIndex(idx))?
        .as_dictionary()
        .ok_or_else(|| PlistParseError::InvalidTypeIndex(idx, "dictionary".to_string()))
}

/// Extract a non-empty string from `{key: String("value")}`.
#[must_use]
pub fn get_string_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// Extract an owned non-empty string from `{key: String("value")}`.
#[must_use]
pub fn get_owned_string_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<String> {
    get_string_from_dict(payload, key).map(String::from)
}

/// Extract a value from `{key: value}`.
#[must_use]
pub fn get_value_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a Value> {
    payload.as_dictionary()?.get(key)
}

/// Extract a boolean from `{key: true}`.
#[must_use]
pub fn get_bool_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<bool> {
    payload.as_dictionary()?.get(key)?.as_boolean()
}

/// Extract a byte slice from `{key: Data(...)}`.
#[must_use]
pub fn get_data_from_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a [u8]> {
    payload.as_dictionary()?.get(key)?.as_data()
}

/// Extract a non-empty string from `{key: {key: String("value")}}`.
#[must_use]
pub fn get_string_from_nested_dict<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_dictionary()?
        .get(key)?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// Extract a float from `{key: {key: 1.2}}`.
#[must_use]
pub fn get_float_from_nested_dict<'a>(payload: &'a Value, key: &'a str) -> Option<f64> {
    payload
        .as_dictionary()?
        .get(key)?
        .as_dictionary()?
        .get(key)?
        .as_real()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::Uid;

    /// Build a plist dictionary from key/value pairs.
    fn dict(pairs: Vec<(&str, Value)>) -> Value {
        let mut dictionary = Dictionary::new();
        for (key, value) in pairs {
            dictionary.insert(key.to_string(), value);
        }
        Value::Dictionary(dictionary)
    }

    #[test]
    fn resolves_simple_archive() {
        // `$top.root` -> `$objects[1]` == "hello"
        let archive = dict(vec![
            (
                "$objects",
                Value::Array(vec![
                    Value::String("$null".to_string()),
                    Value::String("hello".to_string()),
                ]),
            ),
            ("$top", dict(vec![("root", Value::Uid(Uid::new(1)))])),
        ]);

        assert_eq!(
            parse_ns_keyed_archiver(&archive).unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn null_root_is_no_payload() {
        // `NSKeyedArchiver.encode(nil, forKey: NSKeyedArchiveRootObjectKey)`
        // emits exactly this: `$objects == ["$null"]`, `$top.root -> UID 0`.
        let archive = dict(vec![
            (
                "$objects",
                Value::Array(vec![Value::String(NULL_PLACEHOLDER.to_string())]),
            ),
            ("$top", dict(vec![("root", Value::Uid(Uid::new(0)))])),
        ]);

        assert!(matches!(
            parse_ns_keyed_archiver(&archive),
            Err(PlistParseError::NoPayload)
        ));
    }

    #[test]
    fn omits_null_values_from_normal_dictionaries() {
        // `$objects[3]` is a bare "$null" string at a non-zero index. Foundation
        // never emits this shape (a literal "$null" NSString is archived as
        // `{$class: NSString, NS.string: "$null"}`); it exists to prove the
        // placeholder is identified by index, not by string contents.
        let archive = dict(vec![
            (
                "$objects",
                Value::Array(vec![
                    Value::String(NULL_PLACEHOLDER.to_string()),
                    dict(vec![
                        ("missing", Value::Uid(Uid::new(0))),
                        ("present", Value::Uid(Uid::new(2))),
                        ("literal", Value::Uid(Uid::new(3))),
                    ]),
                    Value::String("kept".to_string()),
                    Value::String(NULL_PLACEHOLDER.to_string()),
                ]),
            ),
            ("$top", dict(vec![("root", Value::Uid(Uid::new(1)))])),
        ]);

        assert_eq!(
            parse_ns_keyed_archiver(&archive).unwrap(),
            dict(vec![
                ("present", Value::String("kept".to_string())),
                ("literal", Value::String(NULL_PLACEHOLDER.to_string())),
            ])
        );
    }

    #[test]
    fn omits_null_values_from_ns_dictionary_pairs() {
        let archive = dict(vec![
            (
                "$objects",
                Value::Array(vec![
                    Value::String(NULL_PLACEHOLDER.to_string()),
                    dict(vec![
                        (
                            "NS.keys",
                            Value::Array(vec![Value::Uid(Uid::new(2)), Value::Uid(Uid::new(3))]),
                        ),
                        (
                            "NS.objects",
                            Value::Array(vec![Value::Uid(Uid::new(4)), Value::Uid(Uid::new(0))]),
                        ),
                    ]),
                    Value::String("present".to_string()),
                    Value::String("missing".to_string()),
                    Value::String("kept".to_string()),
                ]),
            ),
            ("$top", dict(vec![("root", Value::Uid(Uid::new(1)))])),
        ]);

        assert_eq!(
            parse_ns_keyed_archiver(&archive).unwrap(),
            dict(vec![("present", Value::String("kept".to_string()))])
        );
    }

    #[test]
    fn cyclic_uid_reference_is_rejected_not_overflowed() {
        // `$objects[1]` points at itself and `$top.root` points at index 1. Without
        // a depth bound this recurses forever and aborts the process via stack
        // overflow; the bound converts it into a recoverable error instead.
        let archive = dict(vec![
            (
                "$objects",
                Value::Array(vec![
                    Value::String("$null".to_string()),
                    Value::Uid(Uid::new(1)),
                ]),
            ),
            ("$top", dict(vec![("root", Value::Uid(Uid::new(1)))])),
        ]);

        assert!(matches!(
            parse_ns_keyed_archiver(&archive),
            Err(PlistParseError::RecursionLimit)
        ));
    }
}
