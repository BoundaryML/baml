#!/bin/sh
set -eu

: "${E2E_CHANNEL:=canary}"
: "${E2E_DISTRO:?E2E_DISTRO is required}"
: "${E2E_TARGET:?E2E_TARGET is required}"
: "${E2E_WRAPPER_VERSION:?E2E_WRAPPER_VERSION is required}"
: "${E2E_WRAPPER_ARCHIVE:=/e2e/wrapper.tar.gz}"
: "${E2E_UPSTREAM_MANIFEST_BASE_URL:=https://pkg.boundaryml.com/manifest/v1}"

case "$E2E_DISTRO" in
  alpine)
    test -f /etc/alpine-release || { echo "error: expected Alpine" >&2; exit 1; }
    apk add --no-cache busybox-extras ca-certificates curl jq
    ;;
  debian)
    test -f /etc/debian_version || { echo "error: expected Debian" >&2; exit 1; }
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y --no-install-recommends busybox ca-certificates curl jq
    rm -rf /var/lib/apt/lists/*
    ;;
  *) echo "error: unsupported distro $E2E_DISTRO" >&2; exit 1 ;;
esac

case "$E2E_TARGET" in
  x86_64-*) expected_machine=x86_64 ;;
  aarch64-*) expected_machine=aarch64 ;;
  *) echo "error: unsupported target $E2E_TARGET" >&2; exit 1 ;;
esac
actual_machine="$(uname -m)"
test "$actual_machine" = "$expected_machine" || { echo "error: uname -m returned $actual_machine, expected $expected_machine" >&2; exit 1; }

case "$E2E_TARGET" in
  *-gnu)
    libc="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
    case "$libc" in glibc*) ;; *) echo "error: expected glibc, got ${libc:-unknown}" >&2; exit 1 ;; esac
    ;;
  *-musl)
    libc="$(ldd --version 2>&1 || true)"
    printf '%s\n' "$libc" | grep -Eiq musl || { echo "error: expected musl, got $libc" >&2; exit 1; }
    ;;
esac

work="$(mktemp -d "${TMPDIR:-/tmp}/baml-wrapper-e2e.XXXXXX")"
server_pid=""
cleanup() {
  if test -n "$server_pid"; then kill "$server_pid" >/dev/null 2>&1 || true; fi
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

web="$work/web"
mkdir -p "$web"
cp "$E2E_WRAPPER_ARCHIVE" "$web/wrapper.tar.gz"
sha256="$(sha256sum "$web/wrapper.tar.gz" | awk '{print $1}')"
cat > "$web/wrapper.json" <<EOF
{"schema":1,"version":"$E2E_WRAPPER_VERSION","released_at":"2026-01-01T00:00:00Z","artifacts":{"$E2E_TARGET":{"url":"http://127.0.0.1:8123/wrapper.tar.gz","sha256":"$sha256"}}}
EOF
curl -fsSL "$E2E_UPSTREAM_MANIFEST_BASE_URL/$E2E_CHANNEL.json" -o "$web/$E2E_CHANNEL.json"
case "$E2E_TARGET" in
  *-gnu)
    portable_target="${E2E_TARGET%-gnu}-musl"
    jq --arg target "$E2E_TARGET" --arg portable_target "$portable_target" '.artifacts[$target] = .artifacts[$portable_target]' "$web/$E2E_CHANNEL.json" > "$web/$E2E_CHANNEL.portable.json"
    mv "$web/$E2E_CHANNEL.portable.json" "$web/$E2E_CHANNEL.json"
    echo "Using released $portable_target CLI artifact to isolate the GNU wrapper ABI test"
    ;;
esac

if command -v httpd >/dev/null 2>&1; then
  httpd -f -p 127.0.0.1:8123 -h "$web" &
else
  busybox httpd -f -p 127.0.0.1:8123 -h "$web" &
fi
server_pid=$!
attempt=0
until curl -fsS http://127.0.0.1:8123/wrapper.json >/dev/null; do
  attempt=$((attempt + 1))
  test "$attempt" -lt 20 || { echo "error: local manifest server did not start" >&2; exit 1; }
  sleep 1
done

export HOME="$work/home"
export BAML_HOME="$HOME/.baml"
export BAML_MANIFEST_BASE_URL="http://127.0.0.1:8123"
mkdir -p "$HOME"
sh /e2e/install.sh --channel "$E2E_CHANNEL" --yes --no-modify-path

installed="$BAML_HOME/bin/baml"
test -x "$installed"
version_output="$("$installed" --version)"
printf '%s\n' "$version_output"
printf '%s\n' "$version_output" | grep -F "baml wrapper $E2E_WRAPPER_VERSION"
printf '%s\n' "$version_output" | grep -F 'baml toolchain '
"$installed" toolchain list

project="$work/project"
"$installed" --color never --no-progress init "$project" --name wrapper-e2e
"$installed" --color never --no-progress --directory "$project" check
echo "E2E PASS: $E2E_DISTRO $E2E_TARGET"
