#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

require_command aws
require_command infisical
require_command ssh

target="${1:-}"
case "$target" in
  x64)
    output_key=X64PublicIp
    ;;
  graviton|arm64)
    output_key=GravitonPublicIp
    ;;
  *)
    die "usage: $0 {x64|graviton} [--] [remote command ...]"
    ;;
esac
shift
if [[ "${1:-}" == -- ]]; then
  shift
fi

key_file="$(mktemp "${TMPDIR:-/tmp}/sam-microgc-test-key.XXXXXX")"
trap 'rm -f "$key_file"' EXIT
infisical secrets get "$PRIVATE_KEY_SECRET" "${infisical_args[@]}" --plain >"$key_file"
[[ -s "$key_file" ]] || die "Infisical secret $PRIVATE_KEY_SECRET was not found in environment $INFISICAL_ENV"
chmod 600 "$key_file"

public_ip="$(stack_output "$output_key")"
[[ -n "$public_ip" && "$public_ip" != None ]] || die "stack output $output_key is unavailable"

ssh \
  -i "$key_file" \
  -o IdentitiesOnly=yes \
  -o StrictHostKeyChecking=accept-new \
  "ec2-user@$public_ip" \
  "$@"
