# Binary Documentation

The `imessage_exporter` binary exports iMessage data to `txt`, or `html` formats. It can also run diagnostics to find problems with the iMessage database.

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

-f, --format <txt, html>
        Specify a single file format to export messages into

-h, --help
        Print help information

-n, --no-copy
        Do not copy attachments, instead reference them in-place

-o, --export-path <path/to/save/files>
        Specify a custom directory for outputting exported data
        If omitted, the defaut directory is ~/imessage_export

-p, --db-path <path/to/chat.db>
        Specify a custom path for the iMessage database file
        If omitted, the defaut directory is ~/Library/Messages/chat.db

-V, --version
        Print version information
```

## Caveats

### HTML Exports

In HTML exports in Safari, when referencing files in-place, you must permit Safari to read from the local file system in the Develop menu:

![](/docs/binary/img/safari_local_file_restrictions.png)

Further, since the files are stored in `~/Library`, you will need to grant your browser Full Disk Access in System Preferences.

### PDF Exports

I could not get PDF export to work in a reasonable way. The best way for a user to do this is to follow the steps above for Safari and print to PDF.

#### `wkhtmltopdf`

`wkhtmltopdf` refuses to render local images, even with the flag enabled like so:

```rust
let mut process = Command::new("wkhtmltopdf")
.args(&vec![
    "--enable-local-file-access".to_string(),
    html_path,
    pdf_path.to_string_lossy().to_string(),
])
.spawn()
.unwrap();
```

This persisted after granting `cargo`, `imessage-exporter`, and `wkhtmltopdf` Full Disk Access permissions as well as after copying files to the same directory as the `HTML` file.

#### Browser Automation

There are several `chomedriver` wrappers for Rust. The ones that use async make this binary too large (over `10mb`) and have too many dependencies. The sync implmentation in the `headless-chrome` crate works, but [times out](https://github.com/atroche/rust-headless-chrome/issues/319) when generating large `PDF`s, even with an extreme timeout.
