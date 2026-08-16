#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 5 ]]; then
  echo "usage: pack-product.sh <staged-native-root> <native-sha256-manifest> <native-provenance.json> <release-plan.json> <output-directory>" >&2
  exit 2
fi

tool_dir="$(cd "$(dirname "$0")" && pwd -P)"
repository_root="$(cd "$tool_dir/../../../../../" && pwd -P)"
project="$tool_dir/../src/Baml.Bridge.csproj"
target_template="$tool_dir/../src/baml-bridge.targets.in"
normalizer="$tool_dir/Baml.NuGetNormalizer/Baml.NuGetNormalizer.csproj"
platforms="$repository_root/baml_language/crates/baml_release/platforms.json"
release_contract="$repository_root/scripts/baml-csharp-release-contract"
expected_exports="$repository_root/release/bridge-cffi-public-exports.txt"
native_root="$(cd "$1" && pwd -P)"
native_manifest="$(cd "$(dirname "$2")" && pwd -P)/$(basename "$2")"
native_provenance="$(cd "$(dirname "$3")" && pwd -P)/$(basename "$3")"
release_plan="$(cd "$(dirname "$4")" && pwd -P)/$(basename "$4")"
output_directory="$5"

for command in jq llvm-nm llvm-objdump llvm-readobj sha256sum unzip; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "required package inspection command is unavailable: $command" >&2
    exit 1
  fi
done

canonical_version="$(jq -er \
  '.canonical_version | select(type == "string" and length > 0)' \
  "$release_plan")"
nuget_version="$(jq -er \
  '.registry_versions.nuget | select(type == "string" and length > 0)' \
  "$release_plan")"
expected_native_source_sha="$(git -C "$repository_root" rev-parse HEAD)"
[[ "$expected_native_source_sha" =~ ^[0-9a-f]{40}$ ]]

