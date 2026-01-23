# Architecture diagram

To regenerate:

```bash
./architecture/generate.sh
```

Or directly:

```bash
cargo run -p tools_stow -- stow --graph architecture/architecture.svg
```

Options:
- `--include-tests` - Include test crates (`*_test`, `*_tests`) in the graph

The graph shows:
- Local workspace crates grouped into namespace clusters (`baml`, `bex`)
- Crates colored by namespace (border) and tag (fill)
- External dependencies from `stow.toml` `graph_external_deps` placed inside each namespace that uses them
- Dashed edges with direction arrows for cross-namespace dependencies
- Transitive reduction applied (only direct dependencies shown)
