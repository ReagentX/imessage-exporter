use std::{
    io::{IsTerminal, Read, copy, stdin},
    path::Path,
};
use tempfile::NamedTempFile;

use crabapple::{
    Authentication, Backup, backup::models::manifest::manifest_plist::ManifestData,
    error::BackupError,
};
use imessage_database::{tables::table::DEFAULT_PATH_IOS, util::platform::Platform};

use crate::app::{
    contacts,
    error::RuntimeError,
    options::{OPTION_CLEARTEXT_PASSWORD, Options},
};

/// Own the temporary file before any plaintext is written, including on errors.
fn private_temp(reader: &mut impl Read, directory: &Path) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new_in(directory)?;
    copy(reader, &mut file)?;
    file.as_file().sync_all()?;
    Ok(file)
}

/// Open the iOS backup, prompting for a password if it is encrypted and one was not provided.
///
/// Returns `Ok(None)` for non-iOS platforms or for unencrypted iOS backups.
pub fn decrypt_backup(options: &Options) -> Result<Option<Backup>, RuntimeError> {
    if !matches!(options.platform, Platform::iOS) {
        return Ok(None);
    }

    // Reading Manifest.plist is cheap and tells us whether the backup is encrypted
    // without needing to derive any keys.
    let manifest_data = ManifestData::from_plist(options.db_path.join("Manifest.plist"))?;

    if !manifest_data.is_encrypted {
        if options.cleartext_password.is_some() {
            return Err(RuntimeError::InvalidOptions(format!(
                "--{OPTION_CLEARTEXT_PASSWORD} was provided, but the iOS backup at {} is not encrypted.",
                options.db_path.display()
            )));
        }
        return Ok(None);
    }

    let password = match options.cleartext_password.as_deref() {
        Some(pw) => pw.to_string(),
        None => prompt_for_password()?,
    };

    eprintln!("Decrypting iOS backup...");
    eprintln!("  [1/5] Deriving backup keys...");
    let backup = match Backup::open(options.db_path.clone(), &Authentication::Password(password)) {
        Ok(backup) => backup,
        Err(BackupError::PasswordOrKeyIncorrect) => {
            return Err(RuntimeError::InvalidOptions(
                "The iOS backup password was incorrect.".to_string(),
            ));
        }
        Err(other) => return Err(other.into()),
    };

    Ok(Some(backup))
}

/// Prompt the user for the backup password, reading from the controlling terminal.
fn prompt_for_password() -> Result<String, RuntimeError> {
    if !stdin().is_terminal() {
        return Err(RuntimeError::InvalidOptions(format!(
            "No terminal available to prompt for the iOS backup password; pass --{OPTION_CLEARTEXT_PASSWORD} for non-interactive use."
        )));
    }
    eprintln!("Encrypted iOS backup detected. Enter password (input hidden):");
    rpassword::prompt_password("> ").map_err(|e| {
        RuntimeError::InvalidOptions(format!(
            "Unable to read password interactively ({e}); pass --{OPTION_CLEARTEXT_PASSWORD} for non-interactive use."
        ))
    })
}

/// Write the decrypted Messages database from the iOS backup to a temp file.
///
/// The real name is `Library/SMS/sms.db`
pub fn get_decrypted_message_database(backup: &Backup) -> Result<NamedTempFile, RuntimeError> {
    let (_, file_id) = DEFAULT_PATH_IOS.split_at(3);
    eprintln!("  [2/5] Resolving messages database...");
    let file = backup.get_file(file_id)?;
    let mut decrypted_chat_db = backup.decrypt_entry_stream(&file)?;

    eprintln!("  [3/5] Decrypting messages database...");
    Ok(private_temp(&mut decrypted_chat_db, &std::env::temp_dir())?)
}

/// Write the decrypted Contacts database from the iOS backup to a temp file.
///
/// The real name is `Library/AddressBook/AddressBook.sqlitedb`
pub fn get_decrypted_contacts_database(backup: &Backup) -> Result<NamedTempFile, RuntimeError> {
    let (_, file_id) = contacts::DEFAULT_PATH_IOS.split_at(3);
    eprintln!("  [4/5] Resolving contacts database...");
    let file = backup.get_file(file_id)?;
    let mut decrypted_contacts_db = backup.decrypt_entry_stream(&file)?;

    eprintln!("  [5/5] Decrypting contacts database...");
    Ok(private_temp(
        &mut decrypted_contacts_db,
        &std::env::temp_dir(),
    )?)
}

/// Decrypt one iOS backup file into a temporary file.
pub fn decrypt_file(backup: &Backup, from: &Path) -> Result<NamedTempFile, RuntimeError> {
    match backup.get_file(
        from.file_name()
            .ok_or_else(|| RuntimeError::FileNameError {
                path: from.to_path_buf(),
                reason: "path has no file name component",
            })?
            .to_str()
            .ok_or_else(|| RuntimeError::FileNameError {
                path: from.to_path_buf(),
                reason: "file name is not valid UTF-8",
            })?,
    ) {
        Ok(file) => {
            let mut plaintext = backup.decrypt_entry_stream(&file)?;
            Ok(private_temp(&mut plaintext, &std::env::temp_dir())?)
        }
        Err(why) => Err(why.into()),
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    #[test]
    fn private_plaintext_is_unique_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let a = private_temp(&mut &b"one"[..], dir.path()).unwrap();
        let b = private_temp(&mut &b"two"[..], dir.path()).unwrap();
        assert_ne!(a.path(), b.path());
        assert_eq!(std::fs::read(a.path()).unwrap(), b"one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                a.as_file().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let path = a.path().to_path_buf();
        drop(a);
        assert!(!path.exists());
    }
    #[test]
    fn failed_plaintext_stream_is_removed() {
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.0 {
                    return Err(std::io::Error::other("synthetic decryption failure"));
                }
                self.0 = true;
                out[0] = 42;
                Ok(1)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(private_temp(&mut Broken(false), dir.path()).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
