---
title: "baml agent install"
description: "Install the BAML agent skill bundled with this toolchain"
---


Install the BAML agent skill bundled with this toolchain

CLI version: `baml-cli 0.18.0`

```text
$ baml help agent install
Install the BAML agent skill bundled with this toolchain

Usage: baml agent install [OPTIONS]

Examples:
  Install the bundled skill:
    baml agent install

  Install the bundled skill in a specific project:
    baml agent install --project ./my-project

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
