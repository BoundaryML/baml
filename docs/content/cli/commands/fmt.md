---
title: "baml fmt"
description: "Format BAML source files."
---


Format BAML source files.

CLI version: `baml-cli 0.18.0`

```text
$ baml help fmt
Format BAML source files.

With explicit paths, formats those files or directories. With no paths, discovers the nearest BAML
project and formats all of its `.baml` files. If no project is found, the command succeeds without
changing anything.

Usage: baml fmt [OPTIONS] [PATHS]...

Examples:
  Format the nearest project:
    baml fmt

  Format a specific file:
    baml fmt baml_src/main.baml

  Preview formatted output:
    baml fmt --dry-run

Arguments:
  [PATHS]...
          Specific files to format. If omitted, all `.baml` files in the project are formatted.

Output options:
  -n, --dry-run
          Write formatter changes to stdout instead of files.

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
