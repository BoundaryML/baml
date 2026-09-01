---
title: "baml ide"
description: "Install or manage IDE integration assets"
---


Install or manage IDE integration assets

CLI version: `baml-cli 0.18.0`

```text
$ baml help ide
Install or manage IDE integration assets

Usage: baml ide [OPTIONS] <COMMAND>

Examples:
  Install into the detected editor:
    baml ide install

  Install into Cursor:
    baml ide install --cursor

Commands:
  install  Install the active toolchain's BAML IDE extension

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



Use `baml help ide <command>` for more information on a specific command.
```
