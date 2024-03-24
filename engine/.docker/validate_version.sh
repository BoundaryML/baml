#!/bin/bash

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
