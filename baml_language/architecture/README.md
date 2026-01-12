# Architecture diagram

These are generated via `stacktower`. To regenerate, run:

(Note, this is from a local build of stacktower with some fixes to address cargo workspaces.)

```bash
# Generate dependency graph
stacktower parse rust Cargo.toml  -o architecture/dependency-graph.json --enrich=false

# Render
stacktower render architecture/dependency-graph.json -o architecture/architecture.svg --only-local --randomize=false --popups=false --include serde,tokio,anyhow,rowan,salsa,thiserror -t nodelink
```
