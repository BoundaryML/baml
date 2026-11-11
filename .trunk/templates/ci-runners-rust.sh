#!/usr/bin/env bash
# Builds the Rust units of the CI jobs. Each command must equal the job's command, with the
# job's env: cargo puts the profile env, the features and the package set in the fingerprint
# of each unit.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

SRC=/opt/actions-runner/_work/baml/baml
cd "${SRC}/baml_language"
export PATH="${MISE_DATA_DIR}/bin:${PATH}" MISE_TRUSTED_CONFIG_PATHS="${SRC}"
# Every job sets CARGO_INCREMENTAL=0.
export CARGO_INCREMENTAL=0
# build.rs puts BAML_GIT_SHA in the artifacts. Each job reads the basis marker and sets the
# same value, so the baked units stay fresh. The jobs print the variant: change it on every
# edit of the template or its scripts.
BAML_GIT_SHA=$(git -C "${SRC}" rev-parse HEAD)
export BAML_GIT_SHA
echo "${BAML_GIT_SHA}" >/cache/trunk-template-basis-sha
echo "trunk-warm-template-v11" >/cache/trunk-template-variant

# The packages with wasm_support, as the wasm32 and pre-commit jobs select them.
metadata=$(cargo metadata --no-deps --format-version 1)
wasm_names=$(jq -r '.packages[] | select(.source == null and .metadata.ci.wasm_support != false) | .name' <<<"${metadata}")
wasm_packages=()
while IFS= read -r name; do
	wasm_packages+=(-p "${name}")
done <<<"${wasm_names}"

# The jobs with CARGO_PROFILE_{DEV,TEST}_OPT_LEVEL=1.
(
	export CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_TEST_OPT_LEVEL=1

	# cargo test (x86_64-unknown-linux-gnu)
	cargo test --no-run --all-features --workspace \
		--exclude baml_tests --exclude baml_cli --exclude baml_pack_host --exclude "sdk_test_*" --exclude baml_bridge
	# Compiler and documentation tests
	cargo build --tests --bins -p baml_tests -p baml_cli -p baml_pack_host --all-features
	# cargo doc
	RUSTDOCFLAGS="-D warnings" cargo doc --all --no-deps
	# The cargo commands of the sdk setup scripts, one package for each command.
	cargo build -p sdk_test_codegen
	cargo build -p bridge_cffi
	cargo build -p bridge_java
	cargo build -p bridge_cffi --no-default-features --features ring-crypto,bundle-http

	# cargo test (x86_64-unknown-linux-musl)
	(
		export CARGO_BUILD_TARGET=x86_64-unknown-linux-musl
		export CC_x86_64_unknown_linux_musl=musl-gcc CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
		cargo build --tests --bin baml-cli --bin baml-pack-host --workspace \
			--exclude baml_tests --exclude "sdk_test_*" --exclude baml_bridge
		cargo test --no-run --workspace --exclude baml_tests --exclude "sdk_test_*" --exclude baml_bridge
	)

	# cargo test (wasm32-unknown-unknown)
	cargo build "${wasm_packages[@]}" --target wasm32-unknown-unknown --no-default-features --release
)

# The jobs with no opt-level env.

# Pre-commit Checks: keep the clippy arguments equal to baml_language/mise.toml.
cargo build -p baml_cli
cargo build -p cargo-stow
cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_CONF_DIR="${PWD}/clippy-wasm" cargo clippy "${wasm_packages[@]}" --target wasm32-unknown-unknown \
	--all-features --locked -- -D warnings -W clippy::disallowed-types

# Typecheck, Unit Tests and Browser Tests: `pnpm --filter pkg-playground build:wasm`.
(
	cd "${SRC}/typescript2/pkg-playground"
	mise exec github:rustwasm/wasm-pack -- wasm-pack build ../../baml_language/crates/bridge_wasm \
		--target web --out-dir ../../../typescript2/pkg-playground/wasm --release
)

# wasm-pack test
cargo check -p bridge_wasm --target wasm32-unknown-unknown
(
	cd crates/bridge_wasm
	mise exec node github:rustwasm/wasm-pack -- wasm-pack test --node
)
