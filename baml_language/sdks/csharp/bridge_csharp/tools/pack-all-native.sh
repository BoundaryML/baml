#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <native-assets-root> <output-directory>" >&2
  exit 2
fi

assets_input=$1
output_input=$2
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
bridge_project="$script_dir/../src/Baml.Bridge/Baml.Bridge.csproj"
normalizer_project="$script_dir/Baml.NuGet.Normalize/Baml.NuGet.Normalize.csproj"

if [[ ! -d "$assets_input" ]]; then
  echo "native assets root does not exist: $assets_input" >&2
  exit 2
fi

assets_root=$(cd -- "$assets_input" && pwd -P)
declare -A expected_names=(
  [linux-x64]=libbridge_cffi.so
  [linux-arm64]=libbridge_cffi.so
  [linux-musl-x64]=libbridge_cffi.so
  [linux-musl-arm64]=libbridge_cffi.so
  [osx-x64]=libbridge_cffi.dylib
  [osx-arm64]=libbridge_cffi.dylib
  [win-x64]=bridge_cffi.dll
  [win-arm64]=bridge_cffi.dll
)
declare -A properties=(
  [linux-x64]=BamlNativeLinuxX64
  [linux-arm64]=BamlNativeLinuxArm64
  [linux-musl-x64]=BamlNativeLinuxMuslX64
  [linux-musl-arm64]=BamlNativeLinuxMuslArm64
  [osx-x64]=BamlNativeOsxX64
  [osx-arm64]=BamlNativeOsxArm64
  [win-x64]=BamlNativeWinX64
  [win-arm64]=BamlNativeWinArm64
)

pack_properties=(-p:BamlPackAllNative=true)
expected_entries=()
for rid in "${!expected_names[@]}"; do
  name=${expected_names[$rid]}
  path="$assets_root/runtimes/$rid/native/$name"
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "missing regular native asset for $rid: $path" >&2
    exit 2
  fi
  pack_properties+=("-p:${properties[$rid]}=$path")
  expected_entries+=("runtimes/$rid/native/$name")
done

mapfile -t actual_assets < <(find "$assets_root/runtimes" -type f -print | sort)
if [[ ${#actual_assets[@]} -ne 8 ]]; then
  echo "expected exactly 8 native input files, found ${#actual_assets[@]}" >&2
  printf '  %s\n' "${actual_assets[@]}" >&2
  exit 2
fi

mkdir -p -- "$output_input"
output_dir=$(cd -- "$output_input" && pwd -P)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/baml-all-native-pack.XXXXXXXX")
partial_output=
partial_symbols=
cleanup() {
  rm -rf -- "$work_dir"
  if [[ -n "$partial_output" ]]; then
    rm -f -- "$partial_output"
  fi
  if [[ -n "$partial_symbols" ]]; then
    rm -f -- "$partial_symbols"
  fi
}
trap cleanup EXIT

dotnet pack "$bridge_project" \
  --configuration Release \
  --output "$work_dir/raw" \
  -p:NuGetAudit=false \
  "${pack_properties[@]}"

shopt -s nullglob
packages=("$work_dir"/raw/*.nupkg)
symbols=("$work_dir"/raw/*.snupkg)
if [[ ${#packages[@]} -ne 1 || ${#symbols[@]} -ne 1 ]]; then
  echo "expected one nupkg and one snupkg, found ${#packages[@]} and ${#symbols[@]}" >&2
  exit 1
fi

package_name=$(basename -- "${packages[0]}")
symbols_name=$(basename -- "${symbols[0]}")
partial_output="$output_dir/.$package_name.tmp.$$"
partial_symbols="$output_dir/.$symbols_name.tmp.$$"
dotnet run \
  --project "$normalizer_project" \
  --configuration Release \
  -- "${packages[0]}" "$partial_output"
dotnet run \
  --project "$normalizer_project" \
  --configuration Release \
  -- "${symbols[0]}" "$partial_symbols"

mapfile -t packaged_native < <(unzip -Z1 "$partial_output" | sed -n '/^runtimes\/.*\/native\//p' | sort)
mapfile -t expected_sorted < <(printf '%s\n' "${expected_entries[@]}" | sort)
if [[ "${packaged_native[*]}" != "${expected_sorted[*]}" ]]; then
  echo "NuGet native payload does not match the required eight RID assets" >&2
  printf 'actual:   %s\n' "${packaged_native[@]}" >&2
  printf 'expected: %s\n' "${expected_sorted[@]}" >&2
  exit 1
fi

mv -f -- "$partial_output" "$output_dir/$package_name"
partial_output=
mv -f -- "$partial_symbols" "$output_dir/$symbols_name"
partial_symbols=

printf '%s\n' "$output_dir/$package_name"
