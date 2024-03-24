#!/bin/bash

# Exit on error
set -e
# Echo each command
set -x

RUST_BACKTRACE=1 baml init -n

cat baml_src/main.baml > $CAPTURE_DIR/baml_init_stdout.log

# Run the command and write stdout and stderr to different files
baml test run > $CAPTURE_DIR/baml_test_stdout.log 2> $CAPTURE_DIR/baml_test_stderr.log

baml version --check --output json >$CAPTURE_DIR/baml_version_check.json

echo "Checking the output file"
# Load the JSON file content
checked_versions=$(cat "${CAPTURE_DIR}/baml_version_check.json")

echo "Raw output from baml version check:"
echo "${checked_versions}"

# Use jq to extract 'current_version' from each item in 'generators' and check if they all start with a digit
valid_versions=$(echo "${checked_versions}" | jq '[.generators[].current_version | test("^[0-9]")] | all')

# Assert equivalent in bash; exit with an error message if not all versions start with a digit
if [[ ${valid_versions} == "false" ]]; then
    echo "baml cli failed to parse package_version_command"
    exit 1
fi

