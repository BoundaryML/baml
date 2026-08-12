# BAML

BAML language support for Visual Studio Code and Cursor, including syntax
highlighting, hover information, completions, diagnostics, go-to-definition,
and the BAML playground.

## Installation

This extension is distributed with the BAML toolchain and installed from the
command line using `baml ide install`. Its version is matched to the active
BAML toolchain.

## Updating

If your machine's active toolchain tracks the `canary` or `nightly` channel,
update the toolchain and reinstall its matching extension:

```bash
# Visual Studio Code
baml toolchain update && baml ide install --code

# Cursor
baml toolchain update && baml ide install --cursor
```

An exact-version toolchain does not advance automatically. Select or install
the newer toolchain first, then run the appropriate `baml ide install` command
again.

## Documentation

See the [BAML quickstart](https://boundaryml.com/quickstart).
