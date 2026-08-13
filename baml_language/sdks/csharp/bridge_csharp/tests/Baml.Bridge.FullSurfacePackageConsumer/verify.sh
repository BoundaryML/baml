#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 3 ]]; then
  echo "usage: verify.sh <exact-package> <rid> <canonical-native-name>" >&2
  exit 2
fi

test_dir="$(cd "$(dirname "$0")" && pwd -P)"
repository_root="$(cd "$test_dir/../../../../../.." && pwd -P)"
language_root="$repository_root/baml_language"
fixture_root="$language_root/sdk_tests/crates/csharp"
package="$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")"
rid="$2"
canonical_native="$3"

test -f "$package"
version="$(unzip -p "$package" baml-bridge.nuspec \
  | sed -n 's#.*<version>\([^<]*\)</version>.*#\1#p')"
if [[ ! "$version" =~ ^[0-9A-Za-z][0-9A-Za-z.+-]*$ ]]; then
  echo "exact package contains an invalid or missing version: $version" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
feed="$work/feed"
packages="$work/packages"
mkdir -p "$feed" "$packages" "$work/consumers"
cp "$package" "$feed/baml-bridge.$version.nupkg"
unzip -p "$package" "runtimes/$rid/native/$canonical_native" \
  > "$work/package-native"
test -s "$work/package-native"

mapfile -t packaged_native_entries < <(
  unzip -Z1 "$package" \
    | grep -E "^runtimes/$rid/native/(bridge_cffi\\.dll|libbridge_cffi\\.dylib|libbridge_cffi\\.so)\$"
)
test "${#packaged_native_entries[@]}" -eq 1
test "${packaged_native_entries[0]}" = "runtimes/$rid/native/$canonical_native"

export BAML_CSHARP_PRODUCT_FEED="$feed"
export NUGET_PACKAGES="$packages"
export DOTNET_CLI_HOME="$work/dotnet-home"
export NUGET_HTTP_CACHE_PATH="$work/nuget-http-cache"
export DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1

native_assets() {
  find "$1" -type f \
    \( -name 'bridge_cffi.dll' \
      -o -name 'libbridge_cffi.dylib' \
      -o -name 'libbridge_cffi.so' \) \
    -print0
}

assert_no_forbidden_publish_assets() {
  local output="$1"
  if find "$output" -type f \
    \( -name '*.proto' \
      -o -name '*.baml' \
      -o -name 'baml.toml' \
      -o -name 'baml-cli*' \
      -o -name '*.cs' \
      -o -name '*.csproj' \) \
    | grep -q .; then
    echo "published fixture contains a source or tooling asset: $output" >&2
    return 1
  fi
}

assert_sidecar_native() {
  local output="$1"
  mapfile -d '' native < <(native_assets "$output")
  test "${#native[@]}" -eq 1
  test "${native[0]}" = "$output/$canonical_native"
  cmp "$work/package-native" "${native[0]}"
}

assert_single_file_inventory() {
  local output="$1"
  local executable_name="$2"
  local assembly="$3"
  local include_native="$4"
  local entry

  while IFS= read -r -d '' entry; do
    test -f "$entry"
    test "$(dirname "$entry")" = "$output"
    case "$(basename "$entry")" in
      "$executable_name"|"$assembly.pdb")
        ;;
      "$canonical_native")
        test "$include_native" = true
        ;;
      *)
        echo "unexpected loose single-file publish asset: $entry" >&2
        return 1
        ;;
    esac
  done < <(find "$output" -mindepth 1 -print0)
}

run_marker() {
  local output="$1"
  local executable_name="$2"
  local marker="$3"
  test -x "$output/$executable_name"
  env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
    "$output/$executable_name" \
    | grep -Fx "$marker"
}

