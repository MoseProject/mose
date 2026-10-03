#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME="$HERE/resources/runtime"
export LD_LIBRARY_PATH="$RUNTIME/lib:$HERE/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$HERE:$RUNTIME/bin${PATH:+:$PATH}"
export OCA_RESOURCE_DIR="$HERE/resources"
exec "$HERE/oca-desktop-bin" "$@"
