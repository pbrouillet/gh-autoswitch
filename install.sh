#!/usr/bin/env bash
# Thin wrapper: configure git to use gh-autoswitch as the credential helper.
# Usage: ./install.sh [--host github.com] [--local|--global]
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
exec "$dir/bin/gh-autoswitch" install "$@"