run_sidecar() {
  local output="$1"
  local executable_name="$2"
  local assembly="$3"
  local marker="$4"
  local single_file="$5"

  assert_no_forbidden_publish_assets "$output"
  assert_sidecar_native "$output"
  if [[ "$single_file" == true ]]; then
    assert_single_file_inventory \
      "$output" "$executable_name" "$assembly" true
  else
    test -f "$output/$assembly.dll"
    test -f "$output/$assembly.deps.json"
    test -f "$output/$assembly.runtimeconfig.json"
  fi
  run_marker "$output" "$executable_name" "$marker"
}

run_self_extract() {
  local output="$1"
  local extraction_root="$2"
  local executable_name="$3"
  local assembly="$4"
  local marker="$5"

  assert_no_forbidden_publish_assets "$output"
  mapfile -d '' bundled_output_native < <(native_assets "$output")
  test "${#bundled_output_native[@]}" -eq 0
  assert_single_file_inventory \
    "$output" "$executable_name" "$assembly" false
  mkdir -p "$extraction_root"
  test -x "$output/$executable_name"
  env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
    DOTNET_BUNDLE_EXTRACT_BASE_DIR="$extraction_root" \
    "$output/$executable_name" \
    | grep -Fx "$marker"
  mapfile -d '' extracted < <(native_assets "$extraction_root")
  test "${#extracted[@]}" -eq 1
  test "$(basename "${extracted[0]}")" = "$canonical_native"
  cmp "$work/package-native" "${extracted[0]}"
}

copy_fixture() {
  local fixture="$1"
  local project_name="$2"
  local source="$fixture_root/$fixture"
  local generation="$work/generation/$fixture"
  local destination="$work/consumers/$fixture"
  local source_file
  local relative

  test -f "$source/$project_name"
  test -f "$source/Program.cs"
  test -f "$source/baml.toml"
  test -d "$source/baml_src"
  mkdir -p "$generation" "$destination"
  cp "$source/baml.toml" "$generation/"
  cp -R "$source/baml_src" "$generation/"
  (
    cd "$language_root"
    cargo run --quiet -p baml_cli -- generate --project "$generation"
  )
  while IFS= read -r -d '' source_file; do
    relative="${source_file#"$source/"}"
    mkdir -p "$destination/$(dirname "$relative")"
    cp "$source_file" "$destination/$relative"
  done < <(
    find "$source" -type f \( -name '*.cs' -o -name '*.csproj' \) \
      ! -path '*/baml_client/*' ! -path '*/bin/*' ! -path '*/obj/*' \
      -print0
  )
  while IFS= read -r -d '' source_file; do
    relative="${source_file#"$generation/baml_client/"}"
    mkdir -p "$destination/baml_client/$(dirname "$relative")"
    cp "$source_file" "$destination/baml_client/$relative"
  done < <(
    find "$generation/baml_client" -type f -name '*.g.cs' -print0
  )
  cp "$test_dir/NuGet.Config" "$destination/"

  mapfile -t generated_sources < <(
    find "$destination/baml_client" -type f -name '*.g.cs' \
      -printf '%P\n' | LC_ALL=C sort
  )
  if [[ "${#generated_sources[@]}" -eq 0 ]]; then
    echo "fixture has no generated C# sources: $fixture" >&2
    return 1
  fi
  (
    cd "$generation/baml_client"
    find . -type f -name '*.g.cs' -printf '%P\n' | LC_ALL=C sort
  ) | diff -u - <(printf '%s\n' "${generated_sources[@]}")

  if find "$destination" -type f \
    \( -name '*.proto' -o -name '*.baml' -o -name '*.toml' -o -name '*.bin' \) \
    | grep -q .; then
    echo "clean fixture contains a forbidden source/runtime asset: $fixture" >&2
    return 1
  fi
}

