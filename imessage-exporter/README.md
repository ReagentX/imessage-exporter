# Binary Documentation

The `imessage-exporter` binary exports iMessage data to `txt` or `html` formats. It can also run diagnostics to find problems with the iMessage database.

## Installation

There are several ways to install this software.

### Cargo (recommended)

This binary is available on [crates.io](https://crates.io/crates/imessage-exporter).

`cargo install imessage-exporter` is the best way to install the app for normal use.

### Prebuilt Binaries

The [releases page](https://github.com/ReagentX/imessage-exporter/releases) provides prebuilt binaries for both Apple Silicon and Intel-based Macs.

### Installing manually

- `clone` the repository
- `cd` to the repository
- `cargo run --release` to compile

## How To Use

```txt
-d, --diagnostics
        Print diagnostic information and exit

-f, --format <txt, html>
        Specify a single file format to export messages into

-n, --no-copy
        Do not copy attachments, instead reference them in-place

-p, --db-path <path/to/chat.db>
        Specify a custom path for the iMessage database file
        If omitted, the default directory is ~/Library/Messages/chat.db

-o, --export-path <path/to/save/files>
        Specify a custom directory for outputting exported data
        If omitted, the default directory is ~/imessage_export

-s, --start-date <YYYY-MM-DD>
        The start date filter. Only messages sent on or after this date will be included

-e, --end-date <YYYY-MM-DD>
        The end date filter. Only messages sent before this date will be included

-l, --no-lazy
        Do not include `loading="lazy"` in HTML export `img` tags
        This will make pages load slower but PDF generation work

-m, --custom-name <custom-name>
        Specify an optional custom name for the database owner's messages in exports

-h, --help
        Print help information

-V, --version
        Print version information
```

### Examples

Export as `html` and copy attachments from the default iMessage Database location to your home directory:

```zsh
% imessage-exporter -f html
```

Export as `txt` from the default iMessage Database location to a new folder in the current working directory called `output`:

```zsh
% imessage-exporter -f txt -o output
```

Export as `html` from `/Volumes/external/chat.db` to `/Volumes/external/export` without copying attachments:

```zsh
% imessage-exporter -f html --no-copy -p /Volumes/external/chat.db -o /Volumes/external/export
```

Export messages from `2020-01-01` to `2020-12-31` as `txt` from the default iMessage Database location to `~/export-2020`:

```zsh
% imessage-exporter -f txt -o ~/export-2020 -s 2020-01-01 -e 2021-01-01
```

## Features

[Click here](../docs/features.md) for a full list of features.

## Caveats

### HTML Exports

In HTML exports in Safari, when referencing files in-place, you must permit Safari to read from the local file system in the Develop menu:

![](../docs/binary/img/safari_local_file_restrictions.png)

Further, since the files are stored in `~/Library`, you will need to grant your browser Full Disk Access in System Settings.

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

There are several `chomedriver` wrappers for Rust. The ones that use async make this binary too large (over `10mb`) and have too many dependencies. The sync implementation in the `headless-chrome` crate works, but [times out](https://github.com/atroche/rust-headless-chrome/issues/319) when generating large `PDF`s, even with an extreme timeout.
