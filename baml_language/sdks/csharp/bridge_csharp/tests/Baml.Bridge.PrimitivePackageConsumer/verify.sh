#!/usr/bin/env bash
set -euo pipefail

verify_repository_paths() (
  local repository_root="$1"
  local publish="$2"
  local repository_path_prefix
  local scan_status

  # Include the path separator so a short mount point such as /work does not
  # mistake an unrelated path segment such as /worker.rs for a repository path.
  repository_path_prefix="${repository_root%/}/"
  if grep -r -a -F -l -- "$repository_path_prefix" "$publish" > /dev/null; then
    echo "published consumer contains a repository path" >&2
    return 1
  else
    scan_status="$?"
    if [[ "$scan_status" -ne 1 ]]; then
      echo "failed to scan published consumer for repository paths" >&2
      return "$scan_status"
    fi
  fi
)

if [[ "${1:-}" == "--verify-repository-paths" ]]; then
  if [[ "$#" -ne 3 ]]; then
    echo "usage: verify.sh --verify-repository-paths <repository-root> <publish-dir>" >&2
    exit 2
  fi
  verify_repository_paths "$2" "$3"
  exit 0
fi

if [[ "$#" -lt 1 || "$#" -gt 4 ]]; then
  echo "usage: verify.sh <exact-package> [rid] [canonical-native-name] [generated-source-root]" >&2
  exit 2
fi

test_dir="$(cd "$(dirname "$0")" && pwd -P)"
repository_root="$(cd "$test_dir/../../../../../.." && pwd -P)"
language_root="$repository_root/baml_language"
fixture="$language_root/sdk_tests/crates/csharp/primitive_slice"
package="$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")"
rid="${2:-linux-x64}"
canonical_native="${3:-libbridge_cffi.so}"
generated_source_root="${4:-}"

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
consumer="$work/consumer"
packages="$work/packages"
publish="$work/publish"
mkdir -p "$feed" "$consumer/baml_client" "$packages" "$publish"
cp "$package" "$feed/baml-bridge.$version.nupkg"

if [[ -z "$generated_source_root" ]]; then
  (cd "$language_root" && cargo run --quiet -p baml_cli -- \
    generate --project "$fixture")
  generated_source_root="$fixture/baml_client"
else
  generated_source_root="$(cd "$generated_source_root" && pwd -P)"
fi
test -f "$generated_source_root/Baml/Generated/BamlProgram.g.cs"
test -f "$generated_source_root/CsharpSlice/Functions.g.cs"
cp "$test_dir/Baml.Bridge.PrimitivePackageConsumer.csproj" "$consumer/"
cp "$test_dir/NuGet.Config" "$consumer/"
cp "$test_dir/Program.cs" "$consumer/"
while IFS= read -r source; do
  relative="${source#"$generated_source_root/"}"
  destination="$consumer/baml_client/$relative"
  mkdir -p "$(dirname "$destination")"
  cp "$source" "$destination"
done < <(find "$generated_source_root" -type f -name '*.g.cs' | LC_ALL=C sort)

list_generated_sources() {
  (
    cd "$1"
    find . -type f -name '*.g.cs' -print \
      | sed 's#^\./##' \
      | LC_ALL=C sort
  )
}

diff -u \
  <(list_generated_sources "$generated_source_root") \
  <(list_generated_sources "$consumer/baml_client")
if find "$consumer" -type f \
  \( -name '*.proto' -o -name '*.baml' -o -name '*.toml' -o -name '*.bin' \) \
  | grep -q .; then
  echo "clean consumer contains a forbidden source/runtime asset" >&2
  exit 1
fi

dotnet_feed="$feed"
dotnet_packages="$packages"
dotnet_mismatch_packages="$work/mismatch-packages"
if [[ "${RUNNER_OS:-}" == "Windows" ]]; then
  dotnet_feed="$(cygpath -am "$feed")"
  dotnet_packages="$(cygpath -am "$packages")"
  dotnet_mismatch_packages="$(cygpath -am "$work/mismatch-packages")"
