---
title: "baml feedback"
description: "Report an issue or improvement to Boundary"
---


Report an issue or improvement to Boundary

CLI version: `baml-cli 0.18.0`

```text
$ baml help feedback
Report an issue or improvement to Boundary

Usage: baml feedback [OPTIONS] [JSON]
       baml feedback <COMMAND>

Examples:
  Report an issue:
    baml feedback --title "Issue (parser): panics on nested unions"

  Include a description with a minimum repro:
    baml feedback --title "..." --description "Minimum repro: class A { ... }"

  Submit a JSON report from standard input:
    echo '{"title": "...", "description": "..."}' | baml feedback -

  Attach files:
    baml feedback --title "..." --files screenshot.png --files repro.baml

  List undelivered reports:
    baml feedback list --status open

  View one report:
    baml feedback view a1b2c3d4

Commands:
  status   Show whether feedback is enabled and the reports sent so far
  list     List past reports
  view     View one past report in full
  disable  Disable sending feedback from this machine
  enable   Re-enable sending feedback

Arguments:
  [JSON]
          Advanced: supply the fields as JSON instead of flags (an inline object, a file path, or
          `-` for stdin)

Options:
      --title <TITLE>
          One line describing the issue, in the form `Issue (feature): description`

      --description <DESCRIPTION>
          Anything relevant to help the BAML team understand your problem/suggestion; include a
          minimum repro.

          Good descriptions cover: what I was doing, what went wrong, a minimum repro (include one
          whenever possible), what I want to happen, and potential syntax ideas.

      --files <PATH>
          Attach a file (image, code, log). Repeatable: `--files a.png --files repro.baml`

      --anonymous
          Report anonymously even while logged in

      --email
          Report with your email (requires `baml auth login`)

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



Use `baml help feedback <command>` for more information on a specific command.
```
