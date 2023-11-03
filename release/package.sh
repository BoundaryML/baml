#!/bin/sh

set -e
set -x
# cd into target/release from the current dir
cargo build --release
cd ./target/release
tar -czvf baml.tar.gz baml
echo $(shasum -a 256 "baml.tar.gz")