#!/bin/sh
set -eu
policy=.chrono-harness/retention.json
if [ -e "$policy" ] || [ -L "$policy" ]; then
    if [ -f "$policy" ] && [ ! -L "$policy" ]; then
        exit 0
    fi
    echo "expected a regular host policy at $policy" >&2
    exit 1
fi
tmp=$(mktemp .chrono-harness/.check-retention.XXXXXXXX)
trap 'rm -f "$tmp"' EXIT
trap 'exit 1' HUP INT TERM
cat >"$tmp" <<'JSON'
{
  "schema": "chrono-output-retention/v2",
  "max_lifetime_seconds": 86400,
  "producers": {
    "lifecycle-original": [
      ".chrono-harness/state/worktrees/",
      ".chrono-harness/state/automatic-cleanup-v2/"
    ],
    "lifecycle-state-original": [
      ".chrono-harness/state/automatic-cleanup-v2/"
    ],
    "check-original": [
      ".chrono-harness/state/"
    ]
  },
  "keep_seconds": 86400,
  "diagnostic_keep_seconds": 86400,
  "max_related_outputs": 512,
  "max_summaries": 128,
  "max_nodes_per_round": 256,
  "max_bytes_per_round": 67108864,
  "max_millis_per_round": 25,
  "max_output_bytes": 1073741824,
  "metadata_keep_seconds": 86400,
  "max_manifest_nodes": 16384,
  "max_metadata_bytes_per_round": 67108864
}
JSON
# Concurrent setup may already have installed the host policy.
if ! ln "$tmp" "$policy" 2>/dev/null; then
    [ -f "$policy" ] || { echo "cannot install $policy" >&2; exit 1; }
fi
