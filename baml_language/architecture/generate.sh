#!/bin/bash

PROJECT_ROOT=$(git rev-parse --show-toplevel)

stacktower parse rust ${PROJECT_ROOT}/baml_language/Cargo.toml  -o ${PROJECT_ROOT}/baml_language/architecture/dependency-graph.json --enrich=false
stacktower render ${PROJECT_ROOT}/baml_language/architecture/dependency-graph.json -o ${PROJECT_ROOT}/baml_language/architecture/architecture.svg --only-local --include serde,tokio,anyhow,rowan,salsa,thiserror -t nodelink --randomize=false --popups=false
