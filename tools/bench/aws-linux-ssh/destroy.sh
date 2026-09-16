#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

require_command aws
check_aws_account

printf 'Deleting CloudFormation stack %s in %s...\n' "$STACK_NAME" "$AWS_REGION"
aws cloudformation delete-stack --stack-name "$STACK_NAME"
aws cloudformation wait stack-delete-complete --stack-name "$STACK_NAME"
printf 'Deleted AWS resources. Infisical secrets %s and %s were retained.\n' "$PRIVATE_KEY_SECRET" "$PUBLIC_KEY_SECRET"
