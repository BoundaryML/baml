#!/usr/bin/env bash
# Build the bridge_wasm artifact from the baml repo and PUT it to the bench3
# api, which serves it at GET /wasm/bridge_wasm.tar.gz for the website's
# vercel-build.sh (instead of compiling Rust on Vercel). Replaces the old
# baml-wasm-service/publish-wasm.sh + dedicated Fly app. Re-run whenever
# baml_language/crates/bridge_wasm — or the pinned BAML version — changes.
#
# Usage:  ATB_SERVICE_TOKEN=... ./scripts/publish_wasm.sh
#         BAML_REPO=/path/to/baml WASM_API=https://bench3-api.fly.dev ./scripts/publish_wasm.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BAML_REPO="${BAML_REPO:-$here/../../../..}"
BAML_REPO="$(cd "$BAML_REPO" && pwd)"
WASM_API="${WASM_API:-https://bench3-api.fly.dev}"
: "${ATB_SERVICE_TOKEN:?set ATB_SERVICE_TOKEN (the bench3 service token)}"
wasm_dir="$BAML_REPO/typescript2/pkg-playground/wasm"

echo "==> [1/4] Build bridge_wasm from $BAML_REPO"
( cd "$BAML_REPO/typescript2" && pnpm build:wasm )

wasm_files=(
  package.json
  bridge_wasm.js
  bridge_wasm.d.ts
  bridge_wasm_bg.wasm
  bridge_wasm_bg.wasm.d.ts
)
for f in "${wasm_files[@]}"; do
  if [[ ! -s "$wasm_dir/$f" ]]; then
    echo "ERROR: expected wasm file missing after build: $wasm_dir/$f" >&2
    exit 1
  fi
done

# Must match cache_key_for_paths() in app-website/scripts/vercel-build.sh so the
# build can detect a stale artifact.
echo "==> [2/4] Compute SOURCE_HASH"
source_hash="$(
  git -C "$BAML_REPO" ls-files -s \
    baml_language typescript2/package.json typescript2/pnpm-lock.yaml |
    cksum |
    awk '{print $1 "-" $2}'
)"
echo "    SOURCE_HASH=$source_hash"

echo "==> [3/4] Stage + tar artifact"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
for f in "${wasm_files[@]}"; do
  cp "$wasm_dir/$f" "$stage/$f"
done
printf '%s\n' "$source_hash" > "$stage/SOURCE_HASH"
tar -czf "$stage/bridge_wasm.tar.gz" -C "$stage" "${wasm_files[@]}" SOURCE_HASH
echo "    $(du -h "$stage/bridge_wasm.tar.gz" | cut -f1) -> bridge_wasm.tar.gz"

echo "==> [4/4] Upload to $WASM_API"
curl -fSL -X PUT \
  -H "Authorization: Bearer $ATB_SERVICE_TOKEN" \
  --data-binary "@$stage/bridge_wasm.tar.gz" \
  "$WASM_API/wasm/bridge_wasm.tar.gz"
echo
echo "==> Done. Verify: curl -fSL $WASM_API/wasm/bridge_wasm.tar.gz | tar -tz"
