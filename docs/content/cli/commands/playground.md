---
title: "baml playground"
description: "Open the BAML playground in a browser."
---


Open the BAML playground in a browser.

CLI version: `baml-cli 0.18.0`

```text
$ baml help playground
Open the BAML playground in a browser.

Serves either a discovered BAML project or one standalone source file. By default, the server
selects the first available port starting at 4265 and opens a browser unless the session is
headless.

Usage: baml playground [OPTIONS]

Examples:
  Open the nearest project:
    baml playground

  Serve a project without opening a browser:
    baml playground --project ./my-project --no-open

Compiler options:
  -F, --features <FEATURES>
          Enable compiler features; repeatable or comma-separated [possible values: beta,
          display_all_warnings]

Project options:
      --file <PATH>
          Standalone single-file source. Loads only this file (no project discovery)

Server options:
      --port <PORT>
          Listen on exactly this port (errors if unavailable). Default: the first free port from
          4265

      --no-open
          Do not open a browser. Opening is also skipped automatically in headless sessions (SSH, or
          no display on Linux)

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
