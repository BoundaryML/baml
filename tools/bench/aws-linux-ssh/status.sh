#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

require_command aws
check_aws_account

instance_ids=("$(stack_output X64InstanceId)" "$(stack_output GravitonInstanceId)")
# The backticks below are JMESPath literals, not shell substitutions.
# shellcheck disable=SC2016
aws ec2 describe-instances \
  --instance-ids "${instance_ids[@]}" \
  --query 'Reservations[].Instances[].{Name:Tags[?Key==`Name`].Value|[0],InstanceId:InstanceId,Type:InstanceType,Architecture:Architecture,State:State.Name,PublicIp:PublicIpAddress}' \
  --output table

printf 'SSH CIDR: %s\n' "$(stack_output SSHCidr)"
printf 'Connect:  %s/ssh.sh x64\n' "$SCRIPT_DIR"
printf 'Connect:  %s/ssh.sh graviton\n' "$SCRIPT_DIR"
