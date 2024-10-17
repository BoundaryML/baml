# Contributing

First off, thanks for your interest in contributing to BAML! We appreciate all the help we can get in making it the best way to build any AI agents or applications.

Before contributing, do try to let us know what task you want to take on, and let us know in our [Discord](https://discord.gg/BTNBeXGuaS) #contributing channel.

Here is our guide on getting setup.

### Compiler Architecture

<TBD — we will write more details here>

- baml-cli / VSCode generates `baml_client` which contains all the interfaces people use to call the `baml-runtime`
- Pest grammar → AST (build diagnostics for linter) → IntermediateRepr
- baml-runtime parses baml files + builds and calls LLM endpoints using internal LLM providers, then parses the data into “jsonish”, and finally coerces that jsonish into the schema.

## Example feature PRs

1. Fix parsing issues:
   1. https://github.com/BoundaryML/baml/pull/1031/files
   2. Coerce ints properly ($3,000 → 3000) https://github.com/BoundaryML/baml/pull/1023
2. Fix syntax highlighting and a grammar parser crash https://github.com/BoundaryML/baml/pull/1013/files
3. Implement literal types like `sports "SOCCER" | "BASKETBALL"` https://github.com/BoundaryML/baml/pull/978
4. Fix issue with openai provider https://github.com/BoundaryML/baml/pull/896/files
5. Implement `map` type https://github.com/BoundaryML/baml/pull/797 (see list of items in the PR)

## Setting up the compiler / runtime in `engine`

1. Install rust
2. run `cargo build` in `engine/` and make sure everything builds on your machine.
3. run some of the unit tests:
   1. `cd engine/baml-lib/baml && cargo test` will run some of our grammar linting tests for example.
4. Run the integration tests.

## Running Integration Tests

Setup your environment variables in an .env file with

OPENAI_API_KEY=”your key” (you mainly just need this one).

Make sure your shell reads these env variables setup so it injects them into the test process, since some of the test scripts don’t try to load these from any .env file yet and just assume the process has them. You can try to use [dotenv-cli](https://www.npmjs.com/package/dotenv-cli)

1. **Python**
   1. Install poetry [https://python-poetry.org/docs/](https://python-poetry.org/docs/)
   2. `cd integ-tests/python`
   3. `poetry shell` (install `poetry` if you don’t have it, and python 3.8)
   4. `poetry lock && poetry install`
   5. `env -u CONDA_PREFIX poetry run maturin develop --manifest-path ../../engine/language_client_python/Cargo.toml` (this builds the compiler and injects the package into the virtual env)
   6. `poetry run baml-cli generate --from ../baml_src` (generate the baml_client)
   7. `poetry run python -m pytest -s`
      1. run a specific test: `poetry run python -m pytest -s -k "my_test_name"`
2. **TypeScript**
   1. Install pnpm [https://pnpm.io/installation](https://pnpm.io/installation)
   2. before that, run `pnpm i` in the `engine/language_client_typescript`
   3. `cd integ-tests/typescript`
   4. `pnpm i`
   5. `pnpm build:debug` (builds your new compiler changes)
   6. `pnpm generate` (generates baml_client for your tests with any new changes)
   7. `pnpm integ-tests` or `pnpm integ-tests -t "my test name"`
3. Ruby
4. **OpenAPI server:**
   1. `cd engine/baml-runtime/tests`
   2. `cargo test --features internal`
   3. This will run the baml-serve server locally, and ping it. You may need to change the PORT variable for your new test to use a different port (we don’t have a good way of autoselecting a port.
   4. To test a particular OpenAPI client (TBD instructions)

### Testing grammar changes

1. Use the playground in [https://pest.rs/](https://pest.rs/) to test your grammar with the new syntax
   1. modify the existing `.pest` file to update the grammar
   2. modify the AST parsing of the new grammar
   3. modify the IR (IntermediateRepr)
   4. ensure you pass all the existing `cargo test` validations in `engine/baml-lib/`
   5. ensure integ tests still pass.
2. We also have a grammar for [`promptfiddle.com`](http://promptfiddle.com) syntax rendering that uses Lezer that you may have to modify. There’s other playground websites for Lezer you can check out.

### Testing VSCode Extension

This requires a macos or linux machine, since we symlink some playground files between both [PromptFiddle.com](http://PromptFiddle.com) website app, and the VSCode extension itself.

**Note:** If you are just making changes to the VSCode extension UI, you may want to go to the section: “Testing Promptfiddle.com”

1. `cd typescript`
2. `pnpm i`
3. `npx turbo build --force`
4. Go to VSCode → Run and Debug (play button near extensions button) → Launch VSCode extension (press play button)
   1. This launches a new VSCode window in Debug mode.
   2. You can try and open up a simple baml project in this window (read our quickstart to setup a simple project, or clone `baml-examples` repo)
5. Reload the extension (command + shift + p) when you change any core logic in the extension, or just close and open the playground if you rebuild the playground.

To rebuild the playground UI

1. `cd typescript/vscode-ext/packages/web-panel`
2. `pnpm build`
3. Close and open the playground in your “Debug mode VSCode window”

### Testing [prompfiddle.com](http://prompfiddle.com)

This is useful if you want to iterate faster on the Extension UI, since it supports hot-reloading.

1. `cd typescript/fiddle-frontend`
2. `pnpm dev`
3. Modify the files in `typescript/playground-common`.
4. Use the `vscode-` prefixed tailwind classes to get proper colors.
