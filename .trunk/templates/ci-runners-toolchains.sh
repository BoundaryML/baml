#!/usr/bin/env bash
# Installs the Rust toolchain, the musl C toolchain and the mise tools of the CI jobs.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

# Build at the job's path, not at /workspace/baml. Cargo keeps absolute paths in its
# output, so a tree built at another path is stale or wrong in the job.
SRC=/opt/actions-runner/_work/baml/baml
cd "${SRC}/baml_language"

# The toolchain that rust-toolchain.toml pins, also as the default for cargo calls from the
# repository root.
rustup show
toolchain=$(rustup show active-toolchain)
rustup default "${toolchain%% *}"
rustup target add x86_64-unknown-linux-musl wasm32-unknown-unknown
sudo DEBIAN_FRONTEND=noninteractive apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y musl-tools

# mise-action uses $MISE_DATA_DIR/bin/mise when it is there. mise installs the `cargo:`
# tools from binaries only when cargo-binstall is on PATH.
# TODO: pin the mise and cargo-binstall versions.
mkdir -p "${MISE_DATA_DIR}/bin"
curl -fsSL https://mise.run | MISE_INSTALL_PATH="${MISE_DATA_DIR}/bin/mise" sh
curl -fsSL https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz |
	tar -xz -C "${MISE_DATA_DIR}/bin" cargo-binstall
export PATH="${MISE_DATA_DIR}/bin:${PATH}" MISE_YES=1 MISE_TRUSTED_CONFIG_PATHS="${SRC}"

# The union of the `install_args` in .github/workflows/kiln-shadow.yml. Run from the
# repository root, as mise-action does, so mise resolves the same versions.
(
	cd "${SRC}"
	mise install \
		sccache direnv node npm:pnpm protoc protoc-gen-go python uv clang-format ast-grep \
		go java gradle dotnet ruby github:rustwasm/wasm-pack cargo:prek cargo:cargo-insta cargo:cargo-nextest
)

cargo fetch
