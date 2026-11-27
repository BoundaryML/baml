#!/usr/bin/env bash
# The Rust bench: the native bindings (examples/bench.rs) against today's
# generated Rust SDK over the VM (tests/type_shapes/bench_today.rs), same
# sources (the type-shapes fixture plus bridge_test), same calls.
#
#   scripts/bench.sh prepare     build both, run each once (macOS scans new binaries)
#   scripts/bench_run.py ...     the timed rounds (under perfq --bench)
set -euo pipefail
cd "$(dirname "$0")/../../.."   # baml_language
OUT=target/native-rust
FIXTURE=sdk_tests/fixtures/type_shapes/baml_src
BRIDGE_TEST=crates/baml_native_host/tests/type_shapes/bridge_test.baml
GENERATED=sdk_tests/crates/rust/type_shapes/generated
mkdir -p "$OUT"

# The library, with the exports the bindings were written for, in order.
rm -rf "$OUT/src" && cp -R "$FIXTURE" "$OUT/src"
mkdir -p "$OUT/src/ns_bridge_test" && cp "$BRIDGE_TEST" "$OUT/src/ns_bridge_test/types.baml"
cargo build --release -p baml_compiler2_native --example native_pack
target/release/examples/native_pack --lib "$OUT/src" "$OUT/libtype_shapes.dylib" bridge_test.add bridge_test.lookup bridge_test.round_trip_points bridge_test.make_points bridge_test.count_points bridge_test.make_ints bridge_test.count_ints bridge_test.nap bridge_test.env_or make_foo round_trip_foo class_refs.make_outer class_refs.round_trip_inner class_refs.round_trip_outer complex_models.round_trip_complex_profile enums.pick_sentiment enums.round_trip_sentiment enums.round_trip_enums lists.round_trip_ints lists.round_trip_optional_strings lists.round_trip_union_list lists.round_trip_list_container maps.round_trip_simple_map maps.round_trip_enum_keyed_map maps.round_trip_list_valued_map maps.round_trip_map_container optional.round_trip_optional_int optional.round_trip_optional_resume optional.round_trip_optional_union optional.round_trip_optional_container unions.round_trip_null_to_end unions.round_trip_union_container unions.round_trip_str_or_int_list unions.round_trip_optional_plus_null primitives.round_trip_int primitives.round_trip_float primitives.round_trip_string primitives.round_trip_bool primitives.return_null bridge_test.two_lists bridge_test.fail_quietly 

# Today's SDK for the same sources: the generator reads the fixture in
# place, so bridge_test is staged into it for this run only.
STAGED="$FIXTURE/ns_bridge_test"
trap 'rm -rf "$STAGED"' EXIT
mkdir -p "$STAGED" && cp "$BRIDGE_TEST" "$STAGED/types.baml"
cargo run --quiet -p sdk_test_codegen -- rust
rm -rf "$STAGED"
trap - EXIT
mkdir -p "$GENERATED/examples"
cp crates/baml_native_host/tests/type_shapes/bench_today.rs "$GENERATED/examples/"
(cd "$GENERATED" && CARGO_TARGET_DIR="$PWD/../../../../../target/sdk-rust-target" \
  cargo build --release --example bench_today)
cp target/sdk-rust-target/release/examples/bench_today "$OUT/"
cargo build --release -p baml_native_host --example bench
cp target/release/examples/bench "$OUT/"
# Today's SDK loads the engine library at run time: this commit's, release,
# named by BAML_BRIDGE_PATH (bench_run.py sets it), not a downloaded one.
cargo build --release -p bridge_cffi
cp target/release/libbridge_cffi.dylib "$OUT/"

# One untimed run of each.
for case in int ints_out; do
  "$OUT/bench" "$OUT/libtype_shapes.dylib" "$case" 100
  BAML_BRIDGE_PATH="$PWD/$OUT/libbridge_cffi.dylib" BAML_BRIDGE_DISABLE_DOWNLOAD=1 "$OUT/bench_today" "$case" 100
done
