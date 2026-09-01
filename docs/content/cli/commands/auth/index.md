---
title: "baml auth"
description: "Manage the identity used by BAML services."
---


Manage the identity used by BAML services.

CLI version: `baml-cli 0.18.0`

```text
$ baml help auth
Manage the identity used by BAML services.

Use `baml auth login` to authenticate, `baml auth whoami` to inspect the current identity, and `baml
auth logout` to remove the authenticated session.

Usage: baml auth [OPTIONS] <COMMAND>

Examples:
  Log in:
    baml auth login

  Show the current identity:
    baml auth whoami

  Log out:
    baml auth logout

Commands:
  login   Log in to Boundary with your email
  whoami  Show the current identity
  logout  Log out (keeps your anonymous feedback id)

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



Use `baml help auth <command>` for more information on a specific command.
```
