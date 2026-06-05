#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

echo "Checking formatting..."
cargo fmt --all --check

echo "Running clippy..."
cargo clippy --workspace --all-targets -- -D warnings

echo "Building documentation..."
RUSTDOCFLAGS="${RUSTDOCFLAGS:--D warnings}" cargo doc --no-deps --workspace

echo "Running tests..."
cargo test --workspace --all-targets

echo "Building release binaries..."
cargo build --release -p imessage-exporter -p imessage-gui

host="$(rustc -vV | awk '/^host: / { print $2 }')"
case "$host" in
    *windows*)
        bin_ext=".exe"
        ;;
    *)
        bin_ext=""
        ;;
esac

mkdir -p output
cp "target/release/imessage-exporter${bin_ext}" "output/imessage-exporter-${host}${bin_ext}"
cp "target/release/imessage-gui${bin_ext}" "output/imessage-gui-${host}${bin_ext}"
cp LICENSE output/

echo "Built:"
echo "  output/imessage-exporter-${host}${bin_ext}"
echo "  output/imessage-gui-${host}${bin_ext}"
echo "  output/LICENSE"
