#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 4 ]]; then
  echo "usage: verify.sh <exact-package> <rid> <canonical-native-name> <generated-source-root>" >&2
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
publish="$work/publish"
mkdir -p "$feed" "$consumer/baml_client" "$packages" "$publish"
cp "$package" "$feed/baml-bridge.$version.nupkg"
cp "$test_dir/Baml.Bridge.DocumentationConsumer.csproj" "$consumer/"
cp "$test_dir/Program.cs" "$consumer/"
cp -R "$generated_source_root/." "$consumer/baml_client/"
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
project="$consumer/Baml.Bridge.DocumentationConsumer.csproj"
NUGET_PACKAGES="$packages" dotnet restore "$project" \
  --runtime "$rid" \
  --configfile "$consumer/NuGet.Config" \
  -p:NuGetAudit=false \
  -p:BamlBridgePackageVersion="$version" \
  -p:BamlGeneratedSourceRoot="$consumer/baml_client"
NUGET_PACKAGES="$packages" dotnet publish "$project" \
  --configuration Release \
  --runtime "$rid" \
  --self-contained false \
  --no-restore \
  --output "$publish" \
  -p:NuGetAudit=false \
  -p:BamlBridgePackageVersion="$version" \
  -p:BamlGeneratedSourceRoot="$consumer/baml_client"
test -f "$publish/$canonical_native"
env -u BAML_BRIDGE_CSHARP_NATIVE_LIBRARY \
  dotnet "$publish/Baml.Bridge.DocumentationConsumer.dll" \
  | grep -Fx 'csharp_documentation_consumer=ok'

echo "csharp_documentation_exact_package=ok"
