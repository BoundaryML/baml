---
title: "baml init"
description: "Scaffold a new BAML project under the given directory (default `.`). Refuses to clobber an existing `baml.toml`."
---


Scaffold a new BAML project under the given directory (default `.`). Refuses to clobber an existing `baml.toml`.

CLI version: `baml-cli 0.18.0`

```text
$ baml help init
Scaffold a new BAML project under the given directory (default `.`). Refuses to clobber an existing
`baml.toml`.

Creates `baml.toml` and `baml_src/main.baml`. The destination directory may already exist, but it
must not already contain a BAML manifest.

Usage: baml init [OPTIONS] [PATH]

Examples:
  Initialize the current directory:
    baml init

  Initialize a directory with an explicit project name:
    baml init ./my-project --name my_project

Arguments:
  [PATH]
          Directory to initialize. Defaults to the current directory

          [default: .]

Project options:
      --name <NAME>
          Project name written to `baml.toml`'s `[package].name`. Defaults to the basename of
          `<PATH>` (or `baml-project` for `.`)

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
