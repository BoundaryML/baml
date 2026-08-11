# Start here: count everything, keep the interesting ones

This set of eleven short documents is the reader-facing description of
**Project Studio**, BAML's observability initiative. It replaces the single
data-model-and-query-examples document as the entry point. That document
asked readers to review storage schemas before they had a model of how a
BAML program executes. This set builds the model first and introduces
vocabulary one document at a time; no term is used before it is defined.

The structure follows a BAML Enhancement Proposal (BEP): background and
prior art, the problem, the design, worked examples, and finally the
artifact an agent loads. Deep cloud engineering (ingest pipelines, how the
hosted query tables are rebuilt, outage behavior of the hosted service) is
deliberately **not here**. Document 08 states what to trust and where that
internal design lives.

## The one idea

Everything in Studio is an instance of one pattern: two layers of truth.

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

The complete layer is never sampled, so counts and rates are true. When
capture itself loses data, the loss is declared, not hidden (document 06).
The retained layer is never complete; it is an explicit selection. Every
table belongs to one layer or the other, and the documents always say
which.

## The documents

| # | Topic | Summary |
|---|---|---|
| [01](01-why-not-otel.md) | The problem with collecting data | Why LLM apps break span-shaped telemetry, what OTel/Langfuse/Braintrust get right and miss, and the bet a language-integrated runtime can make |
| [02](02-what-is-running.md) | What is actually running | Process, run, call, child call, and logical thread, taught on a toy program; why the term is *runs*, not *traces* |
| [03](03-count-everything.md) | Counting every call without storing every call | Calling contexts: a million invocations of one pattern are increments to one row |
| [04](04-keep-the-interesting-ones.md) | Keeping the interesting ones | The rolling tape; tape dumps and retained calls taught together, down to the rows each table stores and the links between them; trigger policy (what fires one, what deliberately does not) |
| [05](05-values.md) | Values: inputs, outputs, errors | Which calls get args/return/error captured, what promote-on-error does today, and what a million failures stores |
| [06](06-is-the-data-trustworthy.md) | When the evidence itself is incomplete | Evidence issues vs app errors; execution status vs evidence availability; every query grades its own completeness |
| [07](07-which-code-was-this.md) | Relating runs to code | Revisions, `definition_key`, and which identifiers survive across revisions vs die with one |
| [08](08-leaving-the-laptop.md) | From your laptop to the cloud | Seal-then-upload, where each kind of data lives, the error-storm answer, and what stays in the internal cloud doc |
| [09](09-the-catalog.md) | Table schemas | Every table: what one row means, example rows, why it earns its keep; and the one table that failed the test |
| [10](10-how-do-i.md) | How do I build …? | Eight end-to-end recipes plus a dashboard checklist: dashboards, drill-downs, incident timelines, revision comparisons, spend, trust |
| [11](11-the-agent-skill.md) | The agent skill | The system compressed into the one file an agent loads; proof the vocabulary works |

Reading paths:

- **Full review** (founders): 01 → 11 in order. Roughly 90 minutes.
- **"Convince me the model is right"**: 02 → 03 → 04 → 06, then 09.
- **"Show me what users get"**: 01 → 10 → 11. Shows the results before
  the mechanism.
- **"I want to check schemas"**: 09, with 03/04/05 as backing.

## How claims are labeled

Every load-bearing claim carries one of three labels. The design corpus
forbids presenting an implementation default as product policy, and the
labels keep the two distinct:

- **[built]**: on this branch today. Numbers under this label (the 250 ms
  summary-update interval, the 16 MiB in-memory event buffer, capture rate
  limits; all covered in documents 03–04) are *implementation defaults*,
  reported so behavior can be reasoned about. They are not frozen
  contracts.
- **[v1]**: a committed target with a delivery gate. Designed, not built.
- **[open]**: a decision not yet made. The document lists the
  options instead of pretending one was chosen. Some [open] items wait on
  benchmark data and some are deliberately deferred; the surrounding prose
  says which.

## Names in this set

This set uses proposed reader-facing names throughout rather than waiting
for review. Each is a rename **proposal [open]**, mapped here and again
where the table is defined in [09](09-the-catalog.md):

| Internal / historical name | Name used in this set | Row means |
|---|---|---|
| `cct_population` | `calling_contexts` | Totals for one calling path within one run |
| `exact_windows` | `tape_dumps` | One preserved slice of the rolling event tape |
| `spawn_edges` | `thread_edges` | Totals for one parent-context → spawned-function relationship |
| `spawn_instances` | `retained_threads` | One individually retained spawned task |
| `llm_population` | `llm_usage` | Token/error totals for one context × provider × model |
| `runs`, `retained_calls`, `evidence_issues`, `functions`, `call_sites`, `revisions` | unchanged | |

"CCT" (calling-context tree) remains the engineering term. Document 03
teaches it so the internal design docs stay readable.

## Where this set sits

- The design authority is [`CANONICAL/design/`](../../design/README.md);
  nothing here overrides it. Where this set simplifies, it says so. Where
  the design is unfrozen, the claim is labeled [open].
- [query-examples](../query-examples.md) remains the schema-level reference
  behind document 09. This set is the entry point; that file is the
  appendix.
