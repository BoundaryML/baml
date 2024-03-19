#!/bin/bash
set -e

../engine/target/debug/baml test run --generator lang_typescript
../engine/target/debug/baml test run --generator lang_python