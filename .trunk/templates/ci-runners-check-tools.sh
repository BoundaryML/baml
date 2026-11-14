#!/usr/bin/env bash
# Fails the bake when a mise tool in mise.toml is not complete, so the last good snapshot
# stays in use. mise checks only the version folder: `mise install` and
# `mise ls --missing` both accept a version folder with no binaries in it.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

cd /opt/actions-runner/_work/baml/baml
export PATH="${MISE_DATA_DIR}/bin:${PATH}" MISE_TRUSTED_CONFIG_PATHS="${PWD}"

failed=0

# A tool with no version folder. The command exits 0 either way, so read its output.
missing=$(mise ls --current --missing)
if [[ -n ${missing} ]]; then
	echo "mise tools that are not installed:" >&2
	echo "${missing}" >&2
	failed=1
fi

# A tool with a version folder but no binaries. Each folder that mise puts on PATH must
# hold at least one executable file.
bin_paths=$(mise bin-paths)
while IFS= read -r dir; do
	executable=""
	if [[ -d ${dir} ]]; then
		executable=$(find -L "${dir}" -maxdepth 1 -type f -perm -u+x -print -quit)
	fi
	if [[ -z ${executable} ]]; then
		echo "no executable in ${dir}" >&2
		failed=1
	fi
done <<<"${bin_paths}"

if [[ ${failed} -eq 0 ]]; then
	echo "every mise tool has its binaries"
fi
exit "${failed}"