publish_fixture() {
  local fixture="$1"
  local project_name="$2"
  local assembly="$3"
  local marker="$4"
  local single_trim="$5"
  local single_extract="$6"
  local consumer="$work/consumers/$fixture"
  local project="$consumer/$project_name"
  local executable_name="$assembly"
  local output
  local -a common
  local -a trim_args

  if [[ "$rid" == win-* ]]; then
    executable_name="$assembly.exe"
  fi

  copy_fixture "$fixture" "$project_name"
  echo "restoring exact-package fixture: $fixture"
  env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
    dotnet restore "$project" \
      --runtime "$rid" \
      --configfile "$consumer/NuGet.Config" \
      -p:NuGetAudit=false \
      -p:BamlBridgePackageVersion="$version" \
      -p:SelfContained=true \
      -p:PublishTrimmed=true \
      -p:PublishSingleFile=true

  common=(
    --configuration Release
    --runtime "$rid"
    --no-restore
    -p:NuGetAudit=false
    -p:BamlBridgePackageVersion="$version"
    -p:SuppressTrimAnalysisWarnings=false
    -p:TrimmerSingleWarn=false
    -p:ILLinkTreatWarningsAsErrors=true
  )

  output="$work/publish/$fixture/normal"
  dotnet publish "$project" "${common[@]}" \
    --self-contained false \
    --output "$output" \
    -p:PublishTrimmed=false \
    -p:PublishSingleFile=false
  run_sidecar "$output" "$executable_name" "$assembly" "$marker" false

  output="$work/publish/$fixture/trimmed"
  dotnet publish "$project" "${common[@]}" \
    --self-contained true \
    --output "$output" \
    -p:PublishTrimmed=true \
    -p:TrimMode=link \
    -p:PublishSingleFile=false
  run_sidecar "$output" "$executable_name" "$assembly" "$marker" false

  trim_args=(-p:PublishTrimmed=false)
  if [[ "$single_trim" == true ]]; then
    trim_args=(-p:PublishTrimmed=true -p:TrimMode=link)
  fi
  output="$work/publish/$fixture/single-$single_trim-$single_extract"
  dotnet publish "$project" "${common[@]}" \
    --self-contained true \
    --output "$output" \
    "${trim_args[@]}" \
    -p:PublishSingleFile=true \
    -p:IncludeNativeLibrariesForSelfExtract="$single_extract"
  if [[ "$single_extract" == true ]]; then
    run_self_extract \
      "$output" "$work/extract/$fixture" \
      "$executable_name" "$assembly" "$marker"
  else
    run_sidecar \
      "$output" "$executable_name" "$assembly" "$marker" true
  fi
}

started_at="$SECONDS"

# Every real surface runs from the package in normal and trimmed form. The
# third publish distributes every single-file trim/native-carrier combination
# across those surfaces, with dynamic values repeated in the strongest shape.
publish_fixture \
  phase9_media Phase9Media.csproj Baml.CSharp.Phase9Media \
  csharp_phase9_media=ok false false
publish_fixture \
  phase10_stream Phase10Stream.csproj Baml.CSharp.Phase10Stream \
  csharp_phase10_stream_request=ok false true
publish_fixture \
  phase11_host_callable Phase11HostCallable.csproj Baml.Bridge.Tests \
  csharp_phase11_host_callable=ok true false
publish_fixture \
  phase12_resources Phase12Resources.csproj Baml.CSharp.Phase12Resources \
  csharp_phase12_resources=ok true true
publish_fixture \
  phase15_dynamic_values Phase15DynamicValues.csproj Baml.CSharp.Phase15DynamicValues \
  csharp_phase15_dynamic_values=ok true true

mapfile -t restored_packages < <(
  find "$packages/baml-bridge/${version,,}" -type f -name '*.nupkg' -print
)
test "${#restored_packages[@]}" -eq 1
cmp "$package" "${restored_packages[0]}"
package_native="$packages/baml-bridge/${version,,}/runtimes/$rid/native/$canonical_native"
test -f "$package_native"
cmp "$work/package-native" "$package_native"

echo "csharp_full_surface_exact_package=15_publish_shapes_ok"
echo "csharp_full_surface_exact_package_seconds=$((SECONDS - started_at))"
