use crate::error::plist::PlistParseError;

use plist::{Dictionary, Value};

use std::collections::HashMap;

/// Determine if a plist item is a nested type, currently limited to Dictionaries
fn is_nested(item: &Value) -> bool {
    matches!(item, Value::Dictionary(_))
}

/// Extract the name of a class
fn get_class_name<'a>(
    objects: &'a Vec<Value>,
    item: &'a Dictionary,
) -> Result<&'a str, PlistParseError<'static>> {
    // Get the class to determine how to parse the data
    let class_idx = item
        .get("$class")
        .ok_or(PlistParseError::MissingKey("$class"))?
        .as_uid()
        .ok_or(PlistParseError::InvalidType("$class", "uid"))?
        .get() as usize;

    let class = objects
        .get(class_idx)
        .ok_or(PlistParseError::NoValueAtIndex(class_idx))?
        .as_dictionary()
        .ok_or(PlistParseError::InvalidTypeIndex(class_idx, "dictionary"))?;

    let class_name = class
        .get("$classname")
        .ok_or(PlistParseError::MissingKey("$classname"))?
        .as_string()
        .ok_or(PlistParseError::InvalidType("$classname", "string"))?;

    Ok(class_name)
}

/// Recursively extract Values from the obejcts array
fn extract<'a>(
    objects: &'a Vec<Value>,
    root: usize,
    cache: &mut HashMap<&'a str, &'a Value>,
    key: Option<&'a str>,
) -> Result<(), PlistParseError<'static>> {
    // Get the current item, if possible
    let item = objects
        .get(root)
        .ok_or(PlistParseError::NoValueAtIndex(root))?;

    // If it is nested, determine if the data is in a known layout, otherwise skip
    if let Some(data) = item.as_dictionary() {
        // Get the class to determine how to parse the data
        match get_class_name(objects, data).unwrap_or("") {
            // These data structures have custom mapping and need their own logic
            "NSDictionary" | "NSMutableDictionary" => {
                // These are the keys in the objects list
                let keys = data
                    .get("NS.keys")
                    .ok_or(PlistParseError::MissingKey("NS.keys"))?
                    .as_array()
                    .ok_or(PlistParseError::InvalidType("NS.keys", "array"))?;
                // These are the values in the objects list
                let values = data
                    .get("NS.objects")
                    .ok_or(PlistParseError::MissingKey("NS.objects"))?
                    .as_array()
                    .ok_or(PlistParseError::InvalidType("NS.objects", "array"))?;
                // Die here if the data is invalid
                if keys.len() != values.len() {
                    return Err(PlistParseError::InvalidDictionarySize(
                        keys.len(),
                        values.len(),
                    ));
                }

                for idx in 0..keys.len() {
                    let key_index = keys
                        .get(idx)
                        .ok_or(PlistParseError::NoValueAtIndex(idx))?
                        .as_uid()
                        .ok_or(PlistParseError::InvalidTypeIndex(idx, "uid"))?
                        .get() as usize;
                    let value_index = values
                        .get(idx)
                        .ok_or(PlistParseError::NoValueAtIndex(idx))?
                        .as_uid()
                        .ok_or(PlistParseError::InvalidTypeIndex(idx, "uid"))?
                        .get() as usize;

                    let key = objects
                        .get(key_index)
                        .ok_or(PlistParseError::NoValueAtIndex(idx))?
                        .as_string()
                        .ok_or(PlistParseError::InvalidTypeIndex(key_index, "string"))?;
                    let value = objects
                        .get(value_index)
                        .ok_or(PlistParseError::NoValueAtIndex(idx))?;

                    match is_nested(value) {
                        true => {
                            extract(objects, value_index, cache, Some(key))?;
                        }
                        false => {
                            cache.insert(key, value);
                        }
                    }
                }
            }
            "NSURL" => {
                let url_idx = data
                    .get("NS.relative")
                    .ok_or(PlistParseError::MissingKey("NS.relative"))?
                    .as_uid()
                    .ok_or(PlistParseError::InvalidType("NS.relative", "uid"))?
                    .get() as usize;

                let url = objects
                    .get(url_idx)
                    .ok_or(PlistParseError::NoValueAtIndex(url_idx))?;
                cache.insert(key.ok_or(PlistParseError::NoValueAtIndex(url_idx))?, url);
            }
            // Generic key-value extractor
            other => {
                for (key, val) in data.into_iter() {
                    // This metadata describes the Obj-C or Swift class NSUnarchiver would serailize to
                    // But since we are supplying our own logic, this is not necessary
                    if key == "$class" {
                        continue;
                    }
                    if let Some(idx) = val.as_uid() {
                        // println!("{other} {idx:?} from {key}");
                        extract(objects, idx.get() as usize, cache, Some(key))?;
                    } else {
                        cache.insert(key, val);
                    }
                }
            }
        }
    } else {
        if let Some(k) = key {
            cache.insert(k, item);
        }
    }

    Ok(())
}

/// Parse a message's `payload_data`
///
/// We parse them using the logic described [here](https://en.wikipedia.org/wiki/Property_list#Serializing_to_plist):
///
/// > Internally, [NSKeyedArchiver] somewhat recapitulates the binary plist format by
/// storing an object table array called $objects in the dictionary. Everything else,
/// including class information, is referenced by a UID pointer. A $top entry under
/// the dict points to the top-level object the programmer was meaning to encode.
pub fn parse_plist(plist: &Value) -> Result<HashMap<&str, &Value>, PlistParseError<'static>> {
    let mut possible_data = HashMap::new();

    let body = plist
        .as_dictionary()
        .ok_or(PlistParseError::InvalidType("body", "dictionary"))?;
    let objects = body
        .get("$objects")
        .ok_or(PlistParseError::MissingKey("$objects"))?
        .as_array()
        .ok_or(PlistParseError::InvalidType("$objects", "array"))?;

    // Index of root object
    let root = body
        .get("$top")
        .ok_or(PlistParseError::MissingKey("$top"))?
        .as_dictionary()
        .ok_or(PlistParseError::InvalidType("$top", "dictionary"))?
        .get("root")
        .ok_or(PlistParseError::MissingKey("root"))?
        .as_uid()
        .ok_or(PlistParseError::InvalidType("root", "uid"))?
        .get() as usize;

    extract(objects, root, &mut possible_data, None)?;

    Ok(possible_data)
}

pub fn extract_parsed_str<'a>(
    payload: &'a HashMap<&'a str, &'a Value>,
    key: &str,
) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(|item| item.as_string())
        .filter(|s| !s.is_empty())
}
