#!/bin/bash

set -e

cd "$(dirname "$0")/.."

cargo run -p tools_stow -- stow --graph architecture/architecture.svg
