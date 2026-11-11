#!/usr/bin/env bash
# Writes the markers that the Kiln jobs read. The jobs read the basis to set BAML_GIT_SHA, and
# print the variant to prove which snapshot they booted.
set -euo pipefail
# The template env sets MISE_DATA_DIR.
: "${MISE_DATA_DIR:?the template env must set MISE_DATA_DIR}"

SRC=/opt/actions-runner/_work/baml/baml

# Run the git stat refresh once here, not in the first checkout of each job.
git -C "${SRC}" update-index --refresh >/dev/null || true

git -C "${SRC}" rev-parse HEAD >/cache/trunk-template-basis-sha
echo "${SRC}" >/cache/trunk-template-built-at
# Change the variant on every edit of the template or its scripts.
echo "trunk-warm-template-v10" >/cache/trunk-template-variant

df -h /
du -sh "${SRC}/baml_language/target" "${MISE_DATA_DIR}"
echo "sdk legs that failed:"
cat /cache/trunk-template-skipped
