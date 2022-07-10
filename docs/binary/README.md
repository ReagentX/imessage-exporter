# Binary Documentation

The `imessage_exporter` binary exports iMessage data to `txt`, `csv`, `pdf`, or `html` formats. It can also run diagnostics to find problems with the iMessage database.

## Installation

`cargo install imessage_exporter` is the best way to install the app for normal use. (Not supported in Alpha Stage)

### Installing as a standalone app

- `clone` the repository
- `cd` to the repository
- `cargo test` to make sure everything works
- `cargo run --release` to compile

## How To Use

```txt
-d, --diagnostics
        Print diagnostic information and exit

-f, --format <txt, csv, pdf, html>
        Specify a single file format to export messages into

-h, --help
        Print help information

-n, --no-copy
        Do not copy attachments, instead reference them in-place

-o, --export-path <path/to/save/files>
        Specify a custom directory for outputting exported data
        If omitted, the defaut directory is /Users/chris/imessage_export

-p, --db-path <path/to/chat.db>
        Specify a custom path for the iMessage database file
        If omitted, the defaut directory is /Users/chris/Library/Messages/chat.db

-V, --version
        Print version information
```

## Caveats

In HTML exports in Safari, when referencing files in-place, you must permit Safari to read from the local file system in the Develop menu:

![](/docs/binary/img/safari_local_file_restrictions.png)

Further, since the files are stored in `~/Library`, you will need to grant your browser Full Disk Access in System Preferences.
