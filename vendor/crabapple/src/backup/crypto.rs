//! Cryptographic routines for key derivation (`PBKDF2`), `AES` key wrap/unwrap, and `CBC` encryption/decryption.

use std::{
    collections::HashMap,
    io::{self, BufReader, Read},
};

use aes::{
    Aes256,
    cipher::{
        Array, BlockModeDecrypt, BlockModeEncrypt, KeyIvInit,
        block_padding::{Padding, Pkcs7},
        consts::{U16, U24, U32},
    },
};
use aes_kw::{KeyInit, KwAes128, KwAes192, KwAes256};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use sha2::Sha256;

use crate::{
    backup::models::{
        file::WrappedKey,
        keyring::{ClassKeyData, EncryptionKey, ProtectionClassKey},
        manifest::manifest_plist::ManifestData,
    },
    error::{BackupError, Result},
};

// Define CBC mode for AES-256
type Aes256CbcDec = cbc::Decryptor<Aes256>;
type Aes256CbcEnc = cbc::Encryptor<Aes256>;

/// Buffer size for batch-reading ciphertext (`8 KiB`)
pub const STREAM_BUFFER_SIZE: usize = 8 * 1024;

/// Derive the `32`-byte encryption key from a user password using `PBKDF2`.
///
/// # Arguments
/// * `password` - User-supplied password bytes.
/// * `dpsl` - `DPSL` parameter from the key bag for first `PBKDF2` pass.
/// * `dpic` - `DPIC` iteration count parameter for the first `PBKDF2` pass.
/// * `salt` - Salt from the backup key bag for second `PBKDF2` pass.
/// * `iter` - Iteration count for the second `PBKDF2` pass (`HMAC-SHA1`).
///
/// # Returns
/// A `32`-byte key for use in `AES`-based decryption.
///
/// # Errors
/// Never fails unless `PBKDF2` implementation panics.
///
/// # Examples
///
/// ```no_run
/// use crabapple::backup::crypto::derive_key_from_password;
///
/// let password = b"password";
/// let dpsl = b"salt1";
/// let dpic = 1000;
/// let salt = b"salt2";
/// let iter = 1000;
/// let key = derive_key_from_password(password, dpsl, dpic, salt, iter).unwrap();
///
/// assert_eq!(key.len(), 32);
/// ```
pub fn derive_key_from_password(
    password: &[u8],
    dpsl: &[u8],
    dpic: u32,
    salt: &[u8],
    iter: u32,
) -> Result<EncryptionKey> {
    let mut derived_pw = vec![0u8; 32]; // iOS backup key is 32 bytes (AES-256)
    let mut key = vec![0u8; 32]; // iOS backup key is 32 bytes (AES-256)

    // First PBKDF2 pass with SHA256
    pbkdf2_hmac::<Sha256>(password, dpsl, dpic, &mut derived_pw);

    // Second PBKDF2 pass with SHA1
    pbkdf2_hmac::<Sha1>(&derived_pw, salt, iter, &mut key);

    Ok(key.into())
}

