# Contributing to BAML

First off, thanks for your interest in contributing to BAML! We appreciate all the help we can get in making it the best way to build any AI agents or applications.

> **📚 For comprehensive development setup instructions, see our [Development Setup Guide](./README-DEV.md)**

## Table of Contents

- [Contributing to BAML](#contributing-to-baml)
  - [Table of Contents](#table-of-contents)
  - [How to Contribute](#how-to-contribute)
    - [Examples of Merged PRs:](#examples-of-merged-prs)
  - [Quick Start - Development Setup](#quick-start---development-setup)
  - [Setting up the BAML Compiler and Runtime](#setting-up-the-baml-compiler-and-runtime)
    - [Compiler Architecture Overview](#compiler-architecture-overview)
    - [Steps to Build and Test Locally](#steps-to-build-and-test-locally)
  - [Running Integration Tests](#running-integration-tests)
    - [Prerequisites for All Tests](#prerequisites-for-all-tests)
      - [Environment Variables](#environment-variables)
    - [TypeScript Integration Tests](#typescript-integration-tests)
    - [Python Integration Tests](#python-integration-tests)
    - [Ruby Integration Tests](#ruby-integration-tests)
    - [Adding New Tests](#adding-new-tests)
    - [Debugging Tests](#debugging-tests)
    - [OpenAPI Server Tests](#openapi-server-tests)
  - [Grammar Testing](#grammar-testing)
  - [VSCode Extension Testing](#vscode-extension-testing)
  - [Testing promptfiddle.com](#testing-promptfiddlecom)

## How to Contribute

1. **Join our Community**:

- Please join our [Discord](https://discord.gg/BTNBeXGuaS) and introduce yourself in the `#contributing` channel. Let us know what you're interested in working on, and we can help you get started.

2. **Check Existing Issues**:

- Look at the [issue tracker](https://github.com/BoundaryML/baml/issues) and find and issue to work on.
  Issues labeled `good first issue` are a good place to start.

3. **Creating an Issue**:

- If you find a bug or have a feature request, please tell us about in the discord channel and then open a new issue. Make sure to provide enough details and include a clear title.

4. **Fork the Repository**:

- Fork the repository and clone your fork locally. Work on your changes in a feature branch.

5. **Submit a Pull Request (PR)**:

- Submit your pull request with a clear description of the changes you've made. Make sure to reference the issue you're working on.

### Examples of Merged PRs:

- **Fix parsing issues**: [PR #1031](https://github.com/BoundaryML/baml/pull/1031)

- **Coerce integers properly**: [PR #1023](https://github.com/BoundaryML/baml/pull/1023)

- **Fix syntax highlighting and a grammar parser crash**: [PR #1013](https://github.com/BoundaryML/baml/pull/1013)

- **Implement literal types (e.g., `sports "SOCCER" | "BASKETBALL"`)**: [PR #978](https://github.com/BoundaryML/baml/pull/978)

- **Fix issue with OpenAI provider**: [PR #896](https://github.com/BoundaryML/baml/pull/896)

- **Implement `map` type**: [PR #797](https://github.com/BoundaryML/baml/pull/797)

## Quick Start - Development Setup

We use [mise](https://mise.jdx.dev/) to manage development tools and ensure everyone has the correct versions.

1. **Run the setup script**:
   ```bash
   ./scripts/setup-dev.sh
   ```

   This will:
   - Install mise (if not already installed)
   - Install all required tools with correct versions (Rust 1.85.0, Go 1.23, Python 3.12, Ruby 3.2.2, Node.js LTS)
   - Install language-specific tools (cargo-watch, wasm-pack, protoc-gen-go, etc.)
   - Set up Python and Ruby dependencies

2. **Verify installation**:
   ```bash
   mise list
   ```

3. **Update tools** (when `mise.toml` changes):
   ```bash
   mise install
   ```

The setup script automatically handles all dependencies and version management, ensuring a consistent development environment across all contributors.

## Setting up the BAML Compiler and Runtime

#### Compiler Architecture Overview

<TBD — we will write more details here>

- `baml-cli/ VSCode` generates `baml_client`, containing all the interfaces people use to call the `baml-runtime`.

- **Pest grammar -> AST (build diagnostics for linter) -> Intermediate Representation (IR)**: The runtime parses BAML files, builds and calls LLM endpoints, parses data into JSONish, and coerces that JSONish into the schema.

### Steps to Build and Test Locally

1. Run the setup script if you haven't already:
   ```bash
   ./scripts/setup-dev.sh
   ```

2. Run `cargo build` in `engine/` and make sure everything builds on your machine.

3. Run some unit tests:

   - `cd engine/baml-lib/baml/` and run `cargo test` to execute grammar linting tests.

4. Run the integration tests.

5. **Set up Git hooks (Recommended)**:
   - Install the pre-commit hook to automatically format Rust code:
     ```bash
     ./tools/install-hooks
     ```
   - This hook will run `cargo fmt` with import organization before each commit
   - If formatting changes are made, you'll need to review and re-commit the changes



**Prerequisites:**
- Node.js 20.x or later
- Rust (stable toolchain)
- Go 1.23 or later
- Python 3.12
- Git

The script automatically handles installing missing tools and dependencies, so you can run it on a fresh machine.

## Running Integration Tests

The integration tests verify BAML's functionality across multiple programming languages. Each language has its own test suite in the `integ-tests/` directory.

### Prerequisites for All Tests

- Rust toolchain (managed by mise)
- BAML CLI (built from source or installed)

#### Environment Variables

You can set up environment variables in two ways:

1. **Using .env file (Recommended for external contributors)**:

   - Create a `.env` file in the `integ-tests` directory
   - Required variables:
     ```bash
     OPENAI_API_KEY=your_key_here
     # Add other provider keys as needed:
     # ANTHROPIC_API_KEY=your_key_here
     # AWS_ACCESS_KEY_ID=your_key_here
     # etc.
     ```

2. **Using Infisical (BAML internal use only)**:
   - Install [Infisical CLI](https://infisical.com/docs/cli/overview)
   - Use the `infisical run` commands shown in examples below
   - External contributors should replace `infisical run --env=test --` with `dotenv -e ../.env --` in all commands

### TypeScript Integration Tests

1. Install prerequisites:

   - Node.js (Latest LTS, managed by mise)
   - pnpm package manager (managed by mise)

2. Build the TypeScript runtime:

```bash
cd engine/language_client_typescript
pnpm build:debug
```

3. Set up and run tests:

```bash
cd integ-tests/typescript
pnpm install
pnpm generate
dotenv -e ../.env -- pnpm integ-tests  # or use infisical for internal BAML devs
```

### Python Integration Tests

1. Install prerequisites:

   - Python 3.8 or higher (3.12 recommended, managed by mise)
   - uv package manager (installed via mise)

2. Set up the environment:

```bash
cd integ-tests/python
uv sync
```

3. Build and install the Python client:

```bash
# Note: env -u CONDA_PREFIX is needed if using Conda
uv run maturin develop --uv --manifest-path ../../engine/language_client_python/Cargo.toml
```

4. Generate client code and run tests:

```bash
uv run baml-cli generate --from ../baml_src
dotenv -e ../.env -- uv run pytest  # or use infisical for internal BAML devs
```

### Ruby Integration Tests

1. Prerequisites are handled by the setup script (Ruby 3.2.2 via mise)

2. Build the Ruby client:

```bash
cd integ-tests/ruby
(cd ../../engine/language_client_ruby && rake compile)
```

3. Install dependencies and generate client:

```bash
bundle install
baml-cli generate --from ../baml_src
```

4. Run tests:

```bash
dotenv -e ../.env -- rake test  # or use infisical for internal BAML devs
```

### Adding New Tests

1. Define your BAML files in `integ-tests/baml_src/`:

   - Add clients in `clients.baml`
   - Add functions and tests in `test-files/providers/`
   - See [BAML Source README](integ-tests/baml_src/README.md) for details

2. Generate client code for each language:

```bash
# TypeScript
cd integ-tests/typescript && pnpm generate

# Python
cd integ-tests/python && uv run baml-cli generate --from ../baml_src

# Ruby
cd integ-tests/ruby && mise exec -- baml-cli generate --from ../baml_src
```

3. Create language-specific test files:

   - Follow the patterns in existing test files
   - Use language-appropriate testing frameworks (Jest, pytest, Minitest)
   - Include both success and error cases
   - Test edge cases and timeouts

4. Run the tests in each language to ensure cross-language compatibility

### Debugging Tests

Each language has its own debugging setup in VS Code:

1. **TypeScript**:

   - Install Jest Runner extension
   - Use launch configuration from TypeScript README
   - Set `BAML_LOG=trace` for detailed logs

2. **Python**:

   - Install Python Test Explorer
   - Use launch configuration from Python README
   - Use `-s` flag to show print statements

3. **Ruby**:
   - Install Ruby Test Explorer
   - Use launch configuration from Ruby README
   - Use verbose mode for detailed output

### OpenAPI Server Tests

1. Navigate to the test directory:

   - `cd engine/baml-runtime/tests/`

2. Run tests with:

- `cargo test --features internal`

This will run the baml-serve server locally, and ping it. You may need to change the PORT variable for your new test to use a different port (we don't have a good way of autoselecting a port).

> Instructions for testing a particular OpenAPI client are TBD.

## Grammar Testing

1. Test new syntax in the [pest playground](https://pest.rs/).

2. Update the following:

   - **Pest grammar**: Modify the `.pest` file.
   - **AST parsing**: Update the AST parsing of the new grammar.
   - **IR**: Modify the Intermediate Representation (IR).

3. Ensure all tests pass:

   - Run `cargo test` in `engine/baml-lib/`
   - Ensure integration tests still pass.

4. Modify the grammar for the [PromptFiddle.com](http://PromptFiddle.com) syntax rendering that uses Lezer, if necessary.

## VSCode Extension Testing

This requires a macos or linux machine, since we symlink some playground files between both [PromptFiddle.com](http://PromptFiddle.com) website app, and the VSCode extension itself.

**Note:** If you are just making changes to the VSCode extension UI, you may want to go to the section: [Testing PromptFiddle.com](#testing-prompfiddlecom).

1. Navigate to the TypeScript directory:

   - `cd typescript/`

2. Install dependencies:

   - `pnpm i`

3. Build and launch the extension:
   - `pnpm build:vscode`
   - Open VSCode and go to the Run and Debug section (play button near the extensions button).
   - Select "Launch VSCode Extension" and press the play button.
     - This will open a new VSCode window in Debug mode.
     - You can open a simple BAML project in this window (refer to our quickstart guide to set up a simple project, or clone the `baml-examples` repository).
4. Generate the language server binary (in case our scripts don't do this for you)

   - `cd typescript/apps/vscode-ext`
   - `pnpm server:build`

5. Reload the extension:
   - Use `Command + Shift + P` to reload the extension when you change any core logic.
   - Alternatively, close and reopen the playground if you rebuild the playground.

To rebuild the playground UI:

1. Navigate to the shared playground components: `cd typescript/packages/playground-common`
2. Make your changes
3. Build: `pnpm build`
4. Close and open the playground in your "Debug mode VSCode window"

## Testing [promptfiddle.com](http://promptfiddle.com)

This is useful if you want to iterate faster on the Extension UI, since it supports hot-reloading.

1. Navigate to the Fiddle Web App directory:

   - `cd typescript/apps/fiddle-web-app`

2. Start the dev server:

   - `pnpm dev`

3. The app will hot-reload when you modify files in:
   - `typescript/packages/playground-common` (shared playground components)
   - `typescript/packages/ui` (shared UI components)
   - `typescript/apps/fiddle-web-app` (app-specific code)
