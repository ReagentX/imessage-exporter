use std::{
    env::temp_dir,
    fs::File,
    io::{BufWriter, Write, copy},
    path::{Path, PathBuf},
};

use crabapple::{Authentication, Backup};
use imessage_database::{tables::table::DEFAULT_PATH_IOS, util::platform::Platform};

use crate::app::{error::RuntimeError, options::Options};

const MAX_IN_MEMORY_DECRYPT: u64 = 25 * 1024 * 1024;

/// Decrypt the iOS backup, if necessary
pub fn decrypt_backup(options: &Options) -> Result<Option<Backup>, RuntimeError> {
    let (Platform::iOS, Some(pw)) = (&options.platform, &options.cleartext_password) else {
        return Ok(None);
    };

    eprintln!("Decrypting iOS backup...");
    eprintln!("  [1/3] Deriving backup keys...");
    let auth = Authentication::Password(pw.clone());
    let backup = Backup::open(options.db_path.clone(), &auth)?;

    Ok(Some(backup))
}

pub fn get_decrypted_message_database(backup: &Backup) -> Result<PathBuf, RuntimeError> {
    let (_, file_id) = DEFAULT_PATH_IOS.split_at(3);
    eprintln!("  [2/3] Resolving messages database...");
    let file = backup.get_file(file_id)?;
    let mut decrypted_chat_db = backup.decrypt_entry_stream(&file)?;

    // Write decrypted sms.db into a platform-specific temporary directory
    let tmp_path = temp_dir().join("crabapple-sms.db");
    let mut file = File::create(&tmp_path)?;

    // Stream-decrypt directly into the temp file
    eprintln!("  [3/3] Decrypting messages database...");
    copy(&mut decrypted_chat_db, &mut file)?;

    eprintln!(
        "Decrypted iOS backup: {} (version {})\n",
        backup.lockdown().device_name,
        backup.lockdown().product_version,
    );
    Ok(tmp_path)
}

/// Decrypt a file from the iOS backup
pub fn decrypt_file(backup: &Backup, from: &Path) -> Result<PathBuf, RuntimeError> {
    match backup.get_file(
        from.file_name()
            .ok_or(RuntimeError::FileNameError)?
            .to_str()
            .ok_or(RuntimeError::FileNameError)?,
    ) {
        Ok(file) => {
            let temp_dir = temp_dir().join(&file.file_id);
            let mut temp_file = File::create(&temp_dir)?;

            // Get the size of the file
            let file_size = file.metadata.size;
            // If the file is larger than 25MB, we will stream decryption from/to disk
            // otherwise, we will decrypt in memory
            if file_size > MAX_IN_MEMORY_DECRYPT {
                // Copy via disk
                let mut decryption_stream = backup.decrypt_entry_stream(&file)?;
                let mut writer = BufWriter::new(temp_file);

                // Copy all data from reader to writer
                copy(&mut decryption_stream, &mut writer)?;

                // Ensure all buffered data is flushed to disk
                writer.flush()?;
            } else {
                // Copy via memory
                let decrypted_bytes = backup.decrypt_entry(&file)?;
                temp_file.write_all(&decrypted_bytes)?;
            }

            // Ensure we remove the temporary file later
            Ok(temp_dir)
        }
        Err(why) => Err(RuntimeError::BackupError(why)),
    }
}
