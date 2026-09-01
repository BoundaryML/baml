---
title: "baml pack"
description: "Package one or more BAML targets as a standalone executable."
---


Package one or more BAML targets as a standalone executable.

CLI version: `baml-cli 0.18.0`

```text
$ baml help pack
Package one or more BAML targets as a standalone executable.

A positional target produces a single-entry executable. One or more `--function` flags produce an
executable whose generated CLI has one subcommand per function. Function parameters are derived from
BAML types.

Usage: baml pack [OPTIONS] [FUNCTION]

Examples:
  Package one function:
    baml pack main

  Choose the executable path:
    baml pack main --output ./my-tool

  Package multiple functions as subcommands:
    baml pack --function Extract --function Classify --output ./baml-tools

  Package a function from a standalone file:
    baml pack --file script.baml main

Arguments:
  [FUNCTION]
          Function to package as the executable's only entry point.

          Mutually exclusive with `--function`.

Compiler options:
  -F, --features <FEATURES>
          Enable compiler features; repeatable or comma-separated [possible values: beta,
          display_all_warnings]

Target options:
  -f, --function <NAME>
          Add a function as a generated executable subcommand. Repeatable.

          Even one `--function` creates a subcommand. Use a positional `<TARGET>` to produce an
          executable with no subcommand layer.

Project options:
      --file <PATH>
          Load one standalone source file instead of discovering a project.

          Mutually exclusive with `--project`.

Build options:
  -o, --output <OUTPUT>
          Path for the packaged executable.

          Defaults to `[package].name`, the project directory name, or the source file stem,
          depending on the project mode.

      --target <TRIPLE>
          Target triple for the packaged executable.

          Defaults to the host platform. Cross-compilation downloads the matching pack host from the
          BAML release artifacts.

Runtime options:
      --output-format <FORMAT>
          Format returned values [default: json] [possible values: debug, json]

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
