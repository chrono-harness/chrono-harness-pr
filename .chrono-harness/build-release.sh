#!/usr/bin/env bash
set -euo pipefail
platform=$1
destination=$2
cargo build --release --locked --manifest-path crates/pr/Cargo.toml
"$CARGO_TARGET_DIR/release/chrono-pr" --help
cp "$CARGO_TARGET_DIR/release/chrono-pr" "$destination/chrono-pr-$platform"
