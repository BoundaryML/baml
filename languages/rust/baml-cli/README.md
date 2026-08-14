# BAML v0 — Rust CLI

BAML v0 CLI for Rust — a command-line interface for the BAML v0 runtime.

## Installation

### From crates.io (when published)

```bash
cargo install baml-cli
```

### From source

```bash
git clone https://github.com/BoundaryML/baml.git
cd baml/languages/rust
./setup-ffi.sh
cargo install --path baml-cli
```

### From local development

```bash
cd languages/rust
./setup-ffi.sh
cargo build --release -p baml-cli
```

The binary will be available at `target/release/baml-cli`.

## Usage

```bash
baml-cli [args...]
```

For more information, see the [BAML documentation](https://docs.boundaryml.com).

## License

MIT License - see the [LICENSE](../../LICENSE) file for details.
