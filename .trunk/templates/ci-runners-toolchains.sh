#!/usr/bin/env bash
# Installs the Rust toolchain, the musl C toolchain and every mise tool in mise.toml.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

# Build at the job's path, not at /workspace/baml. Cargo keeps absolute paths in its
# output, so a tree built at another path is stale or wrong in the job.
SRC=/opt/actions-runner/_work/baml/baml
cd "${SRC}/baml_language"

# Runs a download step at most 3 times, each under a 5-minute limit, as the CI jobs do for the
# wasm target, so one stalled connection cannot hold the build until its own time limit.
retry() {
	local attempt
	for attempt in 1 2 3; do
		if timeout 300 "$@"; then
			return 0
		fi
		echo "attempt ${attempt} of 3 failed: $*" >&2
		sleep 10
	done
	return 1
}

# Downloads <url> and puts it at <dest> only if its SHA-256 is <sha256>. Change the version and
# the digest together; the digest is in the release's asset list.
fetch_verified() {
	local url=$1 sha256=$2 dest=$3
	retry curl -fsSL --retry 3 -o "${dest}" "${url}"
	echo "${sha256}  ${dest}" | sha256sum -c --quiet -
}

# The toolchain that rust-toolchain.toml pins, also as the default for cargo calls from the
# repository root.
rustup show
toolchain=$(rustup show active-toolchain)
rustup default "${toolchain%% *}"
retry rustup target add x86_64-unknown-linux-musl wasm32-unknown-unknown
# The Pre-commit Checks job installs this toolchain, and mise compiles cargo-hawk with it.
retry rustup toolchain install 1.97.1 --profile minimal --component rustc-dev
sudo DEBIAN_FRONTEND=noninteractive apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y musl-tools

# mise-action uses $MISE_DATA_DIR/bin/mise when it is there. mise installs the `cargo:`
# tools from binaries only when cargo-binstall is on PATH. Both are pinned and checked before
# anything runs them.
MISE_VERSION=2026.11.6
MISE_SHA256=d6ebb6f5601bdf5a8d1d56028b24fe575f45075b989941ccc920f95161c5fa28
CARGO_BINSTALL_VERSION=1.26.1
CARGO_BINSTALL_SHA256=43d379fef8cb450961a14ff113e7bce7b5e1e52db709c776d186f928d8a20a40
downloads=$(mktemp -d)
fetch_verified "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-linux-x64.tar.gz" \
	"${MISE_SHA256}" "${downloads}/mise.tar.gz"
fetch_verified "https://github.com/cargo-bins/cargo-binstall/releases/download/v${CARGO_BINSTALL_VERSION}/cargo-binstall-x86_64-unknown-linux-musl.tgz" \
	"${CARGO_BINSTALL_SHA256}" "${downloads}/cargo-binstall.tgz"
mkdir -p "${MISE_DATA_DIR}/bin"
tar -xzf "${downloads}/mise.tar.gz" -C "${downloads}" mise/bin/mise
install -m 0755 "${downloads}/mise/bin/mise" "${MISE_DATA_DIR}/bin/mise"
tar -xzf "${downloads}/cargo-binstall.tgz" -C "${MISE_DATA_DIR}/bin" cargo-binstall
rm -rf "${downloads}"
export PATH="${MISE_DATA_DIR}/bin:${PATH}" MISE_YES=1 MISE_TRUSTED_CONFIG_PATHS="${SRC}"

# Every tool in mise.toml, so a job that starts to use one more tool finds it in the snapshot.
# Run from the repository root, as mise-action does, so mise resolves the same versions.
(
	cd "${SRC}"
	mise install
)

cargo fetch
