#!/usr/bin/env bash

set -euxo pipefail

sudo apt-get update
sudo apt-get install -y \
  gcc-arm-linux-gnueabihf \
  g++-arm-linux-gnueabihf \
  gcc-aarch64-linux-gnu \
  g++-aarch64-linux-gnu
