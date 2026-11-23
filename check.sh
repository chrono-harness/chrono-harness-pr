#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
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
cargo build --locked --manifest-path crates/pr/Cargo.toml
cargo test --locked --features test-gh --manifest-path crates/pr/Cargo.toml -- --test-threads=2

prove() {
  local label=$1 root=$2
  local output="$scratch/$label.rlib"
  "${VERUS:-verus}" --crate-type=lib --no-cheating --no-external-by-default --num-threads 2 \
    --output-json --compile "--emit=link=$output,dep-info=$output.d" "$root" -o "$output" \
    -Zchecksum-hash-algorithm=sha256 > "$scratch/proof.json" || { cat "$scratch/proof.json" >&2; return 1; }
  cat "$scratch/proof.json"
  jq -e '.verus.version == "0.2026.04.24.f8e1704" and
    .verus.commit == "f8e17045c4ac17a02e566b8997070f2eecc255be" and
    (."verification-results" | .success == true and ."is-verifying-entire-crate" == true and
    ."encountered-error" == false and ."encountered-vir-error" == false and
    .errors == 0 and .verified > 0)' "$scratch/proof.json" >/dev/null
  test -s "$output" && test -s "$output.d"
}

prove verify.pr-snapshot crates/pr/src/verified_snapshot.rs
prove verify.pr-snapshot-tests crates/pr/tests/verified_snapshot.rs
