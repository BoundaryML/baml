#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
exec docker --context "${LOCAL_DOCKER_CONTEXT:-colima-baml-hello-world}" compose "$@"