/// Unwrap (decrypt) all protection class keys from the Manifest's key bag.
///
/// # Arguments
/// * `master_key` - The derived 32-byte master key (Kmaster).
/// * `manifest` - Parsed `Manifest.plist` containing the `key_ring`.
///
/// # Returns
/// A map of class ID to its unwrapped `AES` key.
///
/// # Errors
/// Returns [`BackupError::Crypto`] or [`BackupError::KeyUnwrapFailed`] if unwrapping fails.
pub(crate) fn unlock_keys_from_manifest(
    master_key: &EncryptionKey,
    manifest: &ManifestData,
) -> Result<HashMap<u32, ProtectionClassKey>> {
    if master_key.len() != 32 {
        return Err(BackupError::Crypto(format!(
            "Main key for unlocking class keys must be 32 bytes for AES-256, got {}",
            master_key.len()
        )));
    }
    let mut unlocked_keys = HashMap::new();
    let key_ring = manifest
        .key_ring
        .as_ref()
        .ok_or_else(|| BackupError::Crypto("BackupKeyBag not found in PlistInfo".to_string()))?;

    // Iterate over each class key in the key ring
    for (&class_id, class_key_data) in &key_ring.class_keys {
        match class_key_data {
            //  Unwrapped key data with wrapped key
            ClassKeyData {
                wpky: Some(wpky),
                wrap: Some(wrap_bytes),
                ..
            } => {
                // Ensure wrap_bytes is exactly 4 bytes (u32)
                let wrap_val = u32::from_be_bytes(
                    wrap_bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| BackupError::KeyUnwrapFailed(class_id))?,
                );
                // Check if the key is wrapped with AES Key Wrap
                if wrap_val & 0x02 == 0 {
                    // Not wrapped with AES Key Wrap, skip
                    continue;
                }

                // Create the Key Encryption Key (KEK) from the master key
                let unwrapped = aes_kw_unwrap(master_key, &WrappedKey::from(wpky.clone()))
                    .map_err(|_| BackupError::KeyUnwrapFailed(class_id))?;

                // Insert the unwrapped key into the map
                unlocked_keys.insert(
                    class_id,
                    ProtectionClassKey {
                        class_id,
                        key: unwrapped,
                    },
                );
            }
            _ => continue,
        }
    }
    Ok(unlocked_keys)
}

/// Decrypt data using `AES-256 CBC` with `PKCS7` padding and a zero IV.
///
/// # Arguments
/// * `data` - Encrypted ciphertext bytes.
/// * `key` - 32-byte AES key.
///
/// # Returns
/// The decrypted plaintext bytes.
///
/// # Errors
/// Returns [`BackupError::Crypto`] or [`BackupError::InvalidCryptoDataLength`] on failure.
///
/// # Examples
///
/// ```no_run
/// use crabapple::backup::crypto::{aes_encrypt_cbc_with_padding, aes_decrypt_cbc_with_padding};
///
/// let key = vec![0u8; 32].into();
/// let data = b"hello world";
/// let ciphertext = aes_encrypt_cbc_with_padding(data, &key).unwrap();
/// let plaintext = aes_decrypt_cbc_with_padding(&ciphertext, &key).unwrap();
///
/// assert_eq!(plaintext, data);
/// ```
pub fn aes_decrypt_cbc_with_padding(data: &[u8], key: &EncryptionKey) -> Result<Vec<u8>> {
    let key_array = <&Array<u8, U32>>::try_from(key.as_slice()).map_err(|_| {
        BackupError::InvalidCryptoDataLength {
            expected: 32,
            actual: key.len(),
        }
    })?;

    // Ensure data length is a multiple of 16 bytes (AES block size)
    let data_len = if data.len().is_multiple_of(16) {
        data.len()
    } else {
        data.len() - (data.len() % 16)
    };

    let iv: [u8; 16] = [0u8; 16];

    // Create buffer with truncated data if necessary
    let mut buf = if data.len() == data_len {
        data.to_vec()
    } else {
        data[..data_len].to_vec()
    };

    let cipher = Aes256CbcDec::new(key_array, &iv.into());

    let pt_len = cipher
        .decrypt_padded::<Pkcs7>(&mut buf)
        .map_err(|e| BackupError::Crypto(format!("AES CBC decryption error (padding): {e:?}")))?
        .len();

    buf.truncate(pt_len);
    Ok(buf)
}

