# 01: The problem with collecting data

<!-- Story doc 1. No Studio table names appear in this doc; no term taught in
a later doc is used as a term of art. Prior-art subsections are largely fresh
analysis (no recorded comparison exists in the corpus); marked inline. -->

**Key points**

- Four questions follow an LLM misbehavior: counts across every call, the
  exact failing call in context, what data is missing, and which compiled
  code produced it.
- Per-call records answer the first two at a cost that grows with traffic.
  Studio does not create a row per call by default.
- OTel, Langfuse, Logfire, and Braintrust are bolt-on record-per-operation
  designs. None provides complete counting, loss accounting, and
  compiled-version identity together.
- Studio keeps bounded summaries of every call, plus exact evidence for a
  policy-selected few.

## Four questions

A small BAML program processes customer signups: one function takes a
batch of customers and, for each, validates the email address and calls an
LLM function to classify the customer as Approve, Review, or Reject.
Yesterday's batch contained Ada, Bo, and Cy. Bo's classification call
failed (HTTP 500 from the model provider) and the code fell back to a safe
default. Cy's call succeeded but took over six seconds. Doc 02 shows the
program in full; every document in this set uses it.

**1. What happened across every call?** Did the classifier fail once today
or a thousand times? Is six seconds an outlier or typical? These are
counting questions, and a count over a sample is an estimate, not an
answer.

**2. Which exact call went wrong, and what surrounded it?** For Bo: the
failing call, its actual input, the actual provider error, and what else
the program was doing at that moment. Totals cannot answer this.

**3. What is missing?** Acting on the first two answers requires knowing
whether the collection machinery itself dropped anything. A dashboard that
silently omits lost data misleads the reader.

**4. Which code produced it?** Behavior changed; did the code? Each
observation should carry the exact compiled version that produced it, so
before/after comparisons are a query rather than a reconstruction.

Every telemetry system trades off across these four questions
(**telemetry**: the data a system emits about its own execution). LLM
applications make the trade-offs unusually hard, and each existing tool
gives up a property Studio requires.

## What LLM applications do to telemetry

For ordinary web services, the interesting telemetry is mostly *shape*
(status codes, latencies, counts) and payloads are small. LLM applications
differ in four ways.

**The interesting data is values.** Diagnosing an LLM misbehavior requires
the exact prompt, the structured output, and the error body. For Bo, the
useful evidence is the provider's error and the triggering input, not a
status code.

**Values are large.** A rendered prompt is commonly kilobytes; contexts
and structured outputs can be far larger. A pipeline built for 200-byte
log lines refuses, truncates, or charges heavily for a 20 KB prompt on
every call.

**Failures are bursty.** During a model-provider outage, calls fail
everywhere at once, exactly when telemetry is under maximum load and most
needed. A design that captures little in good times and floods in bad
times performs worst when it matters most. <!-- founder-concerns:
Aaron's error-storm concern; local answer in docs 03/04, hosted in 08 -->

**Capture completeness is a first-class question.** People act on this
data: refund a customer, roll back a model, declare an incident. That
requires knowing whether the evidence is complete, so question 3 is not
optional in this domain.

## The three-way tension

Three properties are needed at once:

1. **Complete counting**: every call counted, no sampling; question 1 gets
   an exact answer.
2. **Exact evidence**: real inputs, outputs, errors, and surrounding
   context for the calls that matter; question 2 gets a real answer.
3. **Bounded cost**: overhead low enough to leave on everywhere, storage
   that follows program shape rather than call volume, no cost explosion
   during incidents.

The most direct design, one record per call with its data attached,
delivers the first two and fails the third: its cost is proportional to
traffic by construction (doc 03).

The industry's standard alternatives each give up one property:

```text
one record per call   ->  complete + exact,  cost unbounded
sample (keep ~1%)     ->  exact + cheap,     completeness gone
metrics only          ->  complete + cheap,  exact evidence gone
```

Question 3 is absent from all three rows. A sampled backend returns rows
without reporting what fraction of reality they represent, or whether the
pipeline dropped data on the way in.

## Prior art

Three tool families dominate this space. Each is good at what it was built
for; none was built for the combination above. The design corpus records
almost no analysis of these products; most of this section is fresh
analysis from general product knowledge and needs review against current
vendor documentation before publication.
<!-- prior-art-history §5.7: Langfuse/Logfire/Braintrust zero recorded
analysis; OTel two thin mentions; the one recorded prior-art pass is itself
flagged "from memory, knowledge cutoff Jan 2026". -->

### OpenTelemetry, Langfuse and Logfire, and Braintrust

<!-- fresh analysis: review; recompressed 2026-08-12 from the former fat
table into a what-it-is table plus a property matrix; each cell traces to
a claim in the prior text -->

| Tool | What it is | Built for |
|---|---|---|
| **OpenTelemetry** (OTel) | The open telemetry standard: **spans** (timed records of one operation with key-value attributes) correlated into **traces** across services | Following one request across many services and teams |
| **Langfuse**, **Logfire** | LLM observability (Langfuse, open source) and OTel-based observability with strong structured-Python rendering (Logfire); prompts, outputs, and costs first-class | Inspecting recorded LLM chains, cost accounting |
| **Braintrust** | Evaluation platform: log LLM calls, build datasets from production logs, run experiments | Comparing model and prompt variants side by side |

How each behaves on the properties Studio needs:

