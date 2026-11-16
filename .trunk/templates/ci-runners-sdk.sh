#!/usr/bin/env bash
# Runs the job's own commands for each sdk leg. A leg that fails makes a smaller warm tree,
# not a failed bake: its name goes to the log and to /cache/trunk-template-skipped.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

SRC=/opt/actions-runner/_work/baml/baml
cd "${SRC}/baml_language"
export PATH="${MISE_DATA_DIR}/bin:${PATH}" MISE_TRUSTED_CONFIG_PATHS="${SRC}"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_TEST_OPT_LEVEL=1
# GitHub Actions sets CI=true, and pnpm reads it.
export CI=true
BAML_GIT_SHA=$(git -C "${SRC}" rev-parse HEAD)
export BAML_GIT_SHA

: >/cache/trunk-template-skipped

# sdk_leg <sdk> [mise tool...]
sdk_leg() {
	local sdk=$1
	shift
	if ! cargo test --no-run -p "sdk_test_${sdk}" --all-features ||
		! mise exec "$@" -- "./sdk_tests/crates/${sdk}/setup.sh"; then
		echo "sdk leg ${sdk}" | tee -a /cache/trunk-template-skipped >&2
	fi
}

sdk_leg typescript node npm:pnpm
sdk_leg typescript_web node npm:pnpm github:rustwasm/wasm-pack
sdk_leg rust
sdk_leg java java gradle
sdk_leg python_pydantic2 python uv
sdk_leg go go
sdk_leg csharp dotnet
sdk_leg cpp
sdk_leg ruby_sorbet ruby

# pnpm links node_modules to a store that the job cannot use (ERR_PNPM_UNEXPECTED_STORE), so
# the jobs run a cold `pnpm install`. Only the Rust units of these legs stay warm.
find "${SRC}" -path "${SRC}/baml_language/target" -prune -o -type d -name node_modules -prune \
	-exec rm -rf {} +
rm -rf "${SRC}/.pnpm-store" "${SRC}/baml_language/target/pnpm-store"

# Delete the final outputs at the top of each Cargo profile folder (target/debug,
# target/<triple>/release, ...): binaries, libraries and their .d files. A job that does
# not build an output must not find canary's copy. The SDK tests run target/debug/baml-cli
# when it is there, so they tested canary's CLI, not the PR's. deps/, build/ and
# .fingerprint/ stay, so a job that builds the output gets it back from deps/ at once.
fingerprints=$(find "${SRC}/baml_language/target" -maxdepth 3 -type d -name .fingerprint)
while IFS= read -r fingerprint; do
	[[ -n ${fingerprint} ]] || continue
	find "${fingerprint%/.fingerprint}" -maxdepth 1 \( -type f -o -type l \) ! -name '.*' -print -delete
done <<<"${fingerprints}"

df -h /
du -sh "${SRC}/baml_language/target" "${MISE_DATA_DIR}"
echo "sdk legs that failed:"
cat /cache/trunk-template-skipped
