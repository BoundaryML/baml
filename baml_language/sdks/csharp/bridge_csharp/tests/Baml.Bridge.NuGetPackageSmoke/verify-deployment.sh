#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 4 ]]; then
  echo "usage: verify-deployment.sh <exact-package> <rid> <canonical-native-name> <generated-source-root>" >&2
  exit 2
fi

test_dir="$(cd "$(dirname "$0")" && pwd -P)"
package="$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")"
rid="$2"
canonical_native="$3"
generated_source_root="$(cd "$4" && pwd -P)"
version="$(unzip -p "$package" baml-bridge.nuspec \
  | sed -n 's#.*<version>\([^<]*\)</version>.*#\1#p')"
test -n "$version"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
feed="$work/feed"
consumer="$work/consumer"
packages="$work/packages"
mkdir -p "$feed" "$consumer/baml_sdk" "$packages"
cp "$package" "$feed/baml-bridge.$version.nupkg"
cp "$test_dir/Baml.Bridge.NuGetPackageSmoke.csproj" "$consumer/"
cp "$test_dir/Program.cs" "$consumer/"
cp -R "$generated_source_root/." "$consumer/baml_sdk/"
printf '%s\n' \
  '<?xml version="1.0" encoding="utf-8"?>' \
  '<configuration>' \
  '  <packageSources>' \
  '    <clear />' \
  '    <add key="baml-product" value="%BAML_CSHARP_PRODUCT_FEED%" />' \
  '    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" />' \
  '  </packageSources>' \
  '  <packageSourceMapping>' \
  '    <packageSource key="baml-product"><package pattern="baml-bridge" /></packageSource>' \
  '    <packageSource key="nuget.org"><package pattern="Google.*" /><package pattern="Microsoft.*" /></packageSource>' \
  '  </packageSourceMapping>' \
  '</configuration>' \
  > "$consumer/NuGet.Config"

export BAML_CSHARP_PRODUCT_FEED="$feed"
project="$consumer/Baml.Bridge.NuGetPackageSmoke.csproj"
NUGET_PACKAGES="$packages" dotnet restore "$project" \
  --runtime "$rid" \
  --configfile "$consumer/NuGet.Config" \
  -p:NuGetAudit=false \
  -p:BamlBridgePackageVersion="$version" \
  -p:SelfContained=true \
  -p:PublishTrimmed=true \
  -p:PublishSingleFile=true

package_native="$packages/baml-bridge/${version,,}/runtimes/$rid/native/$canonical_native"
test -f "$package_native"
common=(
  --configuration Release
  --runtime "$rid"
  --self-contained true
  --no-restore
  -p:NuGetAudit=false
  -p:BamlBridgePackageVersion="$version"
  -p:SuppressTrimAnalysisWarnings=false
  -p:TrimmerSingleWarn=false
  -p:ILLinkTreatWarningsAsErrors=true
)
trimmed=(-p:PublishTrimmed=true -p:TrimMode=link)

native_assets() {
  find "$1" -type f \
    \( -name 'bridge_cffi.dll' \
      -o -name 'libbridge_cffi.dylib' \
      -o -name 'libbridge_cffi.so' \) \
    -print0
}

assert_single_file_inventory() {
  local output="$1"
  local include_native="$2"
  local file
  while IFS= read -r -d '' file; do
    test "$(dirname "$file")" = "$output"
    case "$(basename "$file")" in
      Baml.Bridge.NuGetPackageSmoke|*.pdb)
        ;;
      "$canonical_native")
        test "$include_native" = true
        ;;
      *)
        echo "unexpected loose single-file publish asset: $file" >&2
        return 1
        ;;
    esac
  done < <(find "$output" -type f -print0)
}

run_sidecar() {
  local output="$1"
  local single_file="$2"
  test -x "$output/Baml.Bridge.NuGetPackageSmoke"
  mapfile -d '' native < <(native_assets "$output")
  test "${#native[@]}" -eq 1
  test "${native[0]}" = "$output/$canonical_native"
  cmp "$package_native" "${native[0]}"
  if [[ "$single_file" == true ]]; then
    assert_single_file_inventory "$output" true
  fi
  env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
    "$output/Baml.Bridge.NuGetPackageSmoke" \
    | grep -Fx 'csharp_nuget_package_smoke=ok'
}

run_self_extract() {
  local output="$1"
  local extraction_root="$2"
  test -x "$output/Baml.Bridge.NuGetPackageSmoke"
  mapfile -d '' bundled_output_native < <(native_assets "$output")
  test "${#bundled_output_native[@]}" -eq 0
  assert_single_file_inventory "$output" false
  mkdir -p "$extraction_root"
  env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
    DOTNET_BUNDLE_EXTRACT_BASE_DIR="$extraction_root" \
    "$output/Baml.Bridge.NuGetPackageSmoke" \
    | grep -Fx 'csharp_nuget_package_smoke=ok'
  mapfile -d '' extracted < <(native_assets "$extraction_root")
  test "${#extracted[@]}" -eq 1
  test "$(basename "${extracted[0]}")" = "$canonical_native"
  cmp "$package_native" "${extracted[0]}"
}

trimmed_output="$work/trimmed"
dotnet publish "$project" "${common[@]}" "${trimmed[@]}" \
  --output "$trimmed_output" \
  -p:PublishSingleFile=false
run_sidecar "$trimmed_output" false

for trim in false true; do
  trim_args=(-p:PublishTrimmed=false)
  if [[ "$trim" == true ]]; then
    trim_args=("${trimmed[@]}")
  fi

  sidecar="$work/single-$trim-sidecar"
  dotnet publish "$project" "${common[@]}" "${trim_args[@]}" \
    --output "$sidecar" \
    -p:PublishSingleFile=true \
    -p:IncludeNativeLibrariesForSelfExtract=false
  run_sidecar "$sidecar" true

  self_extract="$work/single-$trim-self-extract"
  dotnet publish "$project" "${common[@]}" "${trim_args[@]}" \
    --output "$self_extract" \
    -p:PublishSingleFile=true \
    -p:IncludeNativeLibrariesForSelfExtract=true
  run_self_extract "$self_extract" "$work/extract-$trim"
done

echo "csharp_exact_package_deployment=normal_trimmed_and_single_file_shapes_ok"