fi
export BAML_CSHARP_PRODUCT_FEED="$dotnet_feed"
project="$consumer/Baml.Bridge.PrimitivePackageConsumer.csproj"
config="$consumer/NuGet.Config"
env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
  NUGET_PACKAGES="$dotnet_packages" \
  dotnet restore "$project" --runtime "$rid" --configfile "$config" \
    -p:NuGetAudit=false \
    -p:BamlBridgePackageVersion="$version"
restored_product_package_count="$(find "$packages/baml-bridge" \
  -type f -name '*.nupkg' | wc -l | tr -d ' ')"
test "$restored_product_package_count" -eq 1
restored_product_package="$(find "$packages/baml-bridge" \
  -type f -name '*.nupkg' | sed -n '1p')"
cmp "$package" "$restored_product_package"

env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
  NUGET_PACKAGES="$dotnet_packages" \
  dotnet publish "$project" \
    --configuration Release \
    --runtime "$rid" \
    --self-contained false \
    --no-restore \
    --output "$publish" \
    -p:NuGetAudit=false \
    -p:BamlBridgePackageVersion="$version"

native_count="$(find "$publish" -type f \
  \( -name bridge_cffi.dll -o -name libbridge_cffi.dylib -o -name libbridge_cffi.so \) \
  | wc -l)"
test "$native_count" -eq 1
test -f "$publish/$canonical_native"
unzip -p "$package" "runtimes/$rid/native/$canonical_native" \
  > "$work/package-native"
cmp "$work/package-native" "$publish/$canonical_native"

env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
  dotnet "$publish/Baml.CSharp.PrimitivePackageConsumer.dll" \
  | grep -Fx 'csharp_primitive_package=ok'

if env NUGET_PACKAGES="$dotnet_packages" \
  dotnet build "$project" \
    --configuration Release \
    --runtime linux-s390x \
    --no-restore \
    -p:NuGetAudit=false \
    -p:BamlBridgePackageVersion="$version" \
    > "$work/unsupported-rid.log" 2>&1; then
  echo "build unexpectedly accepted an unsupported RID" >&2
  exit 1
fi
if ! grep -F 'BAML0010' "$work/unsupported-rid.log"; then
  cat "$work/unsupported-rid.log" >&2
  exit 1
fi

if env NUGET_PACKAGES="$dotnet_packages" \
  dotnet build "$project" \
    --configuration Release \
    --runtime "$rid" \
    --no-restore \
    -p:NuGetAudit=false \
    -p:BamlBridgePackageVersion="$version" \
    -p:PublishAot=true \
    > "$work/native-aot.log" 2>&1; then
  echo "build unexpectedly accepted NativeAOT" >&2
  exit 1
fi
if ! grep -F 'BAML0019' "$work/native-aot.log"; then
  cat "$work/native-aot.log" >&2
  exit 1
fi

if find "$publish" -type f \
  \( -name '*.proto' -o -name '*.baml' -o -name 'baml-cli*' \
     -o -name 'baml.toml' -o -name '*.csproj' \) \
  | grep -q .; then
  echo "published consumer contains a forbidden source/tooling asset" >&2
  exit 1
fi
verify_repository_paths "$repository_root" "$publish"

mismatch="$work/mismatch"
mkdir -p "$mismatch"
cp "$test_dir/Baml.Bridge.PrimitivePackageConsumer.csproj" "$mismatch/"
cp "$test_dir/NuGet.Config" "$mismatch/"
cp "$test_dir/Program.cs" "$mismatch/"
mismatch_version="9999.0.0"
if env NUGET_PACKAGES="$dotnet_mismatch_packages" \
  dotnet restore "$mismatch/Baml.Bridge.PrimitivePackageConsumer.csproj" \
    --configfile "$mismatch/NuGet.Config" \
    -p:NuGetAudit=false \
    -p:BamlBridgePackageVersion="$mismatch_version" \
    > "$work/mismatch.log" 2>&1; then
  echo "restore unexpectedly accepted a mismatched package version" >&2
  exit 1
fi
grep -F "$mismatch_version" "$work/mismatch.log"

echo "primitive_exact_package=ok"
