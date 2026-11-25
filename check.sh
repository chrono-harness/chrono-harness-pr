#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ $# -eq 0 ]]; then
  /bin/sh .chrono-harness/ci/install.sh
  export PATH="$PWD/.chrono-harness/bin:$PATH"
  export CHRONO_CHECK_SOURCE=${CHRONO_CHECK_SOURCE:-local}
  exec .chrono-harness/bin/chrono-harness check
fi
if [[ $# -eq 1 ]]; then
  case $1 in
    --native|--linux-process-group) exec chrono-host-bootstrap native-scratch run /bin/bash .chrono-harness/native-check.sh "$1" ;;
  esac
fi
echo 'usage: ./check.sh [--native|--linux-process-group]' >&2
exit 2
