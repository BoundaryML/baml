# Contributing to BAML

Thanks for your interest in contributing to BAML! We appreciate all the help we
can get in making it the best way to build any AI agents or applications.

Please join our [Discord](https://discord.gg/BTNBeXGuaS) and introduce yourself
in the `#contributing` channel. Let us know what you're interested in working
on, and we can help you get started.

# AI Usage

We welcome and encourage contributors to use AI to support BAML development.
However, be warned that due to excessive spam on our repo, robot-generated
**drive-by pull requests will be rejected**.

Please remember that BAML is maintained by humans and is used in production
for a wide range of critical use cases. We expect our contributors to be
thoughtful about their work and engage with us, either in our Discord or on
the issue or pull request.

# Getting Started

See [README-DEV.md](./README-DEV.md) for environment setup (via
[mise](https://mise.jdx.dev/) and `./scripts/setup-dev.sh`), the development
workflow, and how to run tests. In short:

```bash
curl https://mise.run | sh   # install mise
./scripts/setup-dev.sh       # install Rust and everything else
```

Rust changes go in the `baml_language/` workspace — run `cargo test --lib`
there before submitting, and format with:

```bash
cargo fmt -- --config imports_granularity="Crate" --config group_imports="StdExternalCrate"
```
