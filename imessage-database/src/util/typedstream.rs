/*!
 Helpers for working with `Property` types in the Crabstep deserializer.
*/

use crabstep::{OutputData, PropertyIterator, deserializer::iter::Property};

/// Represents a range pair that contains a type index and a length.
#[derive(Debug)]
pub struct TypeLengthPair {
    /// The type index of the property
    pub type_index: i64,
    /// The length of the text affected by the referenced property
    pub length: u64,
}

/// Converts a `Property` to a range pair used to denote a type index and a length
#[inline(always)]
pub fn as_type_length_pair<'a>(property: &'a mut Property<'a, 'a>) -> Option<TypeLengthPair> {
    if let Property::Group(group) = property {
        let mut iter = group.iter();
        if let Some(Property::Primitive(OutputData::SignedInteger(type_index))) = iter.next() {
            if let Some(Property::Primitive(OutputData::UnsignedInteger(length))) = iter.next() {
                return Some(TypeLengthPair {
                    type_index: *type_index,
                    length: *length,
                });
            }
        }
    }

    None
}

/// Converts a `Property` to an `Option<i64>` if it is a signed integer or similar structure.
#[must_use]
#[inline(always)]
pub fn as_signed_integer(property: &Property<'_, '_>) -> Option<i64> {
    if let Property::Group(group) = property {
        let mut iter = group.iter();
        let val = iter.next()?;
        if let Property::Primitive(OutputData::SignedInteger(value)) = val {
            return Some(*value);
        } else if let Property::Object { name, data, .. } = val {
            if *name == "NSNumber" {
                // Clone the iterator to be able to call next() on it
                let mut data_iter = data.clone();
                return as_signed_integer(&data_iter.next()?);
            }
        }
    }
    None
}

/// Converts a `Property` to an `Option<u64>` if it is an unsigned integer or similar structure.
#[must_use]
#[inline(always)]
pub fn as_unsigned_integer<'a>(property: &'a Property<'a, 'a>) -> Option<u64> {
    if let Property::Group(group) = property {
        let mut iter = group.iter();
        let val = iter.next()?;
        if let Property::Primitive(OutputData::UnsignedInteger(value)) = val {
            return Some(*value);
        } else if let Property::Object { name, data, .. } = val {
            if *name == "NSNumber" {
                // Clone the iterator to be able to call next() on it
                let mut data_iter = data.clone();
                return as_unsigned_integer(&data_iter.next()?);
            }
        }
    }
    None
}

/// Converts a `Property` to an `Option<f64>` if it is an unsigned integer or similar structure.
#[must_use]
#[inline(always)]
pub fn as_float<'a>(property: &'a Property<'a, 'a>) -> Option<f64> {
    if let Property::Group(group) = property {
        let mut iter = group.iter();
        let val = iter.next()?;
        if let Property::Primitive(OutputData::Double(value)) = val {
            return Some(*value);
        } else if let Property::Object { name, data, .. } = val {
            if *name == "NSNumber" {
                // Clone the iterator to be able to call next() on it
                let mut data_iter = data.clone();
                return as_float(&data_iter.next()?);
            }
        }
    }
    None
}

/// Converts a `Property` to an `Option<&str>` if it is a `NSString` or similar structure.
#[inline(always)]
pub fn as_nsstring<'a>(property: &'a mut Property<'a, 'a>) -> Option<&'a str> {
    if let Property::Group(group) = property {
        let mut iter = group.iter_mut();
        if let Some(Property::Object { name, data, .. }) = iter.next() {
            if *name == "NSString" || *name == "NSAttributedString" || *name == "NSMutableString" {
                if let Some(Property::Group(prim)) = data.next() {
                    if let Some(Property::Primitive(OutputData::String(s))) = prim.first() {
                        return Some(s);
                    }
                }
            }
        }
    }
    None
}

/// Converts a `Property` to Vec<Property> if it is a `NSDictionary`
#[inline(always)]
pub fn as_ns_dictionary<'a>(
    property: &'a mut Property<'a, 'a>,
) -> Option<&'a mut PropertyIterator<'a, 'a>> {
    if let Property::Group(group) = property {
        let mut iter = group.iter_mut();
        if let Some(Property::Object {
            class: _,
            name,
            data,
        }) = iter.next()
        {
            if *name == "NSDictionary" {
                return Some(data);
            }
        }
    }

    None
}

/// Given a mutable reference to a resolved `Property`,\
/// walks 2 levels of nested groups under an NSURL→NSString and returns the inner &str.
#[inline(always)]
pub fn as_nsurl<'a>(property: &'a mut Property<'a, 'a>) -> Option<&'a str> {
    // only care about top‐level Group
    if let Property::Group(groups) = property {
        for level1 in groups.iter_mut() {
            // look for Object(name="NSURL", data=...)
            if let Property::Object {
                name,
                data: url_data,
                ..
            } = level1
            {
                if *name != "NSURL" {
                    continue;
                }
                // first level under NSURL
                for level2 in url_data {
                    if let Property::Group(mut inner) = level2 {
                        for level3 in &mut inner {
                            // look for Object(name="NSString", data=...)
                            if let Property::Object {
                                name,
                                data: str_data,
                                ..
                            } = level3
                            {
                                if *name != "NSString" {
                                    continue;
                                }
                                // second level under NSString
                                for level4 in str_data {
                                    if let Property::Group(bottom) = level4 {
                                        for p in bottom {
                                            if let Property::Primitive(OutputData::String(s)) = p {
                                                return Some(s);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
