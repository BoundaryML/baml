#!/usr/bin/env bash
set -euo pipefail

readonly installer_url="https://pkg.boundaryml.com/install.sh"
readonly helper="${GITHUB_ACTION_PATH}/lib/wrapper-output.mjs"
readonly bin_dir="${BAML_HOME}/bin"
readonly wrapper="${bin_dir}/baml"

unset BAML_MANIFEST_BASE_URL || true
unset BAML_VERSION || true

tmp_dir="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/setup-baml.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
installer="${tmp_dir}/install.sh"

curl --proto '=https' --tlsv1.2 --retry 3 --retry-connrefused --fail --silent --show-error --location "$installer_url" --output "$installer"
sh "$installer" --wrapper-only --no-modify-path --yes

requested="${INPUT_TOOLCHAIN:-}"
version_override=""
if [[ -n "$requested" ]]; then
  selector="$(printf '%s' "$requested" | node "$helper" validate-selector)"
  version_override="$selector"
  export BAML_VERSION="$version_override"
else
  list_output="$("$wrapper" toolchain list)"
  printf '%s\n' "$list_output"
  selector="$(printf '%s' "$list_output" | node "$helper" selector)"
fi

"$wrapper" toolchain use "$selector"
version_output="$("$wrapper" --version)"
printf '%s\n' "$version_output"
version="$(printf '%s' "$version_output" | node "$helper" version)"
toolchain_path="${BAML_HOME}/toolchains/${version}/bin/baml-cli"

if [[ ! -x "$wrapper" || ! -x "$toolchain_path" ]]; then
  echo "setup-baml: wrapper or resolved toolchain binary is missing or not executable" >&2
  exit 1
fi

printf '%s\n' "$bin_dir" >> "$GITHUB_PATH"
{
  printf 'BAML_HOME=%s\n' "$BAML_HOME"
  printf 'BAML_VERSION=%s\n' "$version_override"
  printf 'BAML_MANIFEST_BASE_URL=\n'
} >> "$GITHUB_ENV"
{
  printf 'version=%s\n' "$version"
  printf 'path=%s\n' "$wrapper"
  printf 'toolchain-path=%s\n' "$toolchain_path"
} >> "$GITHUB_OUTPUT"
