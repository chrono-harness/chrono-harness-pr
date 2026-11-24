#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s .github/release -p test_plan.py
mkdir -p target
scratch=$(mktemp -d "$(pwd -P)/target/check.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
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

verifier=$(command -v "${VERUS:-verus}") || coverage_failure "verifier not found"

prove() {
  chrono-host-bootstrap run-compiler-proof "$scratch" "$1" "$verifier" "$2" ||
    coverage_failure "$1: verifier execution failed"
}

admit_proof() {
  local label=$1
  shift
  chrono-host-bootstrap collect-compiler-proof "$scratch" "$label" "$verifier" \
    0.2026.04.24.f8e1704 f8e17045c4ac17a02e566b8997070f2eecc255be "$@" ||
    coverage_failure "$label: required proof result was not admitted"
}

# Read the exact library selected by Cargo in the build of the tested executable.
metadata=$(jq -ers --arg manifest "$(pwd -P)/$product" '
  [.[] | select(.reason == "compiler-artifact" and .manifest_path == $manifest
                 and .target.kind == ["lib"]) | .filenames[] | select(endswith(".rmeta"))]
  | if length == 1 then .[0] else error("expected the product library") end
' "$scratch/build.jsonl") || coverage_failure "native library was not identified"
filename=${metadata##*/}
stem=${filename#lib}
stem=${metadata%/*}/${stem%.rmeta}
chrono-host-bootstrap check-compiler-sources native "$stem" \
  src/verified_snapshot.rs src/snapshot.rs src/notification.rs src/lib.rs || coverage_failure "native library differs from required sources"

prove verify.pr-snapshot crates/pr/src/verified_snapshot.rs
prove verify.pr-snapshot-tests crates/pr/tests/verified_snapshot.rs

admit_proof verify.pr-snapshot crates/pr/src/verified_snapshot.rs \
  crates/pr/src/lib.rs \
  crates/pr/src/snapshot.rs
admit_proof verify.pr-snapshot-tests crates/pr/tests/verified_snapshot.rs \
  crates/pr/tests/lifecycle.rs \
  crates/pr/tests/../src/verified_snapshot.rs \
  crates/pr/tests/../src/lib.rs \
  crates/pr/tests/../src/snapshot.rs
