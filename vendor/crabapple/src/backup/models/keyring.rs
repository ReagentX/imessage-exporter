//! Backup key bags and class key models.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use crate::{
    backup::util::tlv::tlv_blocks,
    error::{BackupError, Result},
};

/// Represents the key bag from `Manifest.plist` containing encryption parameters and wrapped class keys.
#[derive(Debug, Clone)]
pub(crate) struct KeyRing {
    /// Bag type identifier (backup key bag version).
    pub bag_type: u32,
    /// Unique identifier for the backup key bag (TLV `b"UUID"`).
    pub uuid: Vec<u8>,
    /// Optional wrap key blob for certain classes (TLV `b"WRAP"`).
    pub wrap: Vec<u8>,
    /// DPSL parameter for initial `PBKDF2` derivation (TLV `b"DPSL"`).
    pub dpsl: Vec<u8>,
    /// DPIC iteration count for initial `PBKDF2` derivation (TLV `b"DPIC"`).
    pub dpic: u32,
    /// Salt for second `PBKDF2` derivation (TLV `b"SALT"`).
    pub salt: Vec<u8>,
    /// Iteration count for second `PBKDF2` derivation (TLV `b"ITER"`).
    pub iter: u32,
    /// Other TLV attributes not explicitly parsed (raw tag-to-blob map).
    pub(crate) attrs: HashMap<[u8; 4], Vec<u8>>,
    /// Map of protection class IDs to their wrapped key data.
    pub(crate) class_keys: HashMap<u32, ClassKeyData>,
}

impl KeyRing {
    /// Parse a raw backup key bag blob into a [`BackupKeyBag`], extracting TLV fields.
    ///
    /// # Arguments
    /// * `blob` - Raw TLV-encoded backup key bag bytes.
    pub(crate) fn from_bytes(blob: &[u8]) -> Result<KeyRing> {
        let mut bag = KeyRing {
            bag_type: 0,
            uuid: Vec::new(),
            wrap: Vec::new(),
            dpsl: Vec::new(),
            salt: Vec::new(),
            dpic: 0,
            iter: 0,
            attrs: HashMap::new(),
            class_keys: HashMap::new(),
        };
        let mut current: Option<HashMap<[u8; 4], Vec<u8>>> = None;
        for item in tlv_blocks(blob) {
            let (tag, data) = item?;
            // if a 4‐byte value, interpret as big‐endian u32
            if data.len() == 4 {
                let v = u32::from_be_bytes(
                    data.as_slice()
                        .try_into()
                        .map_err(BackupError::ConversionFailed)?,
                );
                if &tag == b"TYPE" {
                    bag.bag_type = v;
                    continue;
                }
            }

            match &tag {
                b"UUID" if bag.uuid.is_empty() => bag.uuid = data,
                b"WRAP" if bag.wrap.is_empty() => bag.wrap = data,
                b"DPSL" if bag.dpsl.is_empty() => bag.dpsl = data,
                b"SALT" if bag.salt.is_empty() => bag.salt = data,
                b"DPIC" if bag.dpic == 0 => {
                    bag.dpic = u32::from_be_bytes(
                        data.as_slice()
                            .try_into()
                            .map_err(BackupError::ConversionFailed)?,
                    );
                }
                b"ITER" if bag.iter == 0 => {
                    bag.iter = u32::from_be_bytes(
                        data.as_slice()
                            .try_into()
                            .map_err(BackupError::ConversionFailed)?,
                    );
                }
                b"UUID" => {
                    // starting a new class‐key record
                    if let Some(cur) = current.take() {
                        let class_id = u32::from_be_bytes(
                            cur[b"CLAS"][..]
                                .try_into()
                                .map_err(BackupError::ConversionFailed)?,
                        );
                        bag.class_keys
                            .insert(class_id, ClassKeyData::from_map(&cur));
                    }
                    let mut map = HashMap::new();
                    map.insert(tag, data);
                    current = Some(map);
                }
                t if current.is_some()
                    && (t == b"CLAS"
                        || b"WPKY".as_ref() == &t[..]
                        || b"PBKY".as_ref() == &t[..]
                        || b"KTYP".as_ref() == &t[..]
                        || b"WRAP" == &t[..]) =>
                {
                    // This unwrap is safe because we just checked that current is Some
                    // Eventually `if let` guards should be used, but they are not stable yet
                    if let Some(curr) = &mut current {
                        curr.insert(tag, data);
                    }
                }
                // For any other tags, add them to the attrs map
                _ => {
                    bag.attrs.insert(tag, data);
                }
            }
        }
        // don't forget the last one
        if let Some(cur) = current {
            let class_id = u32::from_be_bytes(
                cur[b"CLAS"][..]
                    .try_into()
                    .map_err(BackupError::ConversionFailed)?,
            );
            bag.class_keys
                .insert(class_id, ClassKeyData::from_map(&cur));
        }
        Ok(bag)
    }
}

