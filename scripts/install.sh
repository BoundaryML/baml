#!/bin/sh
set -eu

MANIFEST_BASE_URL="${BAML_MANIFEST_BASE_URL:-https://pkg.boundaryml.com/manifest/v1}"
BAML_HOME="${BAML_HOME:-$HOME/.baml}"
CHANNEL="canary"
VERSION=""
WRAPPER_ONLY=0
NO_MODIFY_PATH=0
YES=0

usage() {
  cat <<'USAGE'
Usage: install.sh [--channel canary|nightly] [--version VERSION] [--wrapper-only] [--no-modify-path] [--yes]
USAGE
}

detect_linux_libc() {
  libc_version=""
  if command -v getconf >/dev/null 2>&1; then
    libc_version="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
  fi
  case "$libc_version" in
    glibc*) printf '%s\n' gnu; return 0 ;;
  esac

  ldd_version=""
  if command -v ldd >/dev/null 2>&1; then
    ldd_version="$(ldd --version 2>&1 || true)"
  fi
  if printf '%s\n' "$ldd_version" | grep -Eiq 'musl'; then
    printf '%s\n' musl
    return 0
  fi
  if printf '%s\n' "$ldd_version" | grep -Eiq 'glibc|gnu libc'; then
    printf '%s\n' gnu
    return 0
  fi
  if [ -f /etc/alpine-release ]; then
    printf '%s\n' musl
    return 0
  fi

  # The musl wrapper is self-contained, so it is the safe bootstrap fallback
  # when a minimal Linux environment exposes neither getconf nor a useful ldd.
  printf '%s\n' musl
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --channel) CHANNEL="${2:?missing channel}"; shift 2 ;;
    --version) VERSION="${2:?missing version}"; shift 2 ;;
    --wrapper-only) WRAPPER_ONLY=1; shift ;;
    --no-modify-path) NO_MODIFY_PATH=1; shift ;;
    --yes) YES=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "error: unknown argument $1" >&2; usage >&2; exit 1 ;;
  esac
done

case "$CHANNEL" in
  canary|nightly) ;;
  *) echo "error: --channel must be canary or nightly" >&2; exit 1 ;;
esac

machine="$(uname -m)"
system="$(uname -s)"
case "$system:$machine" in
  Darwin:arm64) target="aarch64-apple-darwin" ;;
  Darwin:x86_64) target="x86_64-apple-darwin" ;;
  Linux:aarch64|Linux:arm64)
    libc="$(detect_linux_libc)" || exit 2
    target="aarch64-unknown-linux-$libc"
    ;;
  Linux:x86_64)
    libc="$(detect_linux_libc)" || exit 2
    target="x86_64-unknown-linux-$libc"
    ;;
  *) echo "error: unsupported platform $system $machine" >&2; exit 2 ;;
esac

tmp="$(mktemp -d "${TMPDIR:-/tmp}/baml-install.XXXXXX")"
if [ -z "$tmp" ] || [ ! -d "$tmp" ]; then
  echo "error: failed to create temporary directory" >&2
  exit 3
fi
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT INT TERM

manifest="$tmp/wrapper.json"
if ! curl -fsSL "$MANIFEST_BASE_URL/wrapper.json" -o "$manifest"; then
  echo "error: failed to download wrapper manifest" >&2
  exit 3
fi

json="$(tr -d '\n' < "$manifest")"
block="$(printf '%s' "$json" | sed -n "s/.*\"$target\"[[:space:]]*:[[:space:]]*{\([^}]*\)}.*/\1/p")"
url="$(printf '%s' "$block" | sed -n 's/.*"url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
sha256="$(printf '%s' "$block" | sed -n 's/.*"sha256"[[:space:]]*:[[:space:]]*"\([0-9a-fA-F]*\)".*/\1/p' | tr 'A-F' 'a-f')"

if [ -z "$url" ] || [ -z "$sha256" ]; then
  echo "error: wrapper manifest has no artifact for $target" >&2
  exit 5
fi

archive="$tmp/wrapper.archive"
if ! curl -fsSL "$url" -o "$archive"; then
  echo "error: failed to download $url" >&2
  exit 3
