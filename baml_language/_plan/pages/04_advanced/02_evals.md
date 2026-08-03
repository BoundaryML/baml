# Evals and testing

## What to test where

| Layer | How | Model calls |
|---|---|---|
| Output post-processing | ordinary `test` blocks on literal data | none |
| Tools | call the function directly | none |
| Policies | feed literal events, assert on commands | none |
| The whole loop | scripted client | none |
| Real model behavior | replay a recorded journal, or a live eval | none / live |

The design goal: everything except model quality is testable offline. A
`test` block that calls a real LLM function makes a real request; keep
those in evals, not the unit suite.

Policy testing is covered in `../02_guides/09_policies.md`. This page
covers the layers above it.

## Scripted clients

`Client` is an interface, so a fake provider drives the entire loop —
runner, policy, toolbox, journal — deterministically. The agent under
test:

```baml
function PlanTrip(request: string) -> Itinerary {
    client: "openai/gpt-5.2"
    tools: [search_flights, search_hotels]
    prompt: `
        You are a travel agent. The brief: ${request}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}
```

The fake provider and the test:

```baml
class ScriptedClient {
    turns: Event[][],     // what "the model" does on call 1, 2, 3...
    i: int,
    implements baml.session.Client {
        function id(self) -> string { "scripted" }
        function render(self, j: Journal, tb: Toolbox, output_schema: string) -> ProviderRequest {
            ProviderRequest { url: "", headers: {}, body_json: "" }
        }
        function invoke(self, req: ProviderRequest) -> ProviderResponse {
            ProviderResponse { status: 200, body_json: "" }
        }
        function ingest(self, resp: ProviderResponse) -> Event[] {
            let batch = self.turns.at(self.i) ?? [];
            self.i += 1;
            batch
        }
    }
}

test "agent completes after one tool round-trip" {
    let fake = ScriptedClient { i: 0, turns: [
        [ToolRequested { call_id: "t1", tool: "search_flights", args_json: `{"origin":"SFO","dest":"NRT"}` }],
        [FinalProduced { result_json: `{"destination":"Japan","days":14,"flights":[],"hotel":null,"daily_plan":[]}` }],
    ] };
    let s = PlanTrip@session(request = "2 weeks in Japan")
        with baml.session.options(client = fake);
    match (s.run()) {
        let d: baml.session.Done<Itinerary> => assert.equal(d.result.days, 14),
        _ => assert.is_true(false),
    }
}
```

## Replaying recorded journals

A journal captured from a real run replays without a provider: a replay
client serves the recorded `ingest` batches in order. Any production
session becomes a regression test — real model behavior, zero API calls,
deterministic.

```bash
baml session record --out fixtures/plan_trip.journal   # capture
baml test --replay fixtures/                            # replay suite
```

## Evals

Evals run live and judge journals, not just outputs. The journal records
process — which tools ran, in what order, at what cost — so a judge can
score behavior, not only the final answer:

```baml
function eval_suite(cases: EvalCase[]) -> float {
    let g = baml.spawn.TaskGroup.new(8);
    let scores = await baml.future.all(cases.map((c) -> {
        spawn with baml.spawn.options(group = g) {
            let s = PlanTrip@session(request = c.input);
            let _ = s.run();
            JudgeRun(render_for_judge(s.journal()), c.rubric).score   // an LLM function
        }
    }));
    scores.reduce((a, b) -> { a + b }, 0.0) / (scores.length() * 1.0)
}
```

Useful rubric dimensions the journal makes checkable: tool choice (did it
check hotels before booking), efficiency (steps and tokens per case,
straight from `Usage` events), safety (were gated tools ever requested
without an approval event preceding their run).

Run evals as jobs (`../03_examples/02_background_jobs.md`) when suites
get long; each case's journal is kept for inspection.
