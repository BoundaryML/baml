#!/usr/bin/env bash

set -euo pipefail

STACK_NAME="${STACK_NAME:-sam-microgc-test}"
AWS_PROFILE="${AWS_PROFILE:-boundaryml-dev}"
AWS_REGION="${AWS_REGION:-us-east-1}"
EXPECTED_AWS_ACCOUNT="${EXPECTED_AWS_ACCOUNT:-147997132427}"
INFISICAL_PROJECT_ID="${INFISICAL_PROJECT_ID:-bdd280e2-259c-4750-9b16-a8597a67214c}"
INFISICAL_ENV="${INFISICAL_ENV:-dev-humans}"
PRIVATE_KEY_SECRET="${PRIVATE_KEY_SECRET:-SAM_MICROGC_TEST_SSH_PRIVATE_KEY}"
PUBLIC_KEY_SECRET="${PUBLIC_KEY_SECRET:-SAM_MICROGC_TEST_SSH_PUBLIC_KEY}"

export AWS_PROFILE AWS_REGION

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

check_aws_account() {
  local actual_account
  actual_account="$(aws sts get-caller-identity --query Account --output text)"
  [[ "$actual_account" == "$EXPECTED_AWS_ACCOUNT" ]] || die "AWS profile $AWS_PROFILE resolved to account $actual_account, expected $EXPECTED_AWS_ACCOUNT"
}

stack_output() {
  local key="$1"
  aws cloudformation describe-stacks --stack-name "$STACK_NAME" --query "Stacks[0].Outputs[?OutputKey=='$key'].OutputValue | [0]" --output text
}

# This array is consumed by scripts that source this file.
# shellcheck disable=SC2034
infisical_args=(--projectId "$INFISICAL_PROJECT_ID" --env "$INFISICAL_ENV" --silent)
