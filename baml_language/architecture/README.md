# Architecture diagram

These are generated via `stacktower`. To regenerate, run:

```bash
# Generate dependency graph
stacktower parse rust Cargo.toml  -o architecture/dependency-graph.json --enrich=false

# Render
stacktower render architecture/dependency-graph.json -o architecture/architecture.svg --only-local --randomize=false --popups=false --width 2400 --height 1600 --include serde,tokio,anyhow,rowan,salsa,thiserror -t nodelink
```
