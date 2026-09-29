#!/usr/bin/env bash
# Verify default backends and a downstream provider replacement in an isolated
# source import. The replacement requires no backend-selection feature flags.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/check_crypto_provider.py
