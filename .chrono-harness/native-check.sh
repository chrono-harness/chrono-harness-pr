#!/usr/bin/env bash
set -euo pipefail
scratch=${CHRONO_NATIVE_SCRATCH:?expected the directory allocated by chrono-host-bootstrap}
if [[ $1 == --linux-process-group ]]; then
  export CARGO_TARGET_DIR="$scratch/cargo"
  exec cargo test --locked --features test-gh --manifest-path crates/pr/Cargo.toml --test lifecycle quiet_descendant
fi
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s .github/release -p test_plan.py
export TMPDIR="$scratch"
export CARGO_TARGET_DIR="$scratch/cargo"
# Only this product may come from a local path. Check the resolved graph used
# by the binary and its test fixture, then run Cargo offline from that point.
cargo metadata --locked --all-features --manifest-path crates/pr/Cargo.toml --format-version 1 > "$scratch/dependencies.json"
jq -e --arg product "$(pwd -P)/crates/pr/Cargo.toml" '
  all(.packages[];
    if .source == null then .manifest_path == $product
    elif (.source | startswith("registry+")) then true
    elif (.source | startswith("git+")) then
      (.source | test("github\\.com[:/]((chrono-harness/)|(ChronoAIProject/chrono-harness([./?#]|$)))"; "i") | not)
    else false end)
' "$scratch/dependencies.json" >/dev/null || { echo 'unexpected source in the product or test dependency graph' >&2; exit 1; }
export CARGO_NET_OFFLINE=true
cargo fmt --check --manifest-path crates/pr/Cargo.toml
product=crates/pr/Cargo.toml
cargo build --locked --message-format=json-render-diagnostics --manifest-path "$product" > "$scratch/build.jsonl"
cargo test --locked --features test-gh --manifest-path crates/pr/Cargo.toml -- --test-threads=2

coverage_failure() {
  echo "E_VERUS_COVERAGE: $*" >&2
  exit 1
}

# Read the exact library selected by Cargo in the build of the tested executable.
stem=$(chrono-host-bootstrap select-native-cargo-library "$(pwd -P)/$product" "$scratch/build.jsonl") ||
  coverage_failure "native library was not identified"
chrono-host-bootstrap check-compiler-sources native "$stem" \
  src/verified_snapshot.rs src/snapshot.rs src/notification.rs src/run_watch.rs src/reminders.rs src/lib.rs || coverage_failure "native library differs from required sources"
