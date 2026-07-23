# Prior Art and Provider Research

This proposal is designed around interaction shapes already present in major APIs and frameworks. The goal is not to copy one vendor. It is to preserve the differences that remain stable across vendors.

Sources were reviewed on 2026-07-09. Provider APIs change frequently; implementation work must re-check the official docs.

## OpenAI: background responses

OpenAI's [Background mode](https://developers.openai.com/api/docs/guides/background) starts a long-running response asynchronously and returns a response object that the caller polls while it is queued or in progress. The documentation also shows resuming a background stream from an event sequence number and notes that background data is stored to enable polling.

Design lessons:

- background execution is a lifecycle, not a boolean property of every call;
- submission and polling are different operations with different replay safety;
- the remote response ID belongs inside a typed job resource;
- stream resumption needs a cursor owned by the resource;
- retention/privacy behavior must be visible to the caller.

This directly motivates `Background -> Job<T>` rather than `call(background = true) -> T?`.

## OpenAI: durable conversation state

OpenAI's [Conversation state guide](https://developers.openai.com/api/docs/guides/conversation-state) documents both `previous_response_id` continuation and conversation objects with durable identifiers that can be reused across sessions, devices, or jobs.

Design lessons:

- application-held message history and provider-held conversation state are different modes;
- continuation IDs have an owner and billing/retention semantics;
- a session resource is a better abstraction than a string handle passed back to a general provider;
- a session can run many different typed LLM tasks, so it should consume `LlmRequest<T>`.

## OpenAI: realtime

The [Realtime API reference](https://developers.openai.com/api/reference/resources/realtime) is an event protocol with conversation items, response creation/cancellation, audio truncation, session updates, and persistent WebRTC/WebSocket/SIP transport state.

Design lessons:

- realtime is not ordinary generation with `stream = true`;
- interruption operates on a live session and specific response/audio state;
- the connection, event order, transcript, and close behavior belong to `LiveSession`;
- a realtime-only provider should not implement a fake request/response method.

## Anthropic: message batches

Anthropic's [Message Batches guide](https://platform.claude.com/docs/en/build-with-claude/batch-processing) submits many independent Messages requests, exposes processing status and cancellation, and returns results that may not match input order. The guide requires a caller `custom_id` to correlate results.

Design lessons:

- batch processing is distinct from one background response;
- each item needs caller-stable identity;
- the batch owns aggregate status, cancellation, and result iteration;
- success/error/expired/cancelled are item states, not one nullable result;
- a `Batch<T>` resource should expose unordered keyed results.

This motivates separate `Background` and `Batching` capabilities.

## Anthropic: prompt caching

Anthropic's [Prompt caching guide](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) distinguishes automatic caching from explicit cache breakpoints and describes prefix matching and cache lifetime.

Design lessons:

- implicit/automatic caching is an option plus response metadata, not necessarily a first-class resource;
- an explicit provider-managed cache with create/update/delete lifecycle may justify `ManagedCache`;
- cache behavior depends on structured prompt boundaries, so flattening `PromptAst` to text loses useful semantics;
- cache controls may be provider-specific request options even when `Generate` is unchanged.

## Gemini: implicit and explicit context caching

Google's [Gemini context caching guide](https://ai.google.dev/gemini-api/docs/caching) documents implicit caching for newer models and distinguishes it from explicit managed cache APIs available on other Gemini API surfaces.

Design lessons:

- one provider family may expose both transparent and resource-style caching;
- provider/model descriptors should report support without turning every cache behavior into a capability;
- normalized usage metadata should include cache-hit dimensions where available;
- explicit resources need TTL and cleanup, while implicit caching does not.

## Gemini: managed tools versus custom function calls

Google's [tools guide](https://ai.google.dev/gemini-api/docs/tools) distinguishes built-in tools executed on Google's servers from custom function calls executed by the application. The custom flow returns a function name, arguments, and ID; the application executes it and returns the matching result.

Design lessons:

- “tools” contains at least two execution-ownership models;
- server-managed tools can remain provider configuration on `Generate` when there is no client loop;
- client-executed tools need a `Tools` turn protocol and must preserve call IDs;
- the framework must not force server tools through a local dispatcher;
- parallel/non-blocking tool scheduling belongs to the loop/session policy, not the base provider marker.

## Vercel AI SDK: provider wrappers

The AI SDK's [`customProvider`](https://ai-sdk.dev/docs/reference/ai-sdk-core/custom-provider) and model-wrapping APIs demonstrate the ergonomics of mapping aliases, applying default settings, adding middleware, and delegating to a fallback provider without rewriting application tasks.

Design lessons:

- wrappers are the right abstraction for defaults, policy, telemetry, and routing when the operation shape is unchanged;
- applications should select a provider/model object without rewriting the prompt task;
- a custom provider author should implement one semantic generation interface rather than a framework's internal HTTP stages;
- capability forwarding must still be explicit in a more strongly typed system.

## BAML language prior art

The proposal intentionally follows established BAML patterns:

- LLM functions make the return type the structured-output contract.
- Interfaces provide static contracts and runtime dispatch.
- Generic methods carry `T` into provider parsing.
- `prompt` tagged templates create lazy `(Context) -> PromptAst` renderers.
- `PromptAst` preserves roles and media until a provider-neutral message conversion.
- `match` expresses dynamic capability negotiation.
- `cleanup()` gives resources an at-most-once finalizer.
- `spawn`/`await` handle application concurrency without making concurrency a provider feature.
- Ordinary BAML functions are preferred over new DSL when no compiler involvement is necessary.

## Synthesis

The external APIs converge on a small number of durable shapes:

```text
request -> response
request -> stream
request <-> tool turns
request -> job -> polls/result
many keyed requests -> batch -> unordered results
open -> stateful session -> turns/fork/close
open -> live event channel -> interrupt/close
create -> managed cache -> use/delete
```

They do **not** converge on one universal provider call with a bag of flags. BEP-063 therefore standardizes the semantic request passed into these shapes, while keeping the shapes themselves separate capabilities and resources.

The unifying value is `LlmRequest<T>`, not a universal method.