| Tool | Data model | Cost shape | Large values | Unwrapped code | Program version | Loss accounting |
|---|---|---|---|---|---|---|
| **OTel** | one span per operation | traffic-proportional; sampled to cope | attribute bags, silent truncation | invisible | hand-annotated attribute | none (silent) |
| **Langfuse, Logfire** | one row per recorded step | traffic-proportional; record less or sample | inline payloads, costly at size | absent from the data | hand-maintained tag | no completeness contract |
| **Braintrust** | one row per logged call | proportional to what is logged | per-call capture, the practical bar | out of frame | out of frame | silent about the rest |

The short cells compress real nuance. OTel sampling turns "how many
failures today?" into an estimate and loses rare events first. A Langfuse
helper left undecorated does not exist in the data, so no counting
question covers it. Braintrust answers "how good are my outputs?", not
"what happened across every call?". Logfire inherits the OTel span model,
so its attribute-size and sampling constraints apply directly.

A few claims are recorded in the corpus rather than fresh. The one
recorded observation about OTel-native vendors notes the same
preconditions: flat span attributes, server-only operation, acceptance of
silently-empty query results. <!-- prior-art-history §5.6: SigNoz
paragraph, the recorded seed --> OTel standardized no query language
(every backend picks its own), and Studio's design supports OTLP export
only as an explicitly lossy translation, never the source of truth.
<!-- prior-art-history §5.6: steelman Part 3; studio-contract §3
"explicitly lossy interoperability projection" --> One of Studio's stated
goals is to collect everything Braintrust does, and more.
<!-- founder-concerns A2: recorded aspiration, not a recorded product
analysis -->

### Instrumentation is chosen in advance

Which trace is needed becomes clear only after an incident. Every
instrument-first tool requires choosing upfront what to wrap, log, and
sample, and incidents regularly occur where nothing was instrumented. A
system that counts every call and preserves interesting moments
automatically does not require predicting what will matter.

### What all three share

One architecture underlies all three: a library added to the application
emits one record per interesting operation to a backend. Three
consequences cannot be fixed from outside: cost proportional to traffic,
so completeness is traded for affordability; values stored as blobs
attached to records, truncated or dropped under pressure; and a recording
layer that neither sees what it did not wrap nor accounts for what it
lost.
<!-- fresh analysis: review; synthesis of the subsections above -->
These are reasonable designs for tools that must work with any program in
any language. Studio does not build on one of them because the properties
it needs are properties a bolt-on cannot provide.

## What the BAML runtime changes

BAML programs run inside a runtime the language owns. The runtime sees
every function call without wrapping, all spawned concurrent work and its
suspensions, and every value crossing a function boundary as a typed BAML
value rather than a string blob. It knows the exact compiled program, down
to a content hash, because it compiled it. This substrate is on the
current branch today and always on **[built]**.
<!-- vocabulary-lifecycle §8m: the branch has the
profiler/CAS/history/fold substrate; ROOT ledger "Built core" -->

Studio's position is that telemetry should be a language feature, not a
bolt-on. An observer inside the runtime answers all four questions.
In-process summaries count every call without shipping records anywhere
(question 1). Evidence is selected by policy, not by a 1% chance
(question 2). The observer knows what it dropped, so "what is missing?"
is a query (question 3). The runtime knows its compiled identity
**[built]**, so no hand-updated version tag is needed (question 4).
Querying all of it in plain SQL through one `baml query` command is the
committed v1 target **[v1]**.
<!-- decisions-plan: D1–D16 settled; baml query not on the
branch today -->

The design has two layers, used throughout this set:

```text
layer 1: the complete layer
  cheap, bounded summaries of EVERY call - always on, no sampling
  answers: "what happened across every call?"

layer 2: the retained layer
  exact evidence - real events and values - for the INTERESTING few,
  selected by explicit policy, not by chance
  answers: "which exact call, with what context?"
```

### Why Studio does not build on OTel

Each property above lives where OTel is unopinionated or absent. Complete
counting requires *not* emitting a span per call: a rejection of the data
model, not an extension of it. Typed, deduplicated values need more than
attribute bags. Loss accounting has no standard channel. Interoperability
survives: Studio's design includes bounded, policy-aware OTLP export,
explicitly lossy and never the source of truth **[v1]**.
<!-- vocabulary-lifecycle §1 History verbs: Export emits
JSON/JSONL/Parquet/OTLP projections, core bounded export; prior-art-history
§5.6 --> Studio's data can be exported *to* an OTel world; it could not
have been collected through one.

## Runs, not traces

Studio does not use the industry word "trace". A *trace* is a
correlation: spans from scattered instrumentation, stitched together
afterward by propagated IDs, usually sampled. Studio's unit is a **run**:
the runtime's own record of one top-level execution, recorded whole by
the runtime that executed it, with any loss declared by the recorder
itself. Nothing needs stitching. Doc 02 defines runs precisely (with
processes, calls, and threads) using the batch program above.

## Terms defined here

- **Telemetry**: the data a system emits about its own execution.
- **Span / trace**: the industry's units: a timed record of one operation,
  and a set of spans correlated by propagated IDs.
- **Sampling**: keeping a fraction of records to bound cost, at the price of
  complete counting.
- **Bolt-on SDK vs language-integrated runtime**: instrumentation you add
  and maintain versus observation the runtime performs by construction.
- **The two layers** (preview): the complete layer holds bounded summaries
  of every call, always on; the retained layer holds exact evidence for the
  interesting few, selected by policy. Docs 03–05 give both their machinery.
