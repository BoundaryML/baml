#!/bin/bash

set -e

cd "$(dirname "$0")/.."

cargo run -p cargo-stow -- stow --graph architecture/architecture.svg
