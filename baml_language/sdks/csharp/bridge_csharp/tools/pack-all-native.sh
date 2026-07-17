#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <native-assets-root> <output-directory> [platform-contract]" >&2
  exit 2
fi

assets_input=$1
output_input=$2
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
contract_input=${3:-"$script_dir/../../../../../release/platforms.json"}
bridge_project="$script_dir/../src/Baml.Bridge/Baml.Bridge.csproj"
normalizer_project="$script_dir/Baml.NuGet.Normalize/Baml.NuGet.Normalize.csproj"

if [[ ! -d "$assets_input" ]]; then
  echo "native assets root does not exist: $assets_input" >&2
  exit 2
fi

if [[ ! -f "$contract_input" ]]; then
  echo "platform contract does not exist: $contract_input" >&2
  exit 2
fi

assets_root=$(cd -- "$assets_input" && pwd -P)
contract_dir=$(cd -- "$(dirname -- "$contract_input")" && pwd -P)
platform_contract="$contract_dir/$(basename -- "$contract_input")"

contract_status=$(jq -er '
  if .schema == 1
    and .csharp_package.package_id == "baml-bridge"
    and .csharp_package.atomic_all_rids == true
    and .csharp_package.cffi_inputs_required == true
    and ([.targets[] | select(.artifacts.csharp != null)] | length) > 0
    and ([.targets[] | select(.artifacts.csharp != null)] | length)
      == ([.targets[] | select(.artifacts.cffi != null)] | length)
    and all(.targets[];
      (.artifacts.csharp == null and .artifacts.cffi == null)
      or (.artifacts.csharp != null
        and .artifacts.cffi != null
        and (.artifacts.csharp.rid | type == "string" and length > 0)
        and (.artifacts.csharp.native_asset | type == "string" and length > 0)
        and (.artifacts.csharp.pack_property | type == "string" and length > 0)))
  then "valid"
  else error("invalid atomic C# package platform contract")
  end
' "$platform_contract")
[[ "$contract_status" == valid ]]

mapfile -t csharp_assets < <(
  jq -er '.targets[]
    | select(.artifacts.csharp != null)
    | .artifacts.csharp
    | [.rid, .native_asset, .pack_property]
    | @tsv' "$platform_contract"
)

pack_properties=(-p:BamlPackAllNative=true)
expected_entries=()
for asset in "${csharp_assets[@]}"; do
  IFS=$'\t' read -r rid name property <<<"$asset"
  path="$assets_root/runtimes/$rid/native/$name"
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "missing regular native asset for $rid: $path" >&2
    exit 2
  fi
  pack_properties+=("-p:${property}=$path")
  expected_entries+=("runtimes/$rid/native/$name")
done

mapfile -t actual_assets < <(
  find "$assets_root/runtimes" \( -type f -o -type l \) -print | sort
)
if [[ ${#actual_assets[@]} -ne ${#expected_entries[@]} ]]; then
  echo "expected exactly ${#expected_entries[@]} native input files from the platform contract, found ${#actual_assets[@]}" >&2
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
  echo "NuGet native payload does not match the required platform-contract RID assets" >&2
  printf 'actual:   %s\n' "${packaged_native[@]}" >&2
  printf 'expected: %s\n' "${expected_sorted[@]}" >&2
  exit 1
fi

mv -f -- "$partial_output" "$output_dir/$package_name"
partial_output=
mv -f -- "$partial_symbols" "$output_dir/$symbols_name"
partial_symbols=

printf '%s\n' "$output_dir/$package_name"
