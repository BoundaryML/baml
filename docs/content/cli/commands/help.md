---
title: "baml help"
description: "Display documentation for a command"
---


Display documentation for a command

CLI version: `baml-cli 0.18.0`

```text
$ baml help help
Display documentation for a command

Usage: baml help [OPTIONS] [COMMAND]...

Examples:
  Show help for running functions:
    baml help run

  Show help for running tests:
    baml help test

Arguments:
  [COMMAND]...
          Command path to document. Omit to show root help

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
