# @prisma/prisma-schema-wasm

[![Publish pipeline](https://github.com/prisma/prisma-engines/actions/workflows/publish-prisma-schema-wasm.yml/badge.svg)](https://github.com/prisma/prisma-engines/actions/workflows/publish-prisma-schema-wasm.yml)
[![npm package](https://img.shields.io/npm/v/@prisma/prisma-schema-wasm/latest)](https://www.npmjs.com/package/@prisma/prisma-schema-wasm)
[![install size](https://packagephobia.com/badge?p=@prisma/prisma-schema-wasm)](https://packagephobia.com/result?p=@prisma/prisma-schema-wasm)

This directory only contains build logic to package the `prisma-fmt` engine
into a Node package as a WASM module. All the functionality is implemented in
other parts of prisma-engines.

The published NPM package is internal to Prisma. Its API will break without prior warning.

## Example

```bash
node -e "const prismaSchema = require('@prisma/prisma-schema-wasm'); console.log(prismaSchema.version())"
```

## Components

- The GitHub Actions workflow that publishes the NPM package: https://github.com/prisma/prisma-engines/blob/main/.github/workflows/publish-prisma-schema-wasm.yml
    - It is triggered from the https://github.com/prisma/engines-wrapper publish action.
- The [Rust source code](https://github.com/prisma/prisma-engines/tree/main/prisma-schema-wasm/src) for the wasm module
- The [nix build definition](https://github.com/prisma/prisma-engines/blob/main/prisma-schema-wasm/default.nix)
    - It gives us a fully reproducible, thoroughly described build process and environment. The alternative would be a bash script with installs through `rustup`, `cargo install` and `apt`, with underspecified system dependencies and best-effort version pinning.
    - You can read more about nix on [nix.dev](https://nix.dev/) and the [official website](https://nixos.org/).

## Local Dev with Language-Tools
When implementing features for `language-tools` in `prisma-engines`, to sync with your local dev environment for the `language-server`, one can do the following:

### On first setup
```
# Install the latest Rust version with `rustup`
# or update the latest Rust version with `rustup`
rustup update
rustup target add wasm32-unknown-unknown
cargo update -p wasm-bindgen
# Check the version defined in `prisma-schema-wasm/cargo.toml` for `wasm-bindgen` and replace `version` below:
cargo install -f wasm-bindgen-cli@version
```

### On Changes

```bash
./prisma-schema-wasm/scripts/update-schema-wasm.sh
```

This script has the following expectations:
- `language-tools` is in the same dir as `prisma-engines`
  - i.e. `dir/{prisma-engines,language-tools}`
- it's run in the `prisma-engines` root folder