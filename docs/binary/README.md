# Binary Documentation

The `imessage_exporter` binary exports iMessage data to `txt`, `csv`, `pdf`, or `html` formats. It can also run diagnostics to find problems with the iMessage database.

## Installation

`cargo install logria` is the best way to install the app for normal use. (Not supported in Alpha Stage)

### Installing as a standalone app

- `clone` the repository
- `cd` to the repository
- `cargo test` to make sure everything works
- `cargo run --release` to compile

## How To Use

```
    -d, --diagnostics
            Print diagnostic information and exit

    -e, --export <txt, csv, pdf, html>
            Specify a single file format to export messages into

    -h, --help
            Print help information

    -n, --no-copy
            Do not copy attachments, instead reference them in-place

    -o, --export-path <path/to/save/files>
            Specify a custom directory for outputting exported data

    -p, --db-path <path/to/chat.db>
            Specify a custom path for the iMessage database file

    -V, --version
            Print version information
```
