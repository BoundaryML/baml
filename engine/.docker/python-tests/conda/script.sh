#!/bin/bash

conda init
. /root/.bashrc > /dev/null

# Exit on error
set -e
# Echo each command
set -x

conda activate envbaml

baml init -n

# Go back to the main environment
conda activate base

# Check that conda installed baml
conda run -n envbaml python -m baml_version

# Ensure that base does not have baml
conda run -n base python -m baml_version || echo "Base environment should not have baml"

# Run the command and write stdout and stderr to different files
baml test run > $CAPTURE_DIR/baml_test_stdout.log 2> $CAPTURE_DIR/baml_test_stderr.log

conda run -n envbaml python -m baml_example_app > $CAPTURE_DIR/baml_example_stdout.log 2> $CAPTURE_DIR/baml_example_stderr.log

# TODO - add test for 'baml version --check' here