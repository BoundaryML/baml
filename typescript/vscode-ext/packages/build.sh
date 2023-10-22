#!/bin/bash

# Exit on any error
set -euo pipefail

# cd into client and build it
cd vscode
echo "Building vscode..."
pnpm run build | tee
cd ..
cd language-server
echo "Building language-server..."
pnpm run build | tee
cd ..
