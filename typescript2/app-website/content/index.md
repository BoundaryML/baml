# BAML

> The programming language for agents.

BAML is a Turing-complete programming language designed for software written by agents and humans. It feels familiar to TypeScript developers, keeps types meaningful at runtime, has explicit error handling, and includes first-class tools for discovering, running, testing, and sharing code.

Website: https://boundaryml.com/

## Why a new language?

Major changes in computing have historically produced languages designed around the new constraints:

| Computing paradigm | Language |
| --- | --- |
| Hardware | Assembly |
| Operating systems | Java |
| Web applications | JavaScript |
| Agentic coding | BAML |

JavaScript and TypeScript optimize for human productivity. TypeScript intentionally permits unsoundness and JavaScript escape hatches because those tradeoffs helped humans ship software on the web.

Agent-written software changes the tradeoff. An agent will eventually use every escape hatch available to it. BAML instead tries to make invalid states difficult to represent, keep changes local, expose nondeterminism, and give agents structured ways to inspect a codebase.

BAML combines:

- TypeScript-like syntax, unions, generics, and interfaces
- Runtime types with no `any` or unchecked casts
- Typed errors and exhaustive pattern matching
- Green threads with `spawn` and `await`
- Native LLM functions, tests, testsets, and evaluations
- `baml describe` for AST-aware code discovery
- `baml run` for executing any function from the command line
- SDK generation for incremental adoption from Python and TypeScript

## Start using BAML

Install the toolchain and initialize a project:

```sh
# Homebrew on macOS or Linux
brew install baml

# Or use the install script on macOS or Linux
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s

baml init
baml agent install
baml run main
```

`baml agent install` installs guidance matched to the BAML version in the project. Use that skill and `baml describe` when an agent writes or edits `.baml` files.

## Read next

- [Quickstart](https://boundaryml.com/quickstart.md) — install BAML, configure an editor, and run a project
- [Explore BAML](https://boundaryml.com/explore.md) — language design, agent tooling, examples, and tradeoffs
- [Pricing](https://boundaryml.com/pricing.md) — open-source and cloud pricing information
- [Changelog](https://boundaryml.com/changelog) — BAML language release notes
- [Source code](https://github.com/boundaryml/baml)
- [Demo projects](https://github.com/boundaryml/baml-demos)
- [Community Discord](https://boundaryml.com/discord)

## Project status

BAML is open source and currently pre-1.0. The language is usable today, but some APIs and tooling may continue to evolve. Use the installed agent skill, the local CLI, and `baml describe` for information that matches the version in your project.