/// Contains wrapped key variants and metadata for a single protection class entry.
#[derive(Debug, Clone)]
pub(crate) struct ClassKeyData {
    /// Wrapped passcode-derived class key (`WPKY`) if present.
    pub wpky: Option<Vec<u8>>,
    /// Wrapped backup key for non-passcode classes (`WRAP`) if present.
    pub wrap: Option<Vec<u8>>,
    /// Unique identifier for this class key entry (UUID), if present.
    pub _uuid: Option<Vec<u8>>,
}

impl ClassKeyData {
    /// Build a [`ClassKeyData`] from a TLV attribute map.
    ///
    /// # Arguments
    /// * `map` - Tag-to-value map from TLV blocks.
    #[must_use]
    pub(crate) fn from_map(map: &HashMap<[u8; 4], Vec<u8>>) -> ClassKeyData {
        // Prefer WPKY, fallback to PBKY for persistent key
        let wpky = map
            .get(b"WPKY")
            .cloned()
            .or_else(|| map.get(b"PBKY").cloned());
        ClassKeyData {
            wpky,
            wrap: map.get(b"WRAP").cloned(),
            _uuid: map.get(b"UUID").cloned(),
        }
    }
}

/// Wrapper type for an `AES` key encryption key used in key wrapping and unwrapping.
///
/// This newtype wraps a `Vec<u8>` representing a master or class key for `AES` key wrap (`RFC 3394`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionKey(Vec<u8>);

impl AsRef<[u8]> for EncryptionKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for EncryptionKey {
    fn from(v: Vec<u8>) -> EncryptionKey {
        EncryptionKey(v)
    }
}

impl Deref for EncryptionKey {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        &self.0
    }
}

impl DerefMut for EncryptionKey {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

/// Stores a decrypted `AES` key for a specific protection class.
#[derive(Debug, Clone)]
pub struct ProtectionClassKey {
    /// Numeric class identifier
    pub class_id: u32,
    /// Raw decrypted `AES` key.
    pub key: EncryptionKey,
}

#[cfg(test)]
mod tests_types {
    use std::collections::HashMap;

    use crate::backup::models::keyring::{ClassKeyData, KeyRing};

    #[test]
    fn test_backup_key_ring_from_bytes_basic() {
        // Construct a simple TLV blob: TYPE=1, DPSL=b"aa", DPIC=2, SALT=b"bb", ITER=3
        let mut blob = Vec::new();
        // TYPE
        blob.extend(b"TYPE");
        blob.extend(&4u32.to_be_bytes());
        blob.extend(&1u32.to_be_bytes());
        // DPSL
        blob.extend(b"DPSL");
        blob.extend(&2u32.to_be_bytes());
        blob.extend(b"aa");
        // DPIC
        blob.extend(b"DPIC");
        blob.extend(&4u32.to_be_bytes());
        blob.extend(&2u32.to_be_bytes());
        // SALT
        blob.extend(b"SALT");
        blob.extend(&2u32.to_be_bytes());
        blob.extend(b"bb");
        // ITER
        blob.extend(b"ITER");
        blob.extend(&4u32.to_be_bytes());
        blob.extend(&3u32.to_be_bytes());
        let bag = KeyRing::from_bytes(&blob).unwrap();

        assert_eq!(bag.bag_type, 1);
        assert_eq!(bag.dpsl, b"aa");
        assert_eq!(bag.dpic, 2);
        assert_eq!(bag.salt, b"bb");
        assert_eq!(bag.iter, 3);
        // No class keys parsed
        assert!(bag.class_keys.is_empty());
    }

    #[test]
    fn test_class_key_data_prefer_wpky() {
        let mut map: HashMap<[u8; 4], Vec<u8>> = HashMap::new();
        map.insert(*b"PBKY", b"pb".to_vec());
        map.insert(*b"WPKY", b"wp".to_vec());
        map.insert(*b"WRAP", b"wr".to_vec());
        map.insert(*b"UUID", b"id".to_vec());
        let ck = ClassKeyData::from_map(&map);
        assert_eq!(ck.wpky.unwrap(), b"wp".to_vec());
        assert_eq!(ck.wrap.unwrap(), b"wr".to_vec());
        assert_eq!(ck._uuid.unwrap(), b"id".to_vec());
    }
}
