# imessage-export

This crate provides both a library to interact with iMessage data as well as a binary that can perform some useful read-only operations using that data.

![](/docs/hero.png)

## Runtime

The `imessage_exporter` binary exports iMessage data to `txt` or `html` formats. It can also run diagnostics to find problems with the iMessage database.

Docs for the app are located [here](/docs/binary/).

## Library

The `imessage_database` library provides models that allow us to access iMessage information as native data structures.

Docs for the library are located [here](/docs/library/).

### Supported Features

This crate supports every iMessage feature as of MacOS 12.4 (21F79):

- Multi-part messages
- Replies/Threads
- Attachments
- Expressives
- Reactions
- Stickers
- Apps
  - Apple Pay

## Documentation

Documentation is available [here](/docs/).
