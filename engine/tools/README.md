# BAML Engine Tools

This crate contains various utility tools and binaries for the BAML engine. Each tool is implemented as a separate binary target within this single crate.

## Binary Targets

### `language-server-hot-reload`

A hot-reload utility for the BAML CLI that watches for binary changes and automatically restarts the process. This tool is particularly useful during development when you want to automatically restart the language server or other BAML CLI commands when the binary is rebuilt.

**Features:**
- Watches for changes to the target binary
- Automatically restarts the process when changes are detected
- Preserves and replays stdin input to the restarted process
- Configurable debouncing to avoid excessive restarts

**Usage:**
```bash
cargo run --bin language-server-hot-reload -- [BAML_CLI_ARGS...]
```

## Adding New Tools

To add a new tool to this crate:

1. Create a new file in `src/bin/` with your tool's name
2. Add a corresponding `[[bin]]` entry in `Cargo.toml`
3. Implement your tool's functionality
4. Update this README to document the new tool

## Structure

```
tools/
├── Cargo.toml          # Crate configuration with binary targets
├── README.md           # This file
└── src/
    ├── lib.rs          # Common utilities (if needed)
    └── bin/
        └── language-server-hot-reload.rs  # Hot-reload binary
```