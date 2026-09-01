---
title: "baml agent"
description: "Install the toolchain's BAML agent skill for this project"
---


Install the toolchain's BAML agent skill for this project

CLI version: `baml-cli 0.18.0`

```text
$ baml help agent
Install the toolchain's BAML agent skill for this project

Usage: baml agent [OPTIONS] <COMMAND>

Examples:
  Install the bundled skill:
    baml agent install

  Install in a specific project:
    baml agent install --project ./my-project

Commands:
  install  Install the BAML agent skill bundled with this toolchain

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



Use `baml help agent <command>` for more information on a specific command.
```
