---
title: "baml generate add"
description: "Add a client generator to baml.toml"
---


Add a client generator to baml.toml

CLI version: `baml-cli 0.18.0`

```text
$ baml help generate add
Add a client generator to baml.toml

Usage: baml generate add [OPTIONS] <OUTPUT_TYPE>

Arguments:
  <OUTPUT_TYPE>
          [possible values: python/pydantic2, python/pydantic/v1, typescript/node, swift, go, rust,
          typescript/web, java, cpp, csharp]

Options:
      --sdk-import-path <IMPORT_PATH>
          Go module import path for the generated baml_sdk package

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
```
