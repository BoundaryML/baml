---
title: "baml ide install"
description: "Install the active toolchain's BAML editor extension."
---


Install the active toolchain's BAML editor extension.

CLI version: `baml-cli 0.18.0`

```text
$ baml help ide install
Install the active toolchain's BAML editor extension.

Select Cursor or VS Code explicitly, or use `--output-dir` to copy the VSIX for a manual
installation. With no option, BAML selects an available supported editor.

Usage: baml ide install [OPTIONS]

Examples:
  Install into the detected editor:
    baml ide install

  Install into Cursor:
    baml ide install --cursor

  Copy the extension for manual installation:
    baml ide install --output-dir ./extensions

Editor options:
      --cursor
          Install the active toolchain's BAML VSIX into Cursor

      --code
          Install the active toolchain's BAML VSIX into VS Code

      --output-dir <PATH>
          Copy the active toolchain's BAML VSIX into a directory for manual install

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