/// Encrypt data using `AES-256 CBC` with `PKCS7` padding and a zero IV.
///
/// # Arguments
/// * `data` - Plaintext bytes.
/// * `key` - `32`-byte AES key.
///
/// # Returns
/// The ciphertext bytes.
///
/// # Errors
/// Returns [`BackupError::Crypto`] or [`BackupError::InvalidCryptoDataLength`] on failure.
///
/// # Examples
///
/// ```no_run
/// use crabapple::backup::crypto::{aes_encrypt_cbc_with_padding, aes_decrypt_cbc_with_padding};
///
/// let key = vec![0u8; 32].into();
/// let data = b"hello world";
/// let ct = aes_encrypt_cbc_with_padding(data, &key).unwrap();
/// let pt = aes_decrypt_cbc_with_padding(&ct, &key).unwrap();
///
/// assert_eq!(pt, data);
/// ```
pub fn aes_encrypt_cbc_with_padding(data: &[u8], key: &EncryptionKey) -> Result<Vec<u8>> {
    let key_array = <&Array<u8, U32>>::try_from(key.as_slice()).map_err(|_| {
        BackupError::InvalidCryptoDataLength {
            expected: 32,
            actual: key.len(),
        }
    })?;
    let iv: [u8; 16] = [0u8; 16];

    let mut buffer = vec![0u8; data.len() + 16]; // Max possible size for ciphertext with padding
    buffer[..data.len()].copy_from_slice(data);

    let cipher = Aes256CbcEnc::new(key_array, &iv.into());

    let ct_len = cipher
        .encrypt_padded::<Pkcs7>(&mut buffer, data.len())
        .map_err(|e| BackupError::Crypto(format!("AES CBC encryption error (padding): {e:?}")))?
        .len();

    buffer.truncate(ct_len);
    Ok(buffer)
}

/// Internal helper to unwrap `AES` Key Wrap (RFC 3394) based on key length.
///
/// # Arguments
/// * `kek_bytes` - Key Encryption Key (must be `16`, `24`, or `32` bytes).
/// * `wrapped_data` - Wrapped key data (must be at least `8` bytes).
///
/// # Returns
/// The unwrapped key data.
///
/// # Errors
/// Returns [`BackupError::Crypto`] if the unwrapping fails or input lengths are invalid.
pub(crate) fn aes_kw_unwrap(
    kek_bytes: &EncryptionKey,
    wrapped_data: &WrappedKey,
) -> Result<EncryptionKey> {
    if wrapped_data.len() <= 8 {
        return Err(BackupError::Crypto(format!(
            "Wrapped data is too short ({} bytes)",
            wrapped_data.len()
        )));
    }

    let mut unwrapped = vec![0u8; wrapped_data.len() - 8]; // Result is wrapped_len - 8 bytes
    match kek_bytes.len() {
        16 => {
            // AES-128 key unwrap
            let key = <&Array<u8, U16>>::try_from(kek_bytes.as_slice())
                .map_err(BackupError::ConversionFailed)?;
            let kek = KwAes128::new(key);
            kek.unwrap_key(wrapped_data, &mut unwrapped)
                .map_err(|_| BackupError::Crypto("AES 128 Key Unwrap failed".to_string()))?;
        }
        24 => {
            // AES-192 key unwrap
            let key = <&Array<u8, U24>>::try_from(kek_bytes.as_slice())
                .map_err(BackupError::ConversionFailed)?;
            let kek = KwAes192::new(key);
            kek.unwrap_key(wrapped_data, &mut unwrapped)
                .map_err(|_| BackupError::Crypto("AES 192 Key Unwrap failed".to_string()))?;
        }
        32 => {
            // AES-256 key unwrap
            let key = <&Array<u8, U32>>::try_from(kek_bytes.as_slice())
                .map_err(BackupError::ConversionFailed)?;
            let kek = KwAes256::new(key);
            kek.unwrap_key(wrapped_data, &mut unwrapped)
                .map_err(|_| BackupError::Crypto("AES 256 Key Unwrap failed".to_string()))?;
        }
        _ => {
            return Err(BackupError::Crypto(format!(
                "Invalid KEK length: {} bytes (must be 16, 24, or 32)",
                kek_bytes.len()
            )));
        }
    }
    Ok(unwrapped.into())
}

