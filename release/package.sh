#!/bin/sh

set -e
set -x
# cd into target/release from the current dir
cargo build --release
cd ./target/release
tar -czvf gloo.tar.gz gloo
echo $(shasum -a 256 "gloo.tar.gz")