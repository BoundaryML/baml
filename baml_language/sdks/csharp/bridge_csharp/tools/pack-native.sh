#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
  echo "usage: $0 <native-library> <rid> <output-directory> [platform-contract]" >&2
  exit 2
fi

native_input=$1
rid=$2
output_input=$3
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
contract_input=${4:-"$script_dir/../../../../../release/platforms.json"}
bridge_project="$script_dir/../src/Baml.Bridge/Baml.Bridge.csproj"
normalizer_project="$script_dir/Baml.NuGet.Normalize/Baml.NuGet.Normalize.csproj"

if [[ ! -f "$native_input" ]]; then
  echo "native library does not exist: $native_input" >&2
  exit 2
fi

if [[ ! -f "$contract_input" ]]; then
  echo "platform contract does not exist: $contract_input" >&2
  exit 2
fi

native_dir=$(cd -- "$(dirname -- "$native_input")" && pwd -P)
native_library="$native_dir/$(basename -- "$native_input")"
expected_name=$(jq -er --arg rid "$rid" '
  [.targets[].artifacts.csharp
    | select(. != null and .rid == $rid)
    | .native_asset]
  | if length == 1 then .[0]
    elif length == 0 then error("unsupported BAML RID")
    else error("duplicate BAML RID in platform contract")
    end
' "$contract_input") || {
  echo "unsupported or invalid BAML RID in platform contract: $rid" >&2
  exit 2
}

if [[ $(basename -- "$native_library") != "$expected_name" ]]; then
  echo "native library for $rid must be named $expected_name" >&2
  exit 2
fi

mkdir -p -- "$output_input"
output_dir=$(cd -- "$output_input" && pwd -P)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/baml-native-pack.XXXXXXXX")
partial_output=
cleanup() {
  rm -rf -- "$work_dir"
  if [[ -n "$partial_output" ]]; then
    rm -f -- "$partial_output"
  fi
}
trap cleanup EXIT

dotnet pack "$bridge_project" \
  --configuration Release \
  --output "$work_dir/raw" \
  -p:NuGetAudit=false \
  -p:BamlNativeLibrary="$native_library" \
  -p:BamlNativeRid="$rid"

shopt -s nullglob
packages=("$work_dir"/raw/*.nupkg)
if [[ ${#packages[@]} -ne 1 ]]; then
  echo "expected one NuGet package, found ${#packages[@]}" >&2
  exit 1
fi

package_name=$(basename -- "${packages[0]}")
partial_output="$output_dir/.$package_name.tmp.$$"
dotnet run \
  --project "$normalizer_project" \
  --configuration Release \
  -- "${packages[0]}" "$partial_output"
mv -f -- "$partial_output" "$output_dir/$package_name"
partial_output=

printf '%s\n' "$output_dir/$package_name"
