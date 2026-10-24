# crabapple

`crabapple` is a Rust library for reading, inspecting, and extracting data from encrypted iOS backups created by Finder, Apple Devices, or iTunes.

Inspired by [`imessage-exporter`](https://github.com/ReagentX/imessage-exporter), `crabapple` provides a flexible foundation for any project that needs to access iOS backup data.

## Features

- Load and parse the backup's `Manifest.plist` to obtain metadata, device info, and encryption parameters
- Derive encryption keys using `PBKDF2` (`HMAC-SHA256` then `HMAC-SHA1`) and unwrap protection class keys via AES Key Wrap (`RFC 3394`)
- Decrypt and query the `AES-256` encrypted `Manifest.db`, exposing backup file metadata via `rusqlite`
- Retrieve and decrypt individual files by protection class (per-file `AES-CBC` with `PKCS7` padding)
- Cross-platform support for macOS, Windows, and Linux

## Installation

This library is available on [crates.io](https://crates.io/crates/crabapple).

## Documentation

Documentation is available on [docs.rs](https://docs.rs/crabapple).

## Quick Start

```rust,no_run
use std::{io::copy, fs::File};

use crabapple::{Backup, Authentication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize a backup session for a device UDID with a password
    let udid_folder = "/Users/you/Library/Application Support/MobileSync/Backup/DEVICE_UDID";
    let auth = Authentication::Password("your_password".into());
    let backup = Backup::open(udid_folder, &auth)?;

    // List all files in the backup
    let entries = backup.entries()?;
    for entry in &entries {
        println!("{} - {}/{}", entry.file_id, entry.domain, entry.relative_path);
    }

    // Decrypt and read a file entry as a stream
    if let Some(entry) = entries.first() {
        let mut stream = backup.decrypt_entry_stream(&entry)?;
        // Write the stream to a file
        let mut file = File::create("decrypted.txt")?;
        copy(&mut stream, &mut file)?;
    }

    // Alternatively, decrypt and read a file entry into memory
    if let Some(entry) = entries.get(2) {
        let data = backup.decrypt_entry(&entry)?;
        println!("Decrypted {} ({} bytes)", entry.relative_path, data.len());
    }

    // Get the derived key for use elsewhere:
    let derived_key = backup.decryption_key_hex();

    Ok(())
}
```

### Using a Pre-derived Key

Pre-derived keys bypass the expensive key derivation process:

```rust,no_run
use crabapple::{Backup, Authentication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let udid_folder = "/path/to/backup";
    let hex_key = "abcdef0123456789...";
    let auth = Authentication::DerivedKey(hex_key.to_string());
    let backup = Backup::open(udid_folder, &auth)?;
    // ... proceed as normal
    Ok(())
}
```

### Getting Basic Device Information

You can retrieve device metadata (like device name, iOS version, and UDID) without opening the full backup database:

```rust,no_run
use std::path::Path;

use crabapple::backup::device::get_device_basic_info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let udid_folder = Path::new("/Users/you/Library/Application Support/MobileSync/Backup/DEVICE_UDID");
    let info = get_device_basic_info(udid_folder)?;
    println!("Device: {} (iOS {})", info.device_name, info.product_version);
    println!("UDID: {}", info.unique_device_id);
    Ok(())
}
```

This information is also present on a decrypted `Backup` instance:

```rust,no_run
use crabapple::{Backup, Authentication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let udid_folder = "/path/to/backup";
    let hex_key = "abcdef0123456789...";
    let auth = Authentication::DerivedKey(hex_key.to_string());
    let backup = Backup::open(udid_folder, &auth)?;
    
    println!("Device: {} (iOS {})",
        backup.lockdown().device_name,
        backup.lockdown().product_version
    );
    println!("UDID: {}", backup.udid()?);
    Ok(())
}
```

### Error Handling

`crabapple` uses a custom `BackupError` enum for error reporting. You can match on specific cases:

```rust,no_run
use crabapple::{Backup, Authentication};
use crabapple::error::BackupError;

match Backup::open("/bad/path", &Authentication::Password("pass".into())) {
    Ok(b) => println!("Loaded backup successfully"),
    Err(BackupError::ManifestPlistNotFound(path)) => eprintln!("Missing Manifest.plist: {}", path),
    Err(err) => eprintln!("Error initializing backup: {}", err),
}
```

## Targeted Versions

This library targets the current latest public release for iOS. It should work with backups from iOS 10.2 or later, but all features may not be available.

## Crabapple Tree

![My Crabapple Tree](/resources/crabapple.png)
