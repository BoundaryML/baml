#!/bin/bash

# Exit on error
set -e
# Echo each command
set -x

# Initialize pipenv environment
pipenv --python 3.8

# baml init -n requires a python file to auto-detect its a python project
touch main.py

# Initialize the project with baml
pipenv run baml init -n

# Check that pipenv environment has baml
pipenv run python -m baml_version

# Ensure that the global environment does not have baml
python -m baml_version || echo "Global environment should not have baml"

# Run the command and write stdout and stderr to different files
pipenv run baml test run > $CAPTURE_DIR/baml_test_stdout.log 2> $CAPTURE_DIR/baml_test_stderr.log

pipenv run python -m baml_example_app > $CAPTURE_DIR/baml_example_stdout.log 2> $CAPTURE_DIR/baml_example_stderr.log

# TODO - add test for 'baml version --check' here