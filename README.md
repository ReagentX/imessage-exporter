# imessage-exporter

This crate provides both a library to interact with iMessage data as well as a binary that can perform some useful read-only operations using that data.

![HTML Export Sample](/docs/hero.png)

## Binary

The `imessage-exporter` binary exports iMessage data to `txt` or `html` formats. It can also run diagnostics to find problems with the iMessage database.

Installation instructions for the binary are located [here](imessage-exporter/README.md).

## Library

The `imessage_database` library provides models that allow us to access iMessage information as native data structures.

Documentation for the library is located [here](imessage-database/README.md).

### Supported Features

This crate supports every iMessage feature as of macOS 13.5.2 (22G91) and iOS 16.6.1 (20G81):

- Multi-part messages
- Replies/Threads
- Attachments
- Expressives
- Reactions
- Stickers
- Apple Pay
- URL Previews
- App Integrations
- Edited messages

See more detail about supported features [here](docs/features.md).

## Frequently Asked Questions

The FAQ document is located [here](/docs/faq.md).

## Special Thanks

- All of my friends, for putting up with me sending them random messages to test things
- [SQLiteFlow](https://www.sqliteflow.com), the SQL viewer I used to explore and reverse engineer the iMessage database
- [Xplist](https://github.com/ic005k/Xplist), an invaluable tool for reverse engineering the `payload_data` plist format
- [Compart](https://www.compart.com/en/unicode/), an amazing resource for looking up esoteric unicode details
