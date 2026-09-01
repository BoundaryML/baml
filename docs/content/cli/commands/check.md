---
title: "baml check"
description: "Check BAML source files for compiler errors."
---


Check BAML source files for compiler errors.

CLI version: `baml-cli 0.18.0`

```text
$ baml help check
Check BAML source files for compiler errors.

Discovers the nearest BAML project from the search path, checks every source file in that project,
and prints compiler errors and warnings.

Usage: baml check [OPTIONS]

Examples:
  Check the nearest project:
    baml check

  Check a specific project:
    baml check --project ./my-project

Compiler options:
  -F, --features <FEATURES>
          Enable compiler features; repeatable or comma-separated [possible values: beta,
          display_all_warnings]

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