if [[ "$output_directory" != /* ]]; then
  output_directory="$PWD/$output_directory"
fi
mkdir -p "$output_directory"
output_directory="$(cd "$output_directory" && pwd -P)"
output_package="$output_directory/baml-bridge.$nuget_version.nupkg"
if [[ -e "$output_package" ]]; then
  echo "package output already exists: $output_package" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
expected_runtime_entries="$work/expected-runtime-entries.txt"
actual_runtime_entries="$work/actual-runtime-entries.txt"

jq -r '
  [.targets[]
    | select(.artifacts.csharp != null)
    | "runtimes/\(.artifacts.csharp.rid)/native/\(.artifacts.csharp.native_asset)"]
  | sort[]' "$platforms" > "$expected_runtime_entries"
find "$native_root/runtimes" -type f -printf 'runtimes/%P\n' \
  | LC_ALL=C sort > "$actual_runtime_entries"
diff -u "$expected_runtime_entries" "$actual_runtime_entries"
expected_native_count="$(wc -l < "$expected_runtime_entries")"
test "$expected_native_count" -gt 0
test "$(wc -l < "$actual_runtime_entries")" -eq "$expected_native_count"
awk '{print $2}' "$native_manifest" | LC_ALL=C sort \
  > "$work/manifest-runtime-entries.txt"
diff -u "$expected_runtime_entries" "$work/manifest-runtime-entries.txt"
(cd "$native_root" && sha256sum -c "$native_manifest")

"$release_contract" verify-product \
  --platforms "$platforms" \
  --release-plan "$release_plan" \
  --source-sha "$expected_native_source_sha" \
  --provenance "$native_provenance" \
  --manifest "$native_manifest" \
  --native-root "$native_root"

inspection_root="$work/native-inspection"
mkdir -p "$inspection_root"
jq -r '
  .targets[]
  | select(.artifacts.csharp != null and .artifacts.cffi != null)
  | [.triple,
     .os,
     .arch,
     (.libc // "none"),
     .artifacts.csharp.rid,
     .artifacts.csharp.native_asset]
  | @tsv' "$platforms" |
while IFS=$'\t' read -r target os arch libc rid canonical; do
  native="$native_root/runtimes/$rid/native/$canonical"
  inspection="$inspection_root/$rid.txt"
  exports="$inspection_root/$rid.exports.txt"
  llvm-readobj \
    --file-headers \
    --sections \
    --needed-libs \
    --dynamic-table \
    --macho-version-min \
    --coff-debug-directory \
    --coff-exports \
    "$native" > "$inspection"

  if grep -Eq \
    'Name: (\.(debug|zdebug)[^ ]*|\.gnu_debug[^ ]*|\.stab[^ ]*|\.gdb_index|\.ctf|\.SUNW_ctf|\.BTF[^ ]*|\.(apple_|line|mdebug)[^ ]*|(\.|__)(llvm_cov|llvm_prf)[^ ]*|__(debug|apple_)[^ ]*)( |$)' \
    "$inspection"; then
    echo "shipping native contains a debug section: $rid" >&2
    exit 1
  fi

  case "$os:$arch" in
    linux:aarch64)
      grep -Fq 'Format: elf64-littleaarch64' "$inspection"
      grep -Fq 'Machine: EM_AARCH64' "$inspection"
      ;;
    linux:x86_64)
      grep -Fq 'Format: elf64-x86-64' "$inspection"
      grep -Fq 'Machine: EM_X86_64' "$inspection"
      ;;
    macos:aarch64)
      grep -Fq 'Format: Mach-O arm64' "$inspection"
      grep -Fq 'Arch: aarch64' "$inspection"
      grep -Fq 'FileType: DynamicLibrary' "$inspection"
      ;;
    macos:x86_64)
      grep -Fq 'Format: Mach-O 64-bit x86-64' "$inspection"
      grep -Fq 'Arch: x86_64' "$inspection"
      grep -Fq 'FileType: DynamicLibrary' "$inspection"
      ;;
    windows:aarch64)
      grep -Fq 'Format: COFF-ARM64' "$inspection"
      grep -Fq 'Machine: IMAGE_FILE_MACHINE_ARM64' "$inspection"
      grep -Fq 'IMAGE_FILE_DLL' "$inspection"
      ;;
    windows:x86_64)
      grep -Fq 'Format: COFF-x86-64' "$inspection"
      grep -Fq 'Machine: IMAGE_FILE_MACHINE_AMD64' "$inspection"
      grep -Fq 'IMAGE_FILE_DLL' "$inspection"
      ;;
    *)
      echo "unsupported package platform tuple: $target/$os/$arch" >&2
      exit 1
      ;;
  esac

  case "$os" in
    linux)
      grep -Fq 'Type: SharedObject' "$inspection"
      if [[ "$libc" == "musl" ]]; then
        grep -Fxq '  libc.so' "$inspection"
        if grep -Fq 'libc.so.6' "$inspection"; then
          echo "musl RID contains a glibc dependency: $rid" >&2
          exit 1
        fi
      else
        grep -Fxq '  libc.so.6' "$inspection"
      fi
      if grep -Eq 'Name: \.symtab( |$)' "$inspection"; then
        echo "shipping ELF contains a symbol table: $rid" >&2
        exit 1
      fi
      if grep -Eq '(RPATH|RUNPATH)' "$inspection"; then
        echo "shipping ELF contains an RPATH/RUNPATH: $rid" >&2
        exit 1
      fi
      llvm-nm --dynamic --defined-only --extern-only "$native" \
        | awk 'NF { print $NF }' \
        | sed 's/@.*$//' \
        | LC_ALL=C sort -u > "$exports"
      ;;
    macos)
      if grep -Fq 'Segment: __DWARF' "$inspection"; then
        echo "shipping Mach-O contains a DWARF segment: $rid" >&2
        exit 1
      fi
      llvm-objdump --macho --rpaths "$native" \
        > "$inspection_root/$rid.rpaths.txt"
      if awk 'NR > 1 && NF { found = 1 } END { exit !found }' \
        "$inspection_root/$rid.rpaths.txt"; then
        echo "shipping Mach-O contains an RPATH: $rid" >&2
        exit 1
      fi
      llvm-objdump --macho --dylib-id "$native" \
        > "$inspection_root/$rid.dylib-id.txt"
      test "$(awk 'NR > 1 && NF { print }' \
        "$inspection_root/$rid.dylib-id.txt")" = \
        '@rpath/libbridge_cffi.dylib'
      # A stripped shipping dylib has no local defined symbols; its complete
      # defined-symbol set is the public export allowlist.
      llvm-nm --defined-only "$native" \
        | awk 'NF { print $NF }' \
        | sed 's/^_//' \
        | LC_ALL=C sort -u > "$exports"
      ;;
    windows)
      grep -Fq 'PointerToSymbolTable: 0x0' "$inspection"
      grep -Fq 'SymbolCount: 0' "$inspection"
      llvm-readobj --coff-exports "$native" \
        | awk '/^  Name: / { print $2 }' \
        | LC_ALL=C sort -u > "$exports"
      ;;
  esac
  diff -u "$expected_exports" "$exports"

  "$repository_root/scripts/baml-bridge-cffi-hygiene" verify \
    --native "$native" \
    --target "$target"
done

supported_rids="$(jq -r '
  [.targets[]
    | select(.artifacts.csharp != null)
    | .artifacts.csharp.rid]
  | join(";")' "$platforms")"
generated_targets="$work/baml-bridge.targets"
sed "s|@BAML_SUPPORTED_RIDS@|$supported_rids|" \
  "$target_template" > "$generated_targets"
if grep -Fq '@BAML_SUPPORTED_RIDS@' "$generated_targets"; then
  echo "supported RID substitution did not complete" >&2
  exit 1
fi

mkdir -p "$work/raw-a" "$work/raw-b" "$work/normalized"
dotnet build "$normalizer" --configuration Release --nologo \
  -p:NuGetAudit=false
dotnet build "$project" --configuration Release --nologo \
  -p:NuGetAudit=false \
  -p:Version="$canonical_version" \
  -p:InformationalVersion="$canonical_version" \
  -p:PackageVersion="$nuget_version"
for raw in raw-a raw-b; do
  dotnet pack "$project" --configuration Release --nologo \
    --no-build --no-restore \
    --output "$work/$raw" \
    -p:NuGetAudit=false \
    -p:Version="$canonical_version" \
    -p:InformationalVersion="$canonical_version" \
    -p:PackageVersion="$nuget_version" \
    -p:BamlExpectedNativeAssetCount="$expected_native_count" \
    -p:BamlNativeAssetRoot="$native_root" \
    -p:BamlGeneratedTargetsPath="$generated_targets"
done

for suffix in a b; do
  dotnet run --project "$normalizer" \
    --configuration Release --no-build --no-restore -- \
    "$work/raw-$suffix/baml-bridge.$nuget_version.nupkg" \
    "$work/normalized/package-$suffix.nupkg"
done
cmp "$work/normalized/package-a.nupkg" \
  "$work/normalized/package-b.nupkg"

awk '{ print $1 "  " $2 }' "$native_manifest" \
  | LC_ALL=C sort -k2,2 > "$work/bound-native-manifest.sha256"
for suffix in a b; do
  package="$work/normalized/package-$suffix.nupkg"
  package_manifest="$work/package-$suffix-native-manifest.sha256"
  : > "$package_manifest"
  while IFS= read -r runtime_path; do
    digest="$(unzip -p "$package" "$runtime_path" \
      | sha256sum | cut -d ' ' -f 1)"
    printf '%s  %s\n' "$digest" "$runtime_path" \
      >> "$package_manifest"
  done < "$expected_runtime_entries"
  diff -u "$work/bound-native-manifest.sha256" "$package_manifest"
done

unzip -Z1 "$work/normalized/package-a.nupkg" \
  | LC_ALL=C sort > "$work/actual-package-entries.txt"
{
  echo '[Content_Types].xml'
  echo '_rels/.rels'
  echo 'README.md'
  echo 'baml-bridge.nuspec'
  echo 'buildTransitive/baml-bridge.targets'
  echo 'lib/net10.0/Baml.Bridge.dll'
  echo 'package/services/metadata/core-properties/core-properties.psmdcp'
  cat "$expected_runtime_entries"
} | LC_ALL=C sort > "$work/expected-package-entries.txt"
diff -u "$work/expected-package-entries.txt" \
  "$work/actual-package-entries.txt"
expected_package_entry_count="$((expected_native_count + 7))"
test "$(wc -l < "$work/actual-package-entries.txt")" \
  -eq "$expected_package_entry_count"
unzip -p "$work/normalized/package-a.nupkg" baml-bridge.nuspec \
  > "$work/baml-bridge.nuspec"
grep -Fq '<dependency id="Google.Protobuf"' "$work/baml-bridge.nuspec"
if grep -Fq 'Grpc.Tools' "$work/baml-bridge.nuspec"; then
  echo "Grpc.Tools leaked into the product package dependency graph" >&2
  exit 1
fi

"$release_contract" verify-product \
  --platforms "$platforms" \
  --release-plan "$release_plan" \
  --source-sha "$expected_native_source_sha" \
  --provenance "$native_provenance" \
  --manifest "$native_manifest" \
  --native-root "$native_root" \
  --package "$work/normalized/package-a.nupkg"

cp "$work/normalized/package-a.nupkg" "$output_package"
cmp "$work/normalized/package-a.nupkg" "$output_package"
echo "product_package=$output_package"
echo "sha256=$(sha256sum "$output_package" | cut -d ' ' -f 1)"