fi

actual="$(if command -v sha256sum >/dev/null 2>&1; then sha256sum "$archive" | awk '{print $1}'; else shasum -a 256 "$archive" | awk '{print $1}'; fi)"
if [ "$actual" != "$sha256" ]; then
  echo "error: checksum verification failed for wrapper archive" >&2
  exit 4
fi

raw_entries="$tmp/entries.raw.txt"
entries="$tmp/entries.txt"
case "$archive" in
  *.zip) unzip -Z1 "$archive" > "$raw_entries" ;;
  *) tar -tzf "$archive" > "$raw_entries" ;;
esac
sed 's#^\./##' "$raw_entries" > "$entries"

if grep -Eq '(^/|(^|/)\.\.(/|$)|baml-cli|baml-pack-host|baml-vscode\.vsix)' "$entries"; then
  echo "error: wrapper archive layout is not safe" >&2
  exit 5
fi
if ! grep -qx 'bin/baml' "$entries"; then
  echo "error: wrapper archive must contain bin/baml" >&2
  exit 5
fi

extract="$tmp/extract"
mkdir -p "$extract"
case "$archive" in
  *.zip) unzip -q "$archive" -d "$extract" ;;
  *) tar -xzf "$archive" -C "$extract" ;;
esac

mkdir -p "$BAML_HOME/bin"
install_tmp="$BAML_HOME/bin/.baml.tmp"
cp "$extract/bin/baml" "$install_tmp"
chmod 755 "$install_tmp"
mv "$install_tmp" "$BAML_HOME/bin/baml"

cat > "$BAML_HOME/env" <<'ENV'
export BAML_HOME="${BAML_HOME:-$HOME/.baml}"
case ":$PATH:" in
  *":$BAML_HOME/bin:"*) ;;
  *) export PATH="$BAML_HOME/bin:$PATH" ;;
esac
ENV

if [ "$NO_MODIFY_PATH" -eq 0 ] && [ "$YES" -eq 1 ]; then
  line='. "$HOME/.baml/env"'
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    zsh) profiles="$HOME/.zshrc" ;;
    bash) profiles="$HOME/.bashrc" ;;
    *) profiles="" ;;
  esac
  for profile in $profiles; do
    touch "$profile"
    if ! grep -Fqx "$line" "$profile"; then
      printf '\n%s\n' "$line" >> "$profile" || exit 6
    fi
  done
fi

if [ "$WRAPPER_ONLY" -eq 0 ]; then
  selector="$CHANNEL"
  if [ -n "$VERSION" ]; then
    selector="$VERSION"
  fi
  if ! "$BAML_HOME/bin/baml" toolchain use "$selector"; then
    echo "error: wrapper installed, but toolchain bootstrap failed" >&2
    echo "retry: $BAML_HOME/bin/baml toolchain use $selector" >&2
    exit 7
  fi
fi

echo "BAML installed at $BAML_HOME/bin/baml"
if [ "$NO_MODIFY_PATH" -eq 1 ] || [ "$YES" -eq 0 ]; then
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    zsh)
      shell_label="zsh"
      profile_display="\$HOME/.zshrc"
      ;;
    bash)
      shell_label="bash"
      if [ "$system" = "Darwin" ]; then
        profile_display="\$HOME/.bash_profile"
      else
        profile_display="\$HOME/.bashrc"
      fi
      ;;
    fish)
      cat <<'NEXT'

Add BAML to your PATH by running this command in fish:

  fish_add_path "$HOME/.baml/bin"
NEXT
      exit 0
      ;;
    *)
      shell_label="${shell_name:-POSIX}"
      profile_display="\$HOME/.profile"
      ;;
  esac

  cat <<NEXT

Add BAML to your PATH for future $shell_label shells by running:

  grep -Fqx '. "\$HOME/.baml/env"' "$profile_display" 2>/dev/null || printf '\n%s\n' '. "\$HOME/.baml/env"' >> "$profile_display"

Then load BAML in this shell:

  . "\$HOME/.baml/env"
NEXT
fi
