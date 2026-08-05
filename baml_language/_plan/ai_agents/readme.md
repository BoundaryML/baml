# ai_agents — working reference for the AI Functions & Agents BEP

Runnable code for every scenario the BEP docs show. The library is
`root.ai` (`baml_src/ns_ai/`); the demo domain is the docs' travel agent;
each scenario file mirrors one guide page, with tests inline.

```
baml_src/
├── ns_ai/                     the library (the baml.session.* stand-in)
│   ├── events.baml            journal, event types, factories, transcript fold
│   ├── toolbox.baml           tools from reflection; call_any validation
│   ├── client.baml            Client interface; ScriptedClient (fake model)
│   ├── policy.baml            commands, SessionState, ToolLoop, WithSteering, WithRetry, WithBudget
│   ├── session.baml           Session<T>: runner, two lanes, snapshot/resume
│   ├── runner.baml            SessionRunner: the $runner Handle projection (appendix §1)
│   └── openai.baml            live OpenAI client (render/invoke/ingest)
├── shared/                    the travel-agent domain every scenario reuses
│   ├── types.baml  tools.baml  parser.baml  plan_trip.baml
└── scenarios/                 one file per doc topic
    ├── s01_agents.baml        task mode, typed results        → guides/01
    ├── s02_sessions.baml      turns, snapshot/resume          → guides/02
    ├── s03_steering.baml      queued messages, interrupt      → guides/03
    ├── s04_models.baml        provider stamps, client switch  → guides/04
    ├── s05_tools.baml         reflection, validation, closures→ guides/05
    ├── s06_policies.baml      approval gate, middleware       → guides/09
    ├── s07_custom_events.baml send_event, journal folds       → guides/10
    ├── s08_subagents.baml     agent-as-tool, spawn fan-out    → guides/08
    ├── s09_errors.baml        feedback loop, retry, budgets   → advanced/01
    └── s10_runners.baml       $runner picks the handle type   → appendix §1
```

## Run it

```bash
baml check
baml test                                   # 37 offline tests, no model calls
baml test -i '"s06 policies"::*'            # one scenario

infisical run --env=test -- baml run -e 'demo()'    # live OpenAI end-to-end
```

## What simulates what

No language sugar exists yet, so:

- `PlanTrip@session(request = ...)` → `plan_trip_session(request, client)`
- plain `PlanTrip(request)` (task mode) → `plan_trip(request, client)`
- `$policy` / `$client` / `$tools` → configure the session value:
  `s.set_policy(...)`, `s.set_client(...)`, `s.mount(...)` — setters are
  journaled (`ClientChanged` / `PolicyChanged`)
- `$runner = jobs` → `root.ai.open_with(runner, session)`: the runner's
  associated `Handle` type picks the handle (`Session` vs `Job`), the
  appendix §1 typing rule, validated in s10
- `${ctx.transcript}` → `root.ai.render_transcript(journal)` (text fold);
  custom events render only if they implement `root.ai.Promptable` (s07)
- `${ctx.output_format}` + SAP → hand-rolled JSON protocol in
  `shared/parser.baml` (fence-stripping included — the fragility here is
  the argument for the real feature)
- typed custom event unions → fully working: sessions are `Session<T, X>`
  where `X` is the custom-event extension (see s06/s07); `never` = none.
  `s.send(...)` takes `string | X`, one verb for messages and custom
  events, as in the docs. (`Session<T>` with a defaulted `X` needs generic
  parameter defaults, which the language lacks — see reference notes.)

Findings, bugs, and gaps discovered while building this:
`../reference_notes.md`.
