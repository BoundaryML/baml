# Architecture diagram

The workspace dependency graph is generated on demand by `cargo stow` as
[Mermaid](https://mermaid.js.org) flowchart text. It is written to `architecture.mmd`,
which is ignored by git rather than committed.

To regenerate:

```bash
./architecture/generate.sh
```

Or directly:

```bash
cargo run -p cargo-stow -- stow --graph architecture/architecture.mmd
```

Options:
- `--include-tests` - Include test crates (`*_test`, `*_tests`) in the graph

To view it, paste the file into <https://mermaid.live> or any Mermaid renderer.

The graph shows:
- Workspace crates grouped into one cluster per namespace
- Crates outlined in their namespace colour and filled by prefix tag
- External dependencies listed under `[graph] external_deps` in `stow.toml`, placed inside
  each namespace that uses them, one node per distinct set of added features
- Dashed edges for cross-namespace dependencies
- Dashed proxy nodes for `link_crates` and `embed_crates`
- Only direct dependencies: edges another dependency already implies are removed

The rules are specified in `crates/tools_stow/SPEC.md`, section 9.
