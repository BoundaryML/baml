#!/bin/sh

# Exit on error
set -e
# Echo each command
set -x

baml init -n

# Check that poetry installed baml
poetry run python -m baml_version

# Run the command and write stdout and stderr to different files
baml test run > $CAPTURE_DIR/baml_test_stdout.log 2> $CAPTURE_DIR/baml_test_stderr.log

poetry run python -m baml_example_app > $CAPTURE_DIR/baml_example_stdout.log 2> $CAPTURE_DIR/baml_example_stderr.log

check_for_updates="$(baml version --check --output json)"
[[ $(echo "$check_for_updates" | jq '.generators.[].current_version') =~ '[0-9].*' ]] || echo "Failed to resolve current client version"
