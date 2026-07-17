#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 6 ]]; then
  echo "usage: $0 <baml-cli> <package.nupkg> <version> <rid> <native-asset> <work-root>" >&2
  exit 2
fi

baml_cli=$1
package_input=$2
version=$3
rid=$4
native_asset=$5
work_input=$6

if [[ ! -f "$baml_cli" || ! -x "$baml_cli" ]]; then
  echo "baml-cli is not an executable file: $baml_cli" >&2
  exit 2
fi
if [[ ! -f "$package_input" || "$package_input" != *.nupkg || "$package_input" == *.snupkg ]]; then
  echo "NuGet package is not a regular .nupkg: $package_input" >&2
  exit 2
fi
if [[ -z "$version" || -z "$rid" || -z "$native_asset" ]]; then
  echo "version, RID, and native asset must be non-empty" >&2
  exit 2
fi
if [[ -n "${BAML_RUNTIME_PATH:-}"
  || -n "${BAML_BRIDGE_LIBRARY:-}"
  || -n "${BAML_LIBRARY_PATH:-}" ]]; then
  echo "native runtime overrides must be unset for the clean NuGet consumer smoke" >&2
  exit 2
fi

rm -rf -- "$work_input"
mkdir -p -- "$work_input"
work_root=$(cd -- "$work_input" && pwd -P)
mkdir -p "$work_root/baml_src" "$work_root/local-feed"
cp -- "$package_input" "$work_root/local-feed/"

cat > "$work_root/baml.toml" <<'EOF'
[package]
name = "csharp-package-verification"

[generator.csharp]
output_type = "csharp"
output_dir = "."
naming_convention = "language"
EOF

cat > "$work_root/baml_src/main.baml" <<'EOF'
function verify_release(value: string) -> string {
  value
}
EOF

"$baml_cli" generate --from "$work_root"
test -f "$work_root/baml_sdk/BamlGeneratedProgram.g.cs"
grep -Fq "$version" "$work_root/baml_sdk/BamlGeneratedProgram.g.cs"

cat > "$work_root/Consumer.csproj" <<EOF
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <NuGetAudit>false</NuGetAudit>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="baml-bridge" Version="$version" />
  </ItemGroup>
</Project>
EOF

cat > "$work_root/NuGet.Config" <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="baml-local" value="./local-feed" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="baml-local">
      <package pattern="baml-bridge" />
    </packageSource>
    <packageSource key="nuget.org">
      <package pattern="Google.Protobuf" />
    </packageSource>
  </packageSourceMapping>
</configuration>
EOF

cat > "$work_root/Program.cs" <<'EOF'
using System.Reflection;
using System.Runtime.InteropServices;
using Baml;
using BamlSdk;

if (args is not [var expectedVersion, var expectedRid])
{
    throw new InvalidOperationException("expected version and RID arguments");
}
if (Environment.GetEnvironmentVariable("BAML_RUNTIME_PATH") is not null
    || Environment.GetEnvironmentVariable("BAML_BRIDGE_LIBRARY") is not null
    || Environment.GetEnvironmentVariable("BAML_LIBRARY_PATH") is not null)
{
    throw new InvalidOperationException("native runtime override leaked into clean consumer");
}

var managedVersion = typeof(BamlBridge).Assembly
    .GetCustomAttributes<AssemblyMetadataAttribute>()
    .Single(attribute => attribute.Key == "BamlSdkVersion")
    .Value;
if (!string.Equals(managedVersion, expectedVersion, StringComparison.Ordinal))
{
    throw new InvalidOperationException(
        $"managed SDK version {managedVersion} does not match {expectedVersion}");
}

const string expected = "clean-package-reference";
var actual = Functions.VerifyRelease(expected);
if (!string.Equals(actual, expected, StringComparison.Ordinal))
{
    throw new InvalidOperationException($"BAML call returned {actual}");
}

Console.WriteLine($"version: {managedVersion}");
Console.WriteLine($"claimed-rid: {expectedRid}");
Console.WriteLine($"runtime-rid: {RuntimeInformation.RuntimeIdentifier}");
Console.WriteLine("native-call: ok");
EOF

dotnet restore "$work_root/Consumer.csproj" \
  --runtime "$rid" \
  --configfile "$work_root/NuGet.Config" \
  --force \
  --no-cache

assets="$work_root/obj/project.assets.json"
resolved_version=$(jq -er '
  [.libraries
    | to_entries[]
    | select(.key | startswith("baml-bridge/"))
    | .key
    | sub("^baml-bridge/"; "")]
  | if length == 1 then .[0]
    else error("expected exactly one resolved baml-bridge package")
    end
' "$assets")
if [[ "$resolved_version" != "$version" ]]; then
  echo "resolved baml-bridge $resolved_version, expected exact version $version" >&2
  exit 1
fi

runtime_entry="runtimes/$rid/native/$native_asset"
jq -e --arg entry "$runtime_entry" --arg rid "$rid" '
  [.targets
    | to_entries[]
    | .value
    | to_entries[]
    | select(.key | startswith("baml-bridge/"))
    | (.value.runtimeTargets // {})
    | to_entries[]
    | select(.key == $entry and .value.rid == $rid and .value.assetType == "native")]
  | length == 1
' "$assets"

publish_dir="$work_root/publish"
dotnet publish "$work_root/Consumer.csproj" \
  --configuration Release \
  --runtime "$rid" \
  --self-contained false \
  --no-restore \
  --output "$publish_dir"
mapfile -t published_native < <(
  find "$publish_dir" -type f \
    \( -name 'libbridge_cffi.so' \
      -o -name 'libbridge_cffi.dylib' \
      -o -name 'bridge_cffi.dll' \) \
    -print
)
if [[ ${#published_native[@]} -ne 1
  || "${published_native[0]}" != "$publish_dir/$native_asset" ]]; then
  echo "publish output did not select exactly $native_asset for $rid" >&2
  printf '  %s\n' "${published_native[@]}" >&2
  exit 1
fi

output=$(
  cd -- "$publish_dir"
  env -u BAML_RUNTIME_PATH -u BAML_BRIDGE_LIBRARY -u BAML_LIBRARY_PATH \
    dotnet Consumer.dll "$version" "$rid"
)
printf '%s\n' "$output"
grep -Fqx "version: $version" <<<"$output"
grep -Fqx "claimed-rid: $rid" <<<"$output"
grep -Fqx "native-call: ok" <<<"$output"
