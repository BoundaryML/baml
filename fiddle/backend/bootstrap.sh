#!/usr/bin/env /bin/bash

set -euxo pipefail

echo "Running bootstrap"

export TERM=xterm
curl -fsSL https://raw.githubusercontent.com/BoundaryML/homebrew-baml/main/install-baml.sh | \
  sed "s/sudo//" | \
  bash

pip3 install baml flask flask-cors
pip3 install poetry
pip3 install pytest
# poetry install
# pip3 install @boundaryml/baml-core

# apt install sudo