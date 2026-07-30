# Grounding rule and audit

Every code snippet in a BEP page should correspond to code under
`crates/baml_tests/baml_src_temp2`. The preferred executable examples are the
no-argument functions in `ns_ai_scenarios`.

Run an example with:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.<example>
```

The corpus uses compiler-generated task companions. There is one declarative
model-function kind. It selects its model source with either `provider:` or
`client:` and may declare `prompt:` and `tools:`. `client:` is source-level
sugar: the compiler wraps the selected `baml.llm.Client` as an `ai.Provider`,
then continues through exactly the same lowering used by `provider:`.

Both source spellings get a zero-I/O `F@task(...)` form that captures the
arguments, output type, provider, prompt recipe, default tools, and identity in
`ai.Task<T>`. A direct `F(...)` call runs that same task through the bounded
default `ai.run.Agent`. Backtick and raw Jinja prompts are two prompt syntaxes,
not two function or execution models.

The framework itself is the built-in `ai` package. Scenario and provider code
must use `ai.Task`, `ai.Provider`, `ai.run.Agent`, and the other `ai.*` names;
`root.ai` is not a second framework namespace.

The reserved direct-call override `F(..., $provider = provider)` remains proposed
syntax. The executable equivalent today is
`F@task(...).with_provider(provider)`, followed by an explicit runner. Built-in
`ai` source is production code and contains no `test` declarations; its BAML
behavior tests live in `ns_ai_scenarios`.

## Normative names

| Documentation concept | Corpus contract |
| --- | --- |
| Normal provider | `ai.AgentProvider` |
| One provider model turn | `ai.ModelStep<T>` from `AgentProvider.step` |
| Normal explicit execution | `task.run(runner = ai.run.Agent<T>.new())` |
| Normal result | `ai.Done<T> \| ai.BudgetReached \| ai.Handoff` |
| Retry/fallback | `ai.retry(...)` / `ai.fallback(...)` provider wrappers |
| Exact continuation | `ai.Conversation` |
| Durable continuation | `ai.ResumableAgentProvider` |
| Provider switch | `ai.ConversationImportProvider` |

There is no normal provider-owned model/tool loop. A direct generated call
lowers to a default Agent and unwraps `Done<T>`.

A replay-safe failed provider `step` is transactional: it must leave the
conversation unchanged so retry sees the same pre-attempt state.

## Scenario map

| Guide | Scenario directory or entry point |
| --- | --- |
| Tasks and providers | `ns_ai_scenarios/01_tasks_and_providers` |
| Agents and tools | `ns_ai_scenarios/02_tools_and_agents` |
| Routing and reliability | `ns_ai_scenarios/03_routing_and_reliability` |
| Conversations | `ns_ai_scenarios/04_conversations_and_state` |
| Media and realtime | `ns_ai_scenarios/05_media_and_realtime` |
| Observability and testing | `ns_ai_scenarios/06_observability_and_testing` |
| Production resources | `ns_ai_scenarios/07_production` |
| External harnesses | `ns_ai_scenarios/08_external_harnesses` |
| Stateful memory agent | `ns_ai_scenarios/09_memory_agent` |

Provider request-shape tests live beside their private request builders:

```text
ns_openai/ns_internal/responses/request_tests.baml
ns_anthropic/ns_internal/request_tests.baml
ns_google/ns_internal/request_tests.baml
```

The documentation audit must fail on references to removed normal lifecycle
interfaces or runners. Mentions of another framework's method names in the
comparison page are not BAML API references.
