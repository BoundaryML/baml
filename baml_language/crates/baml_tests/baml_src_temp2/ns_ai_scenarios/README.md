# User-guide scenario map

The numbered directories mirror `_plan/bepv2/pages/guide-*/` and reuse the same
support-ticket model. Each `.baml` page is independently readable, while common
task constructors and fixtures live in the theme's `00_*.baml` file.

This copy keeps the same scenario names and assertions as `baml_src_temp`, but
explicit lifecycle selection uses nominal values where possible:

```baml
// Function-driven baseline.
let response = root.ai.drivers.generate_with_meta<Resolution>(task)

// Interface-driven comparison.
let response = task.drive(
  root.ai.driver.generate_with_meta<Resolution>(),
)
```

When a task is not the natural receiver, the driver value receives the honest
plural/provider input instead. Methods on an already-open resource remain
methods on that resource.

Live coverage is consolidated so later runs do not make a redundant network
call for every local policy assertion:

- tasks/providers: OpenAI and Anthropic direct calls and structured streaming
- tools/agents: real OpenAI tool calls, dynamic rosters, hooks, events, and handoff
- routing/reliability: real OpenAI retry path and Anthropic fallback target
- conversations/state: real OpenAI transcript serialization; no network is
  needed to test sealing/restoration
- media/realtime: real OpenAI image input, WebSocket realtime text, and tool calls
- observability/testing: the same task matrix over OpenAI and Anthropic
- production: real-provider task paths; resource-wire gaps are D-007
- external harnesses: the harness adapter over both OpenAI and Anthropic

All live declarations start with `integ-test-`. They are compile-checked with
the offline suite and can be selected independently with `baml test -i`; live
execution requires the corresponding provider credentials.
