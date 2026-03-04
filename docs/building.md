# Building imessage-exporter

This document provides instructions for building the `imessage-database` library and `imessage-exporter` binary from source.

## Prerequisites

### Rust Toolchain

This project requires the Rust toolchain. Install it from [rustup.rs](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This project uses Rust edition 2024. Ensure you have a recent stable version of Rust:

```bash
rustup update stable
```

### Optional Dependencies

For full functionality, especially attachment format conversion:

#### macOS
- **ImageMagick** (for HEIC image conversion on non-macOS platforms when running in compatibility mode)
- **ffmpeg** (for audio CAF→MP4 and video MOV→MP4 conversion)

```bash
brew install imagemagick ffmpeg
```

#### Linux
```bash
# Debian/Ubuntu
sudo apt install imagemagick ffmpeg

# Fedora
sudo dnf install ImageMagick ffmpeg

# Arch
sudo pacman -S imagemagick ffmpeg
```

#### Windows
- Download [ImageMagick](https://imagemagick.org/script/download.php)
- Download [ffmpeg](https://ffmpeg.org/download.html)
- For cross-compiling Windows binaries on macOS: `brew install mingw-w64`

## Getting the Source

Clone the repository:

```bash
git clone https://github.com/ReagentX/imessage-exporter.git
cd imessage-exporter
```

## Building the Library

The `imessage-database` library can be built standalone:

```bash
cargo build -p imessage-database
```

### Release Build

```bash
cargo build -p imessage-database --release
```

Release builds use Link Time Optimization (LTO) and single codegen units for maximum performance, as configured in the workspace `Cargo.toml`.

### Running Library Tests

```bash
cargo test -p imessage-database
```

**Note:** Some tests may fail if run outside of macOS or without a valid iMessage database, as they test platform-specific functionality.

### Using the Library in Your Project

Add to your `Cargo.toml`:

```toml
[dependencies]
imessage-database = "3.3.2"  # Use the latest version from crates.io
```

## Building the Binary

Build the `imessage-exporter` binary:

```bash
cargo build -p imessage-exporter
```

The compiled binary will be located at:
- Debug: `target/debug/imessage-exporter`
- Release: `target/release/imessage-exporter`

### Release Build

For optimized performance:

```bash
cargo build -p imessage-exporter --release
```

### Running the Binary

After building, run the binary directly:

```bash
# Debug build
./target/debug/imessage-exporter --help

# Release build
./target/release/imessage-exporter --help

# Or use cargo run
cargo run --release -p imessage-exporter -- --help
```

### Running Binary Tests

```bash
cargo test -p imessage-exporter
```

**Note:** Some tests may fail if run outside of macOS or without a valid iMessage database, as they test platform-specific functionality and real message data.

## Building the Entire Workspace

Build both the library and binary together:

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Running All Tests

```bash
cargo test
```

### Running Linter

```bash
cargo clippy
```

## Cross-Compilation

The project supports cross-compilation to multiple targets.

### macOS Targets

#### Apple Silicon (ARM64)
```bash
rustup target add aarch64-apple-darwin
cargo build --target aarch64-apple-darwin --release
```

#### Intel (x86_64)
```bash
rustup target add x86_64-apple-darwin
cargo build --target x86_64-apple-darwin --release
```

### Windows Target (from macOS)

Requires `mingw-w64`:

```bash
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

### Linux Targets

```bash
# For cross-compiling to Linux from macOS
rustup target add x86_64-unknown-linux-gnu
cargo build --target x86_64-unknown-linux-gnu --release
```

## Development Tips

### Faster Incremental Builds

For development, debug builds are significantly faster than release builds. Use them for rapid iteration:

```bash
cargo build
cargo test
```

### Running Specific Tests

```bash
# Run tests matching a pattern
cargo test <test_name>

# Run tests in a specific module
cargo test --test <test_module>

# Run a specific test with output
cargo test <test_name> -- --nocapture
```

### Checking Code Without Building

```bash
cargo check
```

This is faster than `cargo build` and useful for quick syntax/type checking.

### Watching for Changes

Install `cargo-watch` for automatic rebuilds:

```bash
cargo install cargo-watch
cargo watch -x check -x test
```

## Release Builds

The `build.sh` script in the repository root automates the release build process:

1. Extracts version from git tags
2. Updates version numbers in `Cargo.toml` files
3. Optionally publishes to crates.io (with `PUBLISH` env var)
4. Builds for multiple targets:
   - `aarch64-apple-darwin` (Apple Silicon)
   - `x86_64-apple-darwin` (Intel macOS)
   - `x86_64-pc-windows-gnu` (Windows)
5. Creates compressed archives in `output/` directory
6. Reverts version numbers back to `0.0.0`

To use it:

```bash
./build.sh
```

With publishing enabled:

```bash
PUBLISH=1 ./build.sh
```

## Troubleshooting

### "error: linker `cc` not found"

Install build essentials:

```bash
# macOS
xcode-select --install

# Debian/Ubuntu
sudo apt install build-essential

# Fedora
sudo dnf install gcc
```

### Dependency Compilation Failures

Ensure you have the latest Rust toolchain:

```bash
rustup update
```

### Cross-Compilation Issues

Make sure you have added the target:

```bash
rustup target list --installed
rustup target add <target-triple>
```

### Out of Disk Space

Release builds with LTO can consume significant disk space. The `target/` directory can be cleaned:

```bash
cargo clean
```

To clean only release artifacts:

```bash
cargo clean --release
```
