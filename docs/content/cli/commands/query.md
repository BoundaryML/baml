---
title: "baml query"
description: "Query the local profile store with SQL"
---


Query the local profile store with SQL

CLI version: `baml-cli 0.18.0`

```text
$ baml help query
Query the local profile store with SQL

Usage: baml query [OPTIONS] [SQL]

Examples:
  baml query "SHOW TABLES"
  baml query "SELECT thread_id, status, total_errors, started_at FROM threads WHERE parent_thread_id
  IS NULL ORDER BY started_at DESC LIMIT 20"
  baml query "SELECT fqn, sum(calls_started) calls, sum(self_ns) self_ns FROM call_path_stats GROUP
  BY fqn ORDER BY self_ns DESC LIMIT 10"
  baml query "SELECT call_id, args['customer']['age'] AS age, output FROM calls WHERE
  args['customer']['age'] >= 30 LIMIT 50" --format jsonl
  baml query --schema --table calls

Arguments:
  [SQL]
          Portable SQL against the versioned catalog (`-` reads stdin; see `--schema` and `baml
          describe query`)

Options:
      --schema
          Print the catalog profile (relations, views, columns, docs)

      --table <NAME>
          Restrict `--schema` output to one relation or view

      --format <FORMAT>
          Output format: fixed-width table, one JSON envelope, or JSON lines with a terminal outcome
          frame

          [default: table]
          [possible values: table, json, jsonl]

      --from <PATH>
          Project directory (defaults to the current directory's project)

      --explain
          Plan the statement without executing it (wraps it in EXPLAIN)

      --max-rows <N>
          Result-row budget (terminal E_QUERY_BUDGET_EXCEEDED when hit)

      --max-wall <DURATION>
          Wall-clock budget, e.g. `30s`, `1500ms`, or plain seconds

      --internal
          Show internal relations too (`BAML_INTERNAL=1` does the same)

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

Exit codes: 0 complete; 1 completed but evidence-incomplete (see the outcome's valueEvaluations); 2
invalid SQL, unknown table, or authorization; 3 query budget exceeded; 4 cancelled; 5 internal or
dependency failure (no store, bind failure, corrupt artifact).
```
