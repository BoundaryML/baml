#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

for command_name in aws curl infisical ssh-keygen; do
  require_command "$command_name"
done

check_aws_account

if [[ -z "${SSH_CIDR:-}" ]]; then
  public_ip="$(curl --fail --silent --show-error https://checkip.amazonaws.com | tr -d '[:space:]')"
  SSH_CIDR="$public_ip/32"
fi

if [[ ! "$SSH_CIDR" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}/([0-9]|[12][0-9]|3[0-2])$ ]]; then
  die "SSH_CIDR must be an IPv4 CIDR, got: $SSH_CIDR"
fi

key_dir="$(mktemp -d "${TMPDIR:-/tmp}/sam-microgc-test-key.XXXXXX")"
trap 'rm -rf "$key_dir"' EXIT
private_key_file="$key_dir/stored-id_ed25519"
public_key_file="$key_dir/id_ed25519.pub"
stored_public_key_file="$key_dir/stored-id_ed25519.pub"

infisical secrets get "$PRIVATE_KEY_SECRET" "${infisical_args[@]}" --plain >"$private_key_file"
infisical secrets get "$PUBLIC_KEY_SECRET" "${infisical_args[@]}" --plain >"$stored_public_key_file"
private_exists=false
public_exists=false
[[ -s "$private_key_file" ]] && private_exists=true
[[ -s "$stored_public_key_file" ]] && public_exists=true

if [[ "$private_exists" == true ]]; then
  chmod 600 "$private_key_file"
  ssh-keygen -y -f "$private_key_file" >"$public_key_file"
  if [[ "$public_exists" == true ]]; then
    stored_public_key_identity="$(awk 'NR == 1 { print $1 " " $2 }' "$stored_public_key_file")"
    derived_public_key_identity="$(awk 'NR == 1 { print $1 " " $2 }' "$public_key_file")"
    [[ "$stored_public_key_identity" == "$derived_public_key_identity" ]] || die "the Infisical public key does not match the stored private key"
    cp "$stored_public_key_file" "$public_key_file"
  else
    infisical secrets set "$PUBLIC_KEY_SECRET=@$public_key_file" "${infisical_args[@]}" >/dev/null
  fi
elif [[ "$public_exists" == true ]]; then
  die "$PUBLIC_KEY_SECRET exists in Infisical but $PRIVATE_KEY_SECRET does not; refusing to deploy a host that cannot be accessed"
else
  generated_private_key_file="$key_dir/id_ed25519"
  ssh-keygen -q -t ed25519 -N '' -C "$STACK_NAME" -f "$generated_private_key_file"
  private_key_file="$generated_private_key_file"
  infisical secrets set "$PRIVATE_KEY_SECRET=@$private_key_file" "$PUBLIC_KEY_SECRET=@$public_key_file" "${infisical_args[@]}" >/dev/null
fi

public_key_material="$(cat "$public_key_file")"

printf 'Deploying %s in %s with SSH restricted to %s...\n' "$STACK_NAME" "$AWS_REGION" "$SSH_CIDR"
aws cloudformation deploy \
  --stack-name "$STACK_NAME" \
  --template-file "$SCRIPT_DIR/cloudformation.yaml" \
  --parameter-overrides \
    "Prefix=$STACK_NAME" \
    "PublicKeyMaterial=$public_key_material" \
    "SSHCidr=$SSH_CIDR" \
  --tags \
    Owner=sam \
    Project=baml \
    Purpose=microgc-benchmark \
  --no-fail-on-empty-changes

"$SCRIPT_DIR/status.sh"
"$SCRIPT_DIR/verify.sh"
