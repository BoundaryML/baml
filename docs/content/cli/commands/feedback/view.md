---
title: "baml feedback view"
description: "View one past report in full"
---


View one past report in full

CLI version: `baml-cli 0.18.0`

```text
$ baml help feedback view
View one past report in full

Usage: baml feedback view [OPTIONS] <ID>

Arguments:
  <ID>
          The report id (from `baml feedback list`)

Options:
      --json
          Output the record as JSON

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
