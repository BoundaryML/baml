# Start here

This set of eleven short documents describes **Project Studio**, BAML's
observability initiative. It is the reader-facing entry point, replacing
the earlier data-model-and-query-examples document. The documents build
the execution model first and introduce vocabulary one document at a
time; no term is used before it is defined.

This document reads like a BEP:
prior art, the problem, the design, worked examples, and the artifact an
agent loads. Cloud-internal engineering (ingest pipelines, how the hosted
query tables are rebuilt, outage behavior of the hosted service) is not
covered here. Document 08 states what to trust and where that internal
design lives.

## The two layers

Every table in Studio belongs to one of two layers.

```text
                     every call, always            the interesting few
                  ┌──────────────────────┐      ┌──────────────────────┐
   your program → │   THE COMPLETE LAYER │      │  THE RETAINED LAYER  │
    (any volume)  │  small summaries of  │      │ exact calls, event   │
                  │  every single call   │      │ tape, and captured   │
                  │  (cheap, bounded)    │      │ values, selected by  │
                  │                      │      │ explicit policy      │
                  └──────────────────────┘      └──────────────────────┘
                     answers "how much,            answers "show me
                     how often, how slow"          exactly what happened"
```

The complete layer is never sampled, so its counts and rates are exact.
When capture itself loses data, the loss is recorded, not hidden
(document 06). The retained layer is never complete; it is an explicit
selection. Every table belongs to one layer or the other, and the
documents state which.

## The documents

| # | Topic | Summary |
|---|---|---|
| [01](01-why-not-otel.md) | The problem with collecting data | Why span-based telemetry fits LLM applications poorly, what OTel, Langfuse, and Braintrust provide, and what a language-integrated runtime can do instead |
| [02](02-what-is-running.md) | What is actually running | Process, run, call, child call, and logical thread, defined on an example program; why the unit is a run, not a trace |
| [03](03-count-everything.md) | Counting every call | Calling contexts: a million invocations of one call path update one row |
| [04](04-keep-the-interesting-ones.md) | Keeping the interesting ones | The rolling tape, tape dumps, and retained calls: the rows each table stores, the links between them, and the trigger policy |
| [05](05-values.md) | Values: inputs, outputs, errors | Which calls capture args, return, and error values; what promote-on-error does today; what a million failures store |
| [06](06-is-the-data-trustworthy.md) | When the evidence is incomplete | Evidence issues vs. application errors; execution status vs. evidence availability; the query outcome |
| [07](07-which-code-was-this.md) | Relating runs to code | Revisions, `definition_key`, and which identifiers are stable across revisions |
| [08](08-leaving-the-laptop.md) | From the laptop to the cloud | Seal-then-upload, where each kind of data lives, error storms, and what stays in the internal cloud doc |
| [09](09-the-catalog.md) | Table schemas | Every table: grain, example rows, and rationale; one proposed table was cut |
| [10](10-how-do-i.md) | How do I build …? | Eight end-to-end query recipes and a dashboard checklist |
| [11](11-the-agent-skill.md) | The agent skill | The system compressed into the one file an agent loads |

Reading paths:

- **Full review**: 01 → 11 in order. Roughly 90 minutes.
- **Verify the data model**: 02 → 03 → 04 → 06, then 09.
- **See the user-facing result first**: 01 → 10 → 11.
- **Check schemas**: 09, with 03/04/05 as background.

## How claims are labeled

Every load-bearing claim carries one of three labels. The design corpus
forbids presenting an implementation default as product policy; the
labels keep the two distinct.

- **[built]**: on this branch today. Numbers under this label (the 250 ms
  summary-update interval, the 16 MiB in-memory event buffer, capture
  rate limits; documents 03–04) are implementation defaults, reported so
  behavior can be reasoned about. They are not frozen contracts.
- **[v1]**: a committed target with a delivery gate. Designed, not built.
- **[open]**: a decision not yet made. The document lists the options.
  Some [open] items wait on benchmark data and some are deliberately
  deferred; the surrounding text says which.

## Names in this set

This set uses proposed reader-facing table names throughout. Each is a
rename proposal **[open]**, mapped here and again where the table is
defined in [09](09-the-catalog.md):

| Internal / historical name | Name used in this set | Row means |
|---|---|---|
| `cct_population` | `calling_contexts` | Totals for one calling path within one run |
| `exact_windows` | `tape_dumps` | One preserved slice of the rolling event tape |
| `spawn_edges` | `thread_edges` | Totals for one parent-context → spawned-function relationship |
| `spawn_instances` | `retained_threads` | One individually retained spawned task |
| `llm_population` | `llm_usage` | Token/error totals for one context × provider × model |
| `runs`, `retained_calls`, `evidence_issues`, `functions`, `call_sites`, `revisions` | unchanged | |

"CCT" (calling-context tree) remains the engineering term. Document 03
defines it so the internal design documents stay readable.

## Where this set sits

- The design authority is [`CANONICAL/design/`](../../design/README.md);
  nothing here overrides it. Where this set simplifies, it says so. Where
  the design is unfrozen, the claim is labeled [open].
- [query-examples](../query-examples.md) remains the schema-level
  reference behind document 09. This set is the entry point; that file is
  the appendix.
