---
title: "baml lsp"
description: "Start a BAML language server over standard input and output."
---


Start a BAML language server over standard input and output.

CLI version: `baml-cli 0.18.0`

```text
$ baml help lsp
Start a BAML language server over standard input and output.

Editor integrations normally start this command automatically. Use one or more `--workspace` paths
when launching it outside an editor client.

Usage: baml lsp [OPTIONS]

Examples:
  Start a language server:
    baml lsp

  Add a workspace root:
    baml lsp --workspace ./my-project

Workspace options:
      --workspace <PATH>
          Workspace root to discover BAML projects from when running the LSP outside an editor
          client. May be passed more than once

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
