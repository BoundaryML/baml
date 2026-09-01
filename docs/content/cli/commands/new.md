---
title: "baml new"
description: "Create a fresh directory at `<PATH>` and scaffold a project inside. Refuses to run if `<PATH>` already exists, the same way `cargo new` does."
---


Create a fresh directory at `<PATH>` and scaffold a project inside. Refuses to run if `<PATH>` already exists, the same way `cargo new` does.

CLI version: `baml-cli 0.18.0`

```text
$ baml help new
Create a fresh directory at `<PATH>` and scaffold a project inside. Refuses to run if `<PATH>`
already exists, the same way `cargo new` does.

Creates the destination directory, `baml.toml`, and `baml_src/main.baml`. Use `baml init` when the
directory already exists.

Usage: baml new [OPTIONS] <PATH>

Examples:
  Create a project directory:
    baml new ./my-project

  Set an explicit project name:
    baml new ./my-project --name my_project

Arguments:
  <PATH>
          Directory to create. Errors if it already exists

Project options:
      --name <NAME>
          Project name written to `baml.toml`'s `[package].name`. Defaults to the basename of
          `<PATH>`

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
