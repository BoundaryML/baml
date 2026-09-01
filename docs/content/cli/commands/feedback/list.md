---
title: "baml feedback list"
description: "List past reports"
---


List past reports

CLI version: `baml-cli 0.18.0`

```text
$ baml help feedback list
List past reports

Usage: baml feedback list [OPTIONS]

Options:
      --status <STATUS>
          Only show reports with this status

          Possible values:
          - open:      Recorded locally but not yet delivered (e.g. sent while offline)
          - anonymous: Delivered to Boundary without an email
          - reported:  Delivered to Boundary with a verified email

      --limit <LIMIT>
          Show at most this many reports (newest last)

      --json
          Output the matching records as JSON

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
