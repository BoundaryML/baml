# setup-baml

`BoundaryML/setup-baml` is a thin composite GitHub Action that installs the official BAML wrapper, selects a BAML toolchain, and adds `baml` to `PATH`. It does not run project checks, generation, or tests; keep those operations in later workflow steps.

## Usage

```yaml
steps:
  - uses: actions/checkout@v7

  - uses: BoundaryML/setup-baml@v1
    with:
      toolchain: nightly

  - run: baml check
  - run: baml generate
  - run: baml test
```

`toolchain` accepts an exact version, `canary`, or `nightly`:

```yaml
- uses: BoundaryML/setup-baml@v1
  with:
    toolchain: 0.16.0
```

An explicit `toolchain` input overrides a conflicting project selector for subsequent workflow steps through the wrapper's documented `BAML_VERSION` mechanism.

When `toolchain` is omitted, the official BAML wrapper resolves the project selector from the nearest `baml.toml`:

```toml
[toolchain]
channel = "nightly"
```

```yaml
- uses: BoundaryML/setup-baml@v1
```

If no project selector exists, the wrapper's configured default is used; an isolated installation starts with `canary`.

## Outputs

| Output | Description |
| --- | --- |
| `version` | Resolved concrete BAML toolchain version |
| `path` | Absolute path to the `baml` wrapper added to `PATH` |
| `toolchain-path` | Absolute path to the resolved `baml-cli` binary |

```yaml
- uses: BoundaryML/setup-baml@v1
  id: baml
  with:
    toolchain: nightly
- run: echo "BAML ${{ steps.baml.outputs.version }}"
```

## Pinning

Use the moving major tag for automatic compatible updates:

```yaml
- uses: BoundaryML/setup-baml@v1
```

For stronger supply-chain reproducibility, pin the action to a full commit SHA and let Dependabot or Renovate propose updates:

```yaml
- uses: BoundaryML/setup-baml@0123456789abcdef0123456789abcdef01234567 # replace with a reviewed commit
```

The action downloads the official installer over HTTPS from the same `pkg.boundaryml.com` endpoints documented in the [BAML quickstart](https://boundaryml.com/quickstart). The installer downloads the official wrapper through the BAML release manifest, verifies its SHA-256 checksum, rejects unsafe archive layouts, and the wrapper applies the same manifest, checksum, and archive-safety checks to toolchains. The action clears manifest-source overrides, sets or clears the toolchain override according to whether the input was explicit, validates selectors before passing them as process arguments, and does not evaluate action input as shell code.

The action installs under the runner tool cache but does not use `actions/cache`: hosted runners are ephemeral, channel manifests move, and caching the small wrapper state would add invalidation complexity without a reliable installation-time benefit.

## Wrapper compatibility

The current wrapper has no machine-readable command that both resolves the active project selector and installs it. Until [BoundaryML/baml#4417](https://github.com/BoundaryML/baml/issues/4417) is available, omitted-input setup asks `baml toolchain list` to perform the canonical `baml.toml` lookup and parses its human-readable active-selector line; resolved version reporting similarly parses `baml --version`. This repository does not duplicate the wrapper's project traversal, manifest, checksum, or extraction logic.

## Contributing

This repository is a read-only mirror. Open changes against [`packaging/github-actions/setup-baml`](https://github.com/BoundaryML/baml/tree/canary/packaging/github-actions/setup-baml) in `BoundaryML/baml`.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