/// Streaming `AES-256-CBC` decryption reader with PKCS7 padding support.
///
/// `AesCbcDecryptReader` implements [`std::io::Read`], decrypting ciphertext on-the-fly
/// and yielding plaintext bytes to the caller.
///
/// This only ever holds two AES blocks (32 bytes) plus whatever the caller’s buffer is.
pub struct AesCbcDecryptReader<R: Read> {
    /// Underlying source of ciphertext data.
    inner: R,
    /// AES-CBC decryptor (maintains IV chaining state).
    cipher: Aes256CbcDec,
    /// Lookahead buffer holding the next `16`-byte ciphertext block.
    lookahead: [u8; 16],
    /// Decrypted plaintext bytes buffered for reading.
    buf: Vec<u8>,
    /// Current read offset within `buf`.
    buf_pos: usize,
    /// Indicates whether the final block (with padding) has been processed.
    eof: bool,
}

impl<R: Read> AesCbcDecryptReader<R> {
    /// Creates a streaming reader that decrypts `AES-256-CBC` encrypted data from any `Read` source.
    ///
    /// The returned `AesCbcDecryptReader` reads [`STREAM_BUFFER_SIZE`]-byte blocks from the underlying ciphertext reader,
    /// decrypts each block using `AES-256-CBC` with a zero IV, and applies `PKCS7` unpadding on the final block.
    /// Only two cipher blocks and one plaintext buffer are held in memory at once.
    ///
    /// # Arguments
    /// * `reader` - Source of ciphertext bytes implementing `Read`.
    /// * `key` - `32`-byte `AES-256` decryption key.
    ///
    /// # Returns
    /// A streaming reader implementing [`std::io::Read`] that yields plaintext as it's read.
    ///
    /// # Errors
    /// Returns [`BackupError::InvalidCryptoDataLength`] if `key` is not exactly `32` bytes.
    /// Returns [`BackupError::Crypto`] if I/O failure occurs or ciphertext is too short/invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, io::copy};
    /// use crabapple::backup::crypto::AesCbcDecryptReader;
    ///
    /// let file = File::open("encrypted.bin").unwrap();
    /// let mut reader = AesCbcDecryptReader::from(file, &vec![0; 32].into()).unwrap();
    /// let mut plaintext = Vec::new();
    /// copy(&mut reader, &mut plaintext).unwrap();
    /// ```
    pub fn from(reader: R, key: &EncryptionKey) -> Result<AesCbcDecryptReader<BufReader<R>>> {
        let key_array = <&Array<u8, U32>>::try_from(key.as_slice()).map_err(|_| {
            BackupError::InvalidCryptoDataLength {
                expected: 32,
                actual: key.len(),
            }
        })?;
        // Wrap source in buffered reader to reduce syscalls
        let mut buf_reader = BufReader::with_capacity(STREAM_BUFFER_SIZE, reader);
        // Read first cipher block into lookahead buffer
        let mut lookahead = [0u8; 16];
        let n = buf_reader
            .read(&mut lookahead)
            .map_err(|e| BackupError::Crypto(format!("I/O error: {e}")))?;
        if n == 0 {
            return Err(BackupError::Crypto("Ciphertext empty".into()));
        }
        if n != 16 {
            return Err(BackupError::Crypto(format!(
                "Unexpected ciphertext length: {n}"
            )));
        }
        let iv: [u8; 16] = [0u8; 16];
        let cipher = Aes256CbcDec::new(key_array, &iv.into());
        Ok(AesCbcDecryptReader {
            inner: buf_reader,
            cipher,
            lookahead,
            buf: Vec::new(),
            buf_pos: 0,
            eof: false,
        })
    }
}

