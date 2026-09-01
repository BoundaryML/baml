---
title: "baml auth login"
description: "Log in to Boundary with your email"
---


Log in to Boundary with your email

CLI version: `baml-cli 0.18.0`

```text
$ baml help auth login
Log in to Boundary with your email

Usage: baml auth login [OPTIONS]

Examples:
  Log in using a browser:
    baml auth login

  Print the verification URL instead:
    baml auth login --no-open

Options:
      --no-open
          Print the verification URL instead of opening a browser

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
