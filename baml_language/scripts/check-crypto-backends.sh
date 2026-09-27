#!/usr/bin/env bash
# Each TLS crypto backend must link only its own crypto library:
#   aws-crypto      -> AWS-LC, no ring
#   ring-crypto     -> ring, no AWS-LC
#   external-crypto -> neither (the host supplies the provider)
# A crate that hard-selects a provider (e.g. `reqwest/rustls`, which pins
# AWS-LC) outside these features breaks that, and this catches it.
set -euo pipefail
cd "$(dirname "$0")/.."

# package  features-that-turn-on-http
targets=(
  "baml_cli -"
  "baml_lsp_server -"
  "bridge_cffi bundle-http"
  "bridge_python bundle-http"
  "bridge_java bundle-http"
  "bridge_swift bundle-http"
)

has() { # has <package> <features> <crate>
  local out
  # `-i` prints nothing (and says so) when the crate is only reachable through
  # dev-dependencies or other targets, and errors when it isn't in the
  # lockfile's graph at all.
  if ! out=$(cargo tree --locked -p "$1" --no-default-features --features "$2" \
      -e normal,build --target all -i "$3" 2>&1); then
    grep -q "did not match any packages" <<<"$out" && return 1
    echo "$out" >&2
    exit 2
  fi
  ! grep -q "nothing to print" <<<"$out"
}

status=0
for target in "${targets[@]}"; do
  read -r pkg extra <<<"$target"
  for backend in aws-crypto ring-crypto external-crypto; do
    features=$backend
    [[ $extra != - ]] && features="$extra,$backend"
    case $backend in
      aws-crypto) want=(aws-lc-rs) forbid=(ring) ;;
      ring-crypto) want=(ring) forbid=(aws-lc-rs aws-lc-sys) ;;
      external-crypto) want=() forbid=(ring aws-lc-rs aws-lc-sys) ;;
    esac
    for crate in "${forbid[@]}"; do
      if has "$pkg" "$features" "$crate"; then
        echo "FAIL: $pkg [$features] links $crate:" >&2
        cargo tree --locked -p "$pkg" --no-default-features --features "$features" \
          -e normal,build,features --target all -i "$crate" >&2 || true
        status=1
      fi
    done
    for crate in "${want[@]}"; do
      if ! has "$pkg" "$features" "$crate"; then
        echo "FAIL: $pkg [$features] does not link $crate" >&2
        status=1
      fi
    done
    [[ $status == 0 ]] && echo "ok: $pkg [$features]"
  done
done
exit $status