impl<R: Read> Read for AesCbcDecryptReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        // Loop until output buffer is full or EOF
        while written < out.len() {
            // Flush any pending plaintext bytes
            if self.buf_pos < self.buf.len() {
                let to_copy = (self.buf.len() - self.buf_pos).min(out.len() - written);
                out[written..written + to_copy]
                    .copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + to_copy]);
                self.buf_pos += to_copy;
                written += to_copy;
                continue;
            }
            // No pending data
            if self.eof {
                break;
            }
            // Batch-read ciphertext into buffer
            let mut chunk = vec![0u8; STREAM_BUFFER_SIZE];
            let n = self.inner.read(&mut chunk)?;
            if n == 0 {
                // Final block: decrypt & unpad last lookahead block
                let mut block: Array<u8, U16> = self.lookahead.into();
                self.cipher.decrypt_block(&mut block);
                let pt = Pkcs7::unpad(&block).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("Padding error: {e:?}"))
                })?;
                self.buf.clear();
                self.buf.extend_from_slice(pt);
                self.buf_pos = 0;
                self.eof = true;
            } else {
                // Combine previous lookahead with new chunk
                let mut data = self.lookahead.to_vec();
                data.extend_from_slice(&chunk[..n]);
                // Compute number of full blocks
                let total = data.len();
                let num_blocks = total / 16;
                if num_blocks == 0 {
                    // Not enough data to decrypt a block yet; save back
                    // (shouldn't happen since lookahead is a full block)
                } else {
                    // Decrypt all except the last block for chaining/padding
                    self.buf.clear();
                    for chunk in data[..(num_blocks - 1) * 16].chunks_exact(16) {
                        let block = <&Array<u8, U16>>::try_from(chunk).map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("block conversion: {e}"),
                            )
                        })?;
                        let mut block = *block;
                        self.cipher.decrypt_block(&mut block);
                        self.buf.extend_from_slice(&block);
                    }
                    // Update lookahead to last full block
                    let last_start = (num_blocks - 1) * 16;
                    self.lookahead
                        .copy_from_slice(&data[last_start..last_start + 16]);
                    self.buf_pos = 0;
                }
            }
            // Loop to flush newly decrypted data
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_kw::{KwAes128, KwAes192, KwAes256};

    #[test]
    fn test_derive_key_consistency() {
        let salt = b"saltsalt";
        let key1 = derive_key_from_password(b"password", &[], 0, salt, 1000).unwrap();
        let key2 = derive_key_from_password(b"password", &[], 0, salt, 1000).unwrap();
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_aes_cbc_roundtrip() {
        let key = vec![0x42; 32].into();
        let data = b"The quick brown fox jumps over the lazy dog";
        let ciphertext = aes_encrypt_cbc_with_padding(data, &key).unwrap();
        assert_ne!(ciphertext, data);
        let plaintext = aes_decrypt_cbc_with_padding(&ciphertext, &key).unwrap();
        assert_eq!(plaintext, data);
    }

    fn wrap_and_unwrap(kek_bytes: &EncryptionKey, plain: &[u8]) {
        let mut wrapped = vec![0u8; plain.len() + 8];
        match kek_bytes.len() {
            16 => {
                let key: [u8; 16] = kek_bytes.as_slice().try_into().unwrap();
                let kek = KwAes128::new(&key.into());
                kek.wrap_key(plain, &mut wrapped).unwrap();
            }
            24 => {
                let key: [u8; 24] = kek_bytes.as_slice().try_into().unwrap();
                let kek = KwAes192::new(&key.into());
                kek.wrap_key(plain, &mut wrapped).unwrap();
            }
            32 => {
                let key: [u8; 32] = kek_bytes.as_slice().try_into().unwrap();
                let kek = KwAes256::new(&key.into());
                kek.wrap_key(plain, &mut wrapped).unwrap();
            }
            _ => panic!("Invalid KEK length"),
        }
        let unwrapped = aes_kw_unwrap(kek_bytes, &wrapped.into()).unwrap();
        assert_eq!(unwrapped, plain.to_vec().into());
    }

    #[test]
    fn test_key_wrap_unwrap_128() {
        let kek = vec![0x0b; 16].into();
        let data = b"12345678ABCDEFGH";
        wrap_and_unwrap(&kek, data);
    }

    #[test]
    fn test_key_wrap_unwrap_192() {
        let kek = vec![0x0c; 24].into();
        let data = b"12345678ABCDEFGH";
        wrap_and_unwrap(&kek, data);
    }

    #[test]
    fn test_key_wrap_unwrap_256() {
        let kek = vec![0x0d; 32].into();
        let data = b"12345678ABCDEFGH";
        wrap_and_unwrap(&kek, data);
    }

    #[test]
    fn test_aes_kw_unwrap_errors() {
        // Wrapped data too short
        let kek = vec![0u8; 16].into();
        let short_data = vec![0u8; 8];
        let err = aes_kw_unwrap(&kek, &WrappedKey::from(short_data)).unwrap_err();
        match err {
            BackupError::Crypto(msg) => assert!(msg.contains("too short")),
            _ => panic!("Expected Crypto error for short data"),
        }
        // Invalid KEK length
        let invalid_kek = vec![0u8; 10].into();
        let wrapped = vec![0u8; 16];
        let err2 = aes_kw_unwrap(&invalid_kek, &WrappedKey::from(wrapped)).unwrap_err();
        match err2 {
            BackupError::Crypto(msg) => assert!(msg.contains("Invalid KEK length")),
            _ => panic!("Expected Crypto error for invalid KEK length"),
        }
    }

    #[test]
    fn test_aes_encrypt_invalid_key_length() {
        let data = b"hello";
        // key too short
        let short_key = vec![0u8; 16].into();
        let err = aes_encrypt_cbc_with_padding(data, &short_key).unwrap_err();
        match err {
            BackupError::InvalidCryptoDataLength {
                actual,
                expected: _,
            } => assert_eq!(actual, 16),
            _ => panic!("Expected InvalidCryptoDataLength for short key"),
        }
        // key too long
        let long_key = vec![0u8; 64].into();
        let err2 = aes_encrypt_cbc_with_padding(data, &long_key).unwrap_err();
        match err2 {
            BackupError::InvalidCryptoDataLength {
                actual,
                expected: _,
            } => assert_eq!(actual, 64),
            _ => panic!("Expected InvalidCryptoDataLength for long key"),
        }
    }

    #[test]
    fn test_aes_decrypt_invalid_key_length() {
        let cipher = vec![0u8; 16];
        let short_key = vec![0u8; 24].into();
        let err = aes_decrypt_cbc_with_padding(&cipher, &short_key).unwrap_err();
        match err {
            BackupError::InvalidCryptoDataLength { actual, expected } => {
                assert_eq!(actual, 24);
                assert_eq!(expected, 32);
            }
            _ => panic!("Expected InvalidCryptoDataLength with actual=24, expected=32"),
        }
    }

    #[test]
    fn test_derive_key_length_and_determinism() {
        let password = b"password";
        let dpsl = b"salt1";
        let dpic = 2;
        let salt = b"salt2";
        let iter = 3;
        let key1 = derive_key_from_password(password, dpsl, dpic, salt, iter).unwrap();
        let key2 = derive_key_from_password(password, dpsl, dpic, salt, iter).unwrap();
        // Key must be 32 bytes and deterministic
        assert_eq!(key1.len(), 32);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_aes_encrypt_decrypt_empty_data() {
        // AES-256 key of zeros
        let key = vec![0u8; 32].into();
        // Encrypt empty plaintext
        let ciphertext = aes_encrypt_cbc_with_padding(&[], &key).unwrap();
        // Even empty plaintext should produce one full block of padding
        assert_eq!(ciphertext.len(), 16);
        // Decrypt back
        let plaintext = aes_decrypt_cbc_with_padding(&ciphertext, &key).unwrap();
        assert_eq!(plaintext.len(), 0);
    }

    #[test]
    fn test_aes_decrypt_trims_non_multiple_of_block_size() {
        // Prepare a valid ciphertext for "hello"
        let key = vec![0u8; 32].into();
        let original = b"hello";
        let mut ciphertext = aes_encrypt_cbc_with_padding(original, &key).unwrap();
        // Append extra bytes that should be ignored
        ciphertext.extend(&[0u8; 5]);
        // Decrypt will truncate to a multiple of block size
        let plaintext = aes_decrypt_cbc_with_padding(&ciphertext, &key).unwrap();
        assert_eq!(plaintext, original);
    }
}
