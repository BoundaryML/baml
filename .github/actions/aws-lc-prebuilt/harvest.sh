#!/usr/bin/env bash
# Packages the AWS-LC library that an ordinary from-source `aws-lc-sys` build
# just produced into the install layout `AWS_LC_SYS_SYSTEM_DIR` expects, so
# later runs can link it instead of recompiling ~260 C files.
#
# Reusing the crate's own output (rather than a separate CMake build) keeps the
# library byte-identical to what a from-source build links, including the
# `aws_lc_<version>_` symbol prefix.
#
# Usage: harvest.sh <aws-lc-sys-version> <dest-dir>
# Run from the cargo workspace root. Exits 0 without creating <dest-dir> when
# there is nothing unambiguous to harvest.
set -euo pipefail

version="$1"
dest="$2"
target_dir="${CARGO_TARGET_DIR:-target}"

libs=()
while IFS= read -r lib; do
    libs+=("$lib")
done < <(find "$target_dir" -type f \
    \( -name 'libaws_lc_*_crypto.a' -o -name 'aws_lc_*_crypto.lib' \) \
    -path '*/build/aws-lc-sys-*/out/*' 2>/dev/null)

if [ "${#libs[@]}" -ne 1 ]; then
    echo "::warning::aws-lc harvest skipped: expected 1 built library, found ${#libs[@]}"
    exit 0
fi

lib="${libs[0]}"
out_dir="$(dirname "$lib")"
build_dir="$(dirname "$out_dir")"

crate_src=""
for candidate in "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/aws-lc-sys-"$version"/src; do
    [ -d "$candidate" ] && crate_src="$candidate"
done
if [ -z "$crate_src" ]; then
    echo "::warning::aws-lc harvest skipped: aws-lc-sys $version sources not found"
    exit 0
fi

# The build script selects one pregenerated bindings module by emitting its
# name as a cfg (e.g. `x86_64_unknown_linux_gnu`, or `universal` on macOS).
bindings=""
while IFS= read -r cfg; do
    [ -f "$crate_src/${cfg}_crypto.rs" ] && bindings="$crate_src/${cfg}_crypto.rs"
done < <(sed -n 's/^cargo:rustc-cfg=//p' "$build_dir/output")
if [ -z "$bindings" ]; then
    echo "::warning::aws-lc harvest skipped: no pregenerated bindings for this target"
    exit 0
fi

for header in base.h boringssl_prefix_symbols.h; do
    if [ ! -f "$out_dir/include/openssl/$header" ]; then
        echo "::warning::aws-lc harvest skipped: $header missing from build output"
        exit 0
    fi
done

rm -rf "$dest"
mkdir -p "$dest/lib" "$dest/share/rust"
cp -R "$out_dir/include" "$dest/include"

# `crypto-awslc` is the alternate name aws-lc-sys accepts. The default,
# `crypto`, would shadow OpenSSL's libcrypto for anything else linking it.
case "$lib" in
    *.lib) cp "$lib" "$dest/lib/crypto-awslc.lib" ;;
    *)     cp "$lib" "$dest/lib/libcrypto-awslc.a" ;;
esac

# The system-install path `include!`s the bindings inside a module, where the
# file's leading inner attribute (`#![allow(...)]`) is not permitted.
awk '/^#!\[allow\($/ { skip = 1 } !skip { print } skip && /^\)\]$/ { skip = 0 }' \
    "$bindings" > "$dest/share/rust/aws_lc_bindings.rs"

echo "Harvested $lib into $dest"
