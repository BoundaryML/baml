#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app_dir="$(cd "${script_dir}/.." && pwd)"
wasm_dir="${app_dir}/../pkg-playground/wasm"
wasm_pin_file="${app_dir}/../pkg-playground/wasm.version"
package_name="@boundaryml/bridge-wasm"

wasm_files=(
  "${wasm_dir}/package.json"
  "${wasm_dir}/bridge_wasm.js"
  "${wasm_dir}/bridge_wasm.d.ts"
  "${wasm_dir}/bridge_wasm_bg.wasm"
  "${wasm_dir}/bridge_wasm_bg.wasm.d.ts"
)

check_wasm_exists() {
  for wasm_file in "${wasm_files[@]}"; do
    if [[ ! -s "$wasm_file" ]]; then
      return 1
    fi
  done
}

if [[ ! -s "$wasm_pin_file" ]]; then
  echo "missing wasm pin file: ${wasm_pin_file}" >&2
  exit 1
fi

wasm_pin="$(tr -d '[:space:]' < "$wasm_pin_file")"
if [[ -z "$wasm_pin" ]]; then
  echo "empty wasm pin file: ${wasm_pin_file}" >&2
  exit 1
fi

package_spec="${package_name}@${wasm_pin}"
echo "==> Restore bridge_wasm from npm (${package_spec})"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

npm view "$package_spec" version >/dev/null
(cd "$tmp_dir" && npm pack "$package_spec")

rm -rf "$wasm_dir"
mkdir -p "$wasm_dir"
tar -xzf "$tmp_dir"/*.tgz --strip-components=1 -C "$wasm_dir"

if ! check_wasm_exists; then
  echo "restored ${package_spec}, but expected bridge_wasm artifacts are missing" >&2
  exit 1
fi
