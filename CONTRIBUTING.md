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

1. Setup your environment variables in an `.env` file with:

- `OPENAI_API_KEY=”your key”` (you mainly just need this one).

2. Ensure the environment variables are into the test process. You can use [dotenv-cli](https://www.npmjs.com/package/dotenv-cli) to do this.


### Python Integration Tests

1. Install poetry [https://python-poetry.org/docs/](https://python-poetry.org/docs/)

2. Navigate to the Python integration tests: `cd integ-tests/python/`

3. Run the following commands:
   - `poetry shell`
   - `poetry lock && poetry install`
   - `env -u CONDA_PREFIX poetry run maturin develop --manifest-path ../../engine/language_client_python/Cargo.toml`
   - `poetry run baml-cli generate --from ../baml_src`
   - `poetry run python -m pytest -s`
   - To run a specific test: `poetry run python -m pytest -s -k "my_test_name"`


### TypeScript Integration Tests

   1. Install pnpm: [https://pnpm.io/installation](https://pnpm.io/installation)

   2. Navigate to the Language Client TypeScript directory and install dependencies:
      - `cd engine/language_client_typescript/`
      - `pnpm i`

   3. Navigate to the TypeScript integration tests:
      - `cd integ-tests/typescript/`
   
   4. Run the following commands:

      - `pnpm i` (install dependencies)
      - `pnpm build:debug` (builds your new compiler changes)
      - `pnpm generate` (generates `baml_client` for your tests with any new changes)
      - `pnpm integ-tests` or `pnpm integ-tests -t "my test name"`


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

## Testing [prompfiddle.com](http://prompfiddle.com)

This is useful if you want to iterate faster on the Extension UI, since it supports hot-reloading.

1. Navigate to the Fiddle Frontend directory:
   - `cd typescript/fiddle-frontend`

2. Start the dev server:
   - `pnpm dev`

3. Modify the files in `typescript/playground-common`

4. Use the `vscode-` prefixed tailwind classes to get proper colors.
