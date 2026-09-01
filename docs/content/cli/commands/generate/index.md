---
title: "baml generate"
description: "Generate client code from BAML definitions."
---


Generate client code from BAML definitions.

CLI version: `baml-cli 0.18.0`

```text
$ baml help generate
Generate client code from BAML definitions.

Reads every `[generator.<name>]` section in `baml.toml`, validates the project, and writes each
configured client. Use `--output-dir` to override the configured output directory for every
generator in this invocation.

Usage: baml generate [OPTIONS] [COMMAND]

Examples:
  Generate clients for the nearest project:
    baml generate

  Generate clients for a specific project:
    baml generate --project ./my-project

  Override the output directory:
    baml generate --output-dir ./generated

Commands:
  add  Add a client generator to baml.toml

Compiler options:
  -F, --features <FEATURES>
          Enable compiler features; repeatable or comma-separated [possible values: beta,
          display_all_warnings]

Generation options:
  -o, --output-dir <PATH>
          Output directory override (takes precedence over generator config)

Global options:
  -q, --quiet...
          Suppress nonessential output

  -v, --verbose...
          Increase diagnostic verbosity. Repeatable

      --color <WHEN>
          Control ANSI colors [possible values: auto, always, never]

      --no-progress
          Disable progress output

      --directory <PATH>
          Change to this directory before running the command

      --project <PATH>
          Discover the BAML project from this path

      --output-preset <PRESET>
          Select output defaults [default: auto] [possible values: auto, human, agent]

      --hyperlinks <WHEN>
          Control terminal hyperlinks [possible values: auto, always, never]

      --diagnostic-format <FORMAT>
          Select the diagnostic format [possible values: human, agent, concise]

  -h, --help
          Display concise help for this command



Use `baml help generate <command>` for more information on a specific command.
```
