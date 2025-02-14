# Contributing to BAML

First off, thanks for your interest in contributing to BAML! We appreciate all the help we can get in making it the best way to build any AI agents or applications.

## Table of Contents

- [How to Contribute](#how-to-contribute)
   - [Join our Community](#join-our-community)
   - [Check Existing Issues](#check-existing-issues)
   - [Creating an Issue](#creating-an-issue)
   - [Fork the Repository](#fork-the-repository)
   - [Submit a Pull Request (PR)](#submit-a-pull-request-pr)
   - [Examples of Merged PRs](#examples-of-merged-prs)
- [Setting up the BAML Compiler and Runtime](#setting-up-the-baml-compiler-and-runtime)
   - [Compiler Architecture Overview](#compiler-architecture-overview)
   - [Steps to Build and Test Locally](#steps-to-build-and-test-locally)
- [Running Integration Tests](#running-integration-tests)
   - [Python Integration Tests](#python-integration-tests)
   - [TypeScript Integration Tests](#typescript-integration-tests)
   - [OpenAPI Server Tests](#openapi-server-tests)
- [Grammar Testing](#grammar-testing)
- [VSCode Extension Testing](#vscode-extension-testing)
- [Testing PromptFiddle.com](#testing-prompfiddlecom)


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



## Setting up the BAML Compiler and Runtime

#### Compiler Architecture Overview

<TBD — we will write more details here>

- `baml-cli/ VSCode` generates `baml_client`, containing all the interfaces people use to call the `baml-runtime`.

- **Pest grammar -> AST (build diagnostics for linter) -> Intermediate Representation (IR)**: The runtime parses BAML files, builds and calls LLM endpoints, parses data into JSONish, and coerces that JSONish into the schema.


### Steps to Build and Test Locally

1. Install Rust

2. Run `cargo build` in `engine/` and make sure everything builds on your machine.

3. Run some unit tests:
   - `cd engine/baml-lib/baml/` and run `cargo test` to execute grammar linting tests.

4. Run the integration tests.

## Running Integration Tests

The integration tests verify BAML's functionality across multiple programming languages. Each language has its own test suite in the `integ-tests/` directory.

### Prerequisites for All Tests

- [Rust toolchain](https://rustup.rs/) (for building native clients)
- [BAML CLI](https://github.com/boundaryml/baml#installation)

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
   - [Node.js](https://nodejs.org/) (Latest LTS recommended)
   - [pnpm](https://pnpm.io/installation) package manager

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
   - [Python](https://www.python.org/downloads/) 3.8 or higher
   - [uv](https://astral.sh/uv) package manager

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

1. Install prerequisites:
   - [mise](https://mise.jdx.dev/getting-started.html) for Ruby version management:
     ```bash
     brew install mise  # on macOS
     # or
     curl https://mise.run | sh  # other platforms
     ```
   - Rust toolchain (installed above)

2. Set up mise and build the Ruby client:
```bash
cd integ-tests/ruby
mise install  # This will install Ruby version from .mise.toml
(cd ../../engine/language_client_ruby && mise exec -- rake compile)
```

3. Install dependencies and generate client:
```bash
mise exec -- bundle install
mise exec -- baml-cli generate --from ../baml_src
```

4. Run tests:
```bash
dotenv -e ../.env -- mise exec -- rake test  # or use infisical for internal BAML devs
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

### Common Issues and Solutions

1. **Environment Setup**:
   - For external contributors:
     - Create a `.env` file with required API keys
     - Use `dotenv` commands instead of Infisical
   - For BAML internal developers:
     - Ensure Infisical is configured correctly
     - Use `infisical run` commands

2. **Client Generation**:
   - Ensure BAML CLI is up to date: `baml update-client`
   - Check BAML source files for errors
   - Regenerate client code after changes

3. **Build Issues**:
   - Clean and rebuild language clients
   - Check language-specific toolchain versions
   - Verify all dependencies are installed

4. **Environment Variables**:
   - Set up Infisical correctly
   - Verify API keys are present
   - Check `.env` file if not using Infisical

5. **Test Timeouts**:
   - Adjust timeout settings in test configurations
   - Consider rate limiting for API calls
   - Use appropriate test annotations/decorators

### OpenAPI Server Testss

1. Navigate to the test directory:
   - `cd engine/baml-runtime/tests/`

2. Run tests with:

- `cargo test --features internal`

This will run the baml-serve server locally, and ping it. You may need to change the PORT variable for your new test to use a different port (we don’t have a good way of autoselecting a port).

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
   - `npx turbo build --force`
   - Open VSCode and go to the Run and Debug section (play button near the extensions button).
   - Select "Launch VSCode Extension" and press the play button.
     - This will open a new VSCode window in Debug mode.
     - You can open a simple BAML project in this window (refer to our quickstart guide to set up a simple project, or clone the `baml-examples` repository).

4. Reload the extension:
   - Use `Command + Shift + P` to reload the extension when you change any core logic.
   - Alternatively, close and reopen the playground if you rebuild the playground.


To rebuild the playground UI:

1. `cd typescript/vscode-ext/packages/web-panel`
2. `pnpm build`
3. Close and open the playground in your “Debug mode VSCode window”

## Testing [promptfiddle.com](http://promptfiddle.com)

This is useful if you want to iterate faster on the Extension UI, since it supports hot-reloading.

1. Navigate to the Fiddle Frontend directory:
   - `cd typescript/fiddle-frontend`

2. Start the dev server:
   - `pnpm dev`

3. Modify the files in `typescript/playground-common`

4. Use the `vscode-` prefixed tailwind classes to get proper colors.
