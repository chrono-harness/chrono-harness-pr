#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ $# -eq 0 ]]; then
  /bin/sh .chrono-harness/ci/install.sh
  export PATH="$PWD/.chrono-harness/bin:$PATH"
  export CHRONO_CHECK_SOURCE=${CHRONO_CHECK_SOURCE:-local}
  exec .chrono-harness/bin/chrono-harness check
fi
if [[ $# -eq 1 && $1 == --linux-process-group ]]; then
  mkdir -p target
  scratch=$(mktemp -d "$(pwd -P)/target/linux-process-group.XXXXXXXX")
  trap 'rm -rf -- "$scratch"' EXIT
  CARGO_TARGET_DIR="$scratch/cargo" cargo test --locked --features test-gh --manifest-path crates/pr/Cargo.toml --test lifecycle quiet_descendant
  exit 0
fi
if [[ $# -ne 1 || $1 != --native ]]; then
  echo 'usage: ./check.sh [--native|--linux-process-group]' >&2
  exit 2
fi
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
  src/verified_snapshot.rs src/snapshot.rs src/notification.rs src/run_watch.rs src/reminders.rs src/lib.rs || coverage_failure "native library differs from required sources"
