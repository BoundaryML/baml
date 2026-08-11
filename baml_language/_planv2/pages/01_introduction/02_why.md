# Why

## The problem

Applications that call models accumulate three kinds of untyped glue.
Prompts are strings, so the schema the model must produce lives apart
from the type the code expects, and the two drift. Provider SDKs
disagree about message shapes, tool formats, and error surfaces, so
supporting a second provider forks the calling code. Agent loops —
call the model, run the tools, feed results back, repeat — are
rewritten per application, and each rewrite makes its own decisions
about retries, budgets, and what gets recorded.

## The approach

BAML already types the first problem away: an LLM function's return
type is the schema, rendered into the prompt and enforced by the
parser. This BEP types the other two.

Execution decomposes into three values with small interfaces. A spec
(`MyFunc@spec(args)`) is one unit of model work with its arguments
bound. A runner drives a spec to completion; the built-in `Agent`
runner owns the loop, tool execution, and budgets. A client performs
one model turn over one provider wire API and returns canonical
content. Because runners and clients are ordinary values implementing
ordinary interfaces, an application can replace either without new
language features, and a new provider is one class rather than a fork
of the calling code.

Every run appends typed events to a journal. The journal is the
transcript source for the next model turn and the trace of the
finished run, so recording is not an integration to add later.

## What you do not get

This BEP does not include long-lived conversations. There is no
built-in session object, no steering channel, no policy layer, and no
background job handle. A run starts, loops, and returns. Each of
these arrives in a later phase without breaking this BEP's surface
(`../05_appendix/03_future_phases.md`) — and none of them is gated on
waiting: the loop's primitives are public, so a custom runner can
hold a journal across turns, append user messages between them, or
return a handle instead of a value today
(`../02_guides/02_specs_and_runners/03_writing_a_runner.md`).

There is no graph or state-machine DSL. Control flow between runs is
ordinary BAML code.

Streaming is an optional client capability. `StreamingClient.invoke_stream`
returns a pull-based one-turn stream, and an LLM function's `$stream` companion
exposes typed partial values while retaining the same terminal result shape.
The journal records only the completed turn, not token deltas, so streaming
does not change replay semantics.

## Relation to other systems

pi separates provider descriptors from reusable wire API
implementations and keeps local state sufficient for resume; this BEP
adopts both. Pydantic AI and the OpenAI Agents SDK persist provider
message arrays; this BEP records typed events instead and renders them
per provider. `../05_appendix/01_comparisons.md` records the
comparisons in detail, including the earlier BAML designs.
