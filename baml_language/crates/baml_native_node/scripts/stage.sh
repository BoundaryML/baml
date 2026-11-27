#!/usr/bin/env bash
# Stage the type-shapes fixture with the namespace `bridge_test`, pack it
# into a native library, and generate today's TypeScript SDK for it twice:
# over today's bridge (`target/native-node/today`) and as the native bindings
# (`target/native-node/native`). Then install both packages.
#
# Needs, built in release: `native_pack` (cargo build --release -p
# baml_compiler2_native --example native_pack), the addon (cargo build
# --release -p baml_native_node), and today's bridge
# (sdks/typescript/bridge_typescript: pnpm build:release).
set -euo pipefail
cd "$(dirname "$0")/../../.."   # baml_language
OUT=target/native-node
SRC=$OUT/fixtures/type_shapes/baml_src
rm -rf "$OUT/fixtures"
mkdir -p "$OUT/fixtures/type_shapes"
cp -R sdk_tests/fixtures/type_shapes/baml_src "$OUT/fixtures/type_shapes/"
mkdir -p "$SRC/ns_bridge_test"
cp crates/baml_native_node/fixture/bridge_test.baml "$SRC/ns_bridge_test/types.baml"

# Every function that packs; the ones that do not are listed with why.
target/release/examples/native_pack --lib "$SRC" "$OUT/libtype_shapes.dylib" 2> "$OUT/skipped.txt"
target/release/examples/native_pack --lib "$SRC" "$OUT/unused.dylib" --describe 2>/dev/null > "$OUT/interface.json"
HASH=0x$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).interface_hash' "$OUT/interface.json")
echo "interface hash $HASH; $(wc -l < "$OUT/skipped.txt" | tr -d ' ') functions skipped (see $OUT/skipped.txt)"

# Today's bridge refuses bytecode from another commit. It is rebuilt rarely
# (a release build), so the generator takes the commit the bridge was built
# at: BRIDGE_GIT_SHA, by default the commit of its last build.
BRIDGE_GIT_SHA=${BRIDGE_GIT_SHA:-ed50f6b614c04996d9c178b9667ba06619e8b58f}
BAML_GIT_SHA=$BRIDGE_GIT_SHA cargo run --release -q -p baml_native_node --example generate_ts -- \
  "$OUT/fixtures" "$OUT/libtype_shapes.dylib" "$HASH" "$OUT"

export npm_config_store_dir="$PWD/target/pnpm-store"
for side in today native; do
  (cd "$OUT/$side" && pnpm install --ignore-workspace --ignore-scripts --silent)
done
