# /what-is-baml — interactive examples

Standalone brief for building the interactive examples on `/what-is-baml`.
Everything needed is in this file: the page copy as it ships, the agreed section
outline, the UI components required, and the build order.

The page copy is done. The examples are not built. This is about the examples.

---

## 1. The page today

Eight sections at `/what-is-baml`. Each has a card in a 2x3 grid near the top
that links down to its section. Each section body will host one interactive
example explorer.

### Hero

> **Modern programming languages weren't built for agents.**
>
> We built BAML to fight slop. That means no escape hatches (like `as any`).
> Almost everything else feels like TypeScript (unions, generics, lambdas).
> BAML has the:
>
> - syntax and readability of **TypeScript**
> - correctness and tooling of **Rust**
> - compile times and concurrency of **Go**
> - dynamism and ~~nothing else~~ of **Python**
>
> The parts different from TypeScript are: *(the six cards)*
>
> **Incrementally adoptable.** BAML runs standalone on macOS, Linux, Windows, or
> inside your existing projects in Python, TypeScript (Node), TypeScript (WASM),
> Java, C#, .NET, C++, Go, Rust, Kotlin (Android), Swift (iOS). PHP and Ruby
> soon. But not JavaScript. We only support languages that took more than 10
> days to build.
> **Type-safe like OpenAPI, but performant like FFI.**

### Section copy

**1. AI functions**
> Calling a model should feel like calling a function, not wiring up an SDK. In
> BAML it's a typed function: declare the input and output types, get structured
> data back. Malformed output gets repaired against your types by schema-aligned
> parsing, so every model gets better at structured output.

**2. The anti-slop type system**
> Every invariant you can't enforce is one an agent will eventually violate.
> Start with type erasure: we don't do it, so there's no `any` or unchecked cast
> for a model to hide behind. In BAML, invalid states don't compile.

**3. Observability and profiling**
> Observability only pays off in hindsight: if you knew what to trace, you'd have
> traced it already. BAML traces every function by default. 6x faster than OTEL
> in Rust, 200x faster than in Python, and writes traces 1000x smaller, which is
> why it can stay on, even in prod.

**4. Agent-first toolchain**
> For ten years, tooling was built for humans: LSPs, autocomplete, breakpoints,
> hover docs. It's about time the real author of the code got fair treatment. An
> LSP for you, `baml describe` and friends for them. And a really fast compiler
> for both.

**5. Workflow primitives**
> How has everyone accepted async as a good idea? Every agent framework is trying
> to do concurrency on top of languages that lack it. Like Go, BAML has green
> threads. Unlike Go, you can await what they return.

**6. Better testing**
> Engineers spent twenty years squashing flaky tests. Then models made every test
> flaky by definition. BAML tests can grade distributions. Cases can be
> hard-coded examples, golden datasets, or real prod traces. Yesterday's outage
> becomes today's regression test.

**7. Incremental adoption**
> We're not going to pretend you should rewrite your codebase in BAML. That's how
> working systems become broken ones. You can write a whole app in BAML if you
> want. But we went the extra mile in the other direction: every type, every
> function, every method crosses the bridge to your language. Even generics. Even
> lambdas. Sh*t just works.

**8. Making money?**
> Yes please. BAML is and always will be open: Apache-2, free, no internet
> required. The Boundary Cloud starts with observability, but when you create the
> language, the runtime, and the tracing layer, you can build things nobody else
> can. And we think you'll love paying for some of them. The language took two
> years to build, the cloud needs about three more months.

---

## 2. Section outline

The agreed structure. Tab labels are short by design: the problem framing lives
in the description line inside the example header, which has room for a
sentence.

### §1 AI Functions

**AI Functions · Heal JSON · Stream · Switch Models · Agents**

- **AI Functions** — call models like typed functions, not SDK wiring.
- **Heal JSON** — repair malformed, missing, or incorrectly typed model output.
- **Stream** — receive typed partial objects, not raw tokens.
- **Switch Models** — change providers/models without rewriting the function.
- **Agents** — tool calling, handoffs, voice, and related agent behaviors.

### §2 Anti-slop Type System

**No Any · switch < match · Typed Errors · Local Reasoning**

- **No Any** — no unchecked casts or escape hatches.
- **switch < match** — exhaustive matching; missing cases fail at compile time.
- **Typed Errors** — errors are part of function signatures.
- **Local Reasoning** — fully qualified names eliminate import/alias
  reconstruction.
  - `root.*` — current package
  - `baml.*` — standard library
  - `some_name.*` — external package

### §3 Observability

**OTEL < BAML · Enrichment · Agents Query Traces**

- **OTEL < BAML** — sampled/dropped events versus complete BAML traces.
- **Enrichment** — capture inputs, outputs, errors, and structured values.
- **Agents Query Traces** — agents inspect traces directly instead of relying on
  dashboards.

### §4 Agent-first Toolchain

A comparison gallery, not six flat tabs.

**Compile Faster · Build Cheaper · Search Better · Run Directly · Ship Anywhere · Stay Current**

- **Compile Faster** — equivalent programs in Rust, Go, TypeScript, and BAML
  compiled side by side.
- **Build Cheaper** — human versus agent compiler errors, with token counts.
- **Search Better** — `baml describe` versus a grep/read/search loop.
- **Run Directly** — `baml run` versus writing a scratch harness.
- **Ship Anywhere** — cross-compile every target from any host; 14 MB binary
  versus Bun's roughly 64 MB.
- **Stay Current** — polling/checking versus updates pushed into context.

Each panel shows the actual comparison, not just a claim.

### §5 Workflow Primitives

**No Function Coloring · Ambient Cancellation · Bounded Concurrency · Composable Policies**

These have nested proof scenes.

- **No Function Coloring** — `spawn` and `await` are expressions; BAML workflow
  functions do not become `async`-colored.
- **Ambient Cancellation** — cancellation arrives at `await` points without
  passing `ctx` or `AbortSignal` through every function.
- **Bounded Concurrency** — named spawns, `TaskGroup.new(limit)`, FIFO queueing,
  and cancellation of active and queued work.
- **Composable Policies** — `withRetry`, timeout, timing, rate limiting, and
  future combinators like `all`, `race`, `any`.

**Caveat that must survive into the UI:** the "no async coloring" claim is about
**BAML source specifically**. Generated host SDKs still expose sync and `_async`
entrypoints. If a demo shows a host-language caller, that distinction has to be
visible or the demo overclaims. Also: the demos show bounded task groups, not an
explicit thread-pool reuse API.

### §6 Better Testing

**Tests as Code · Generate Cases · Handle Flakiness**

- **Tests as Code** — imperative tests, assertions, normal control flow, and LLM
  judges as ordinary AI functions.
- **Generate Cases** — inline data, CSVs, production traces, and synthetic cases
  generated by AI functions.
- **Handle Flakiness** — `Quorum(5, 3)` and `PassRate(0.7)` measure
  nondeterministic quality; `Retry(3)` handles transient infrastructure failures.

Core message:

> One model run is a sample, not a verdict.

### §7 Incremental Adoption

A two-axis bridge explorer.

- **Left:** BAML code plus a feature selector — Function, Type, Error, Method,
  Generic, Lambda.
- **Right:** host-language selector plus the generated interface and usage —
  Python, TypeScript, Go, Rust, Java, C#, C++, Kotlin, Swift.

One BAML definition visibly crossing into different languages and language
features.

### §8 Making Money?

No interactive example. Copy and CTA only: Apache-2, free and usable offline,
Boundary Cloud as the commercial layer, observability and infrastructure built
around the language/runtime/tracing stack.

---

## 3. The shape of the work

**Comparison is the dominant pattern, not code-and-run.** Counting the outline:

- §2 — all four are two-sided (TS lies vs BAML refuses; switch vs match; untyped
  vs typed errors; import reconstruction vs qualified names)
- §4 — all six, explicitly
- §3.1 — OTEL versus BAML
- §5.1 — colored async versus BAML
- §1 Heal JSON — broken output versus repaired object
- §6 Handle Flakiness — one run versus a distribution

That is roughly **13 of 26 examples**. The comparison pane is the workhorse and
should be built first and built well, not treated as a variant of a code block.

The `<` in `switch < match` and `OTEL < BAML` is the visual language for it:
**left is the old way, right is BAML, and the operator between them is the
claim.** Worth carrying into the component itself.

Second pattern: **variants are a toggle, not more tabs.** One example whose
snippet changes by one line (`Heal JSON`: malformed / missing / wrong type;
`Switch Models`: provider list; `Handle Flakiness`: Quorum / PassRate). This is
what keeps the structure flat. §5 is the deliberate exception where nesting is
wanted.

---

## 4. UI components

| Component | Used by | Status |
| --- | --- | --- |
| **Comparison pane** — two sides plus the claim between them. Must hold code\|code, terminal\|terminal, bars, and token counts, not just two code blocks. | ~13 examples across §2, §4, §3.1, §5.1, §1.2, §6.3 | **build first** |
| **Explorer shell** — tab row, header (title + description + Run), swappable body. Needs 2-level nesting for §5's proof scenes. Responsive: tabs scroll or collapse, code pane scrolls inside itself. | every section | **build** |
| **Variant toggle** — segmented control that swaps one line of the snippet. | §1.2, §1.4, §6.2, §6.3, §4.5 | **build** |
| **Two-axis bridge explorer** — feature selector × host-language selector, two synced panes. | §7 only | **build** |
| **Concurrency scene** — cancellation propagating to await points, TaskGroup limit, FIFO queue, cancellation of in-flight and queued work. | §5.2, §5.3 | **build**, hardest |
| **Stream animation** — typed partial object filling in over time. | §1.3 | **build**, small |
| **Distribution dots** — 5 runs, 3 pass. | §6.3 | **build**, small |
| **Run + typed result** — the WASM playground. | §1.1, §1.5, §6.1, §7 | adapt `LivePlayground` |
| **Benchmark bars** | §4.1, §4.5 | adapt `SpawnChart`, `PackChart` |
| **Terminal playback** | §4.3, §4.4, §3.3 | exists (`TermPlay`) |
| **Falling trace events** | §3.1 | exists, see below |

---

## 5. What already exists

**The OTEL/BAML widget is §3.1, already built.**
`app/what-is-baml/_components/lost-events.tsx`, live on the page today. Trace
events fall past the paragraph. Under OTEL most are dropped (red = sampled out,
amber dashed = never instrumented) and hovering reveals only a vague guess
(`sampled out · payments?`). Under BAML every event is traced and readable. A
counter climbs and freezes when you switch to BAML; a second counter in the page
footer never stops (`OTEL would have dropped 1,284 events while you read this
page. You have all of them.`).

Its OTEL/BAML segmented control **is** the comparison control for that example.
It moves inside the shell as-is; nothing needs rebuilding.

**31 verified BAML snippets** in `app/baml-intro/_components/snippets.ts`,
several annotated as `baml check` / `baml test` verified. Adapt these rather than
writing new BAML from scratch:

| Snippet | Useful for |
| --- | --- |
| `BAML_SENTIMENT` | §1 typed LLM function |
| `BAML_IMAGE` | §1 multimodal |
| `BAML_WF_FANOUT`, `BAML_SPAWN`, `BAML_SPAWN_ADV` | §5 spawn/await |
| `BAML_WF_TALLY` | §5 workflow graph |
| `TS_LIES`, `BAML_UNKNOWN` | §2 No Any |
| `TS_INSTANCEOF`, `BAML_MATCH` | §2 switch < match |
| `TS_CATCH`, `BAML_UNREACHABLE` | §2 Typed Errors |
| `NS_BAD`, `NS_GOOD` | §2 Local Reasoning |
| `DESCRIBE_EVENTS`, `LS_EVENTS`, `GREP_EVENTS` | §4 Search Better |
| `RUN_FN_EVENTS`, `RUN_E_EVENTS` | §4 Run Directly |
| `PACK_EVENTS`, `PACK_BENCH`, `BAML_PACKED` | §4 Ship Anywhere |
| `BAML_CSV_TESTS`, `BAML_HTTP_TESTS` | §6 Generate Cases |
| `BAML_RUNNER` | §6 Handle Flakiness |
| `SPAWN_BENCH` | §4/§5 benchmark bars |

Also available: `WorkflowPlayground` and `LivePlayground` (graph tab) for trace
and graph views.

---

## 6. Build order

1. **Comparison pane + shell + toggle, proven on §2.** Four examples, one shape,
   and the content already exists (`TS_LIES`, `BAML_MATCH`, `BAML_UNREACHABLE`,
   `NS_GOOD`). If §2 renders well, a third of the page is solved.
2. **§7 bridge explorer.** Most distinctive thing on the page. Unlock: run the
   real SDK generator for all nine languages so the right-hand pane is authentic
   output rather than hand-written.
3. **§4 gallery** once the comparison pane is proven. Six comparisons in a
   different layout.
4. **§1 and §6** — playground plus the two small visuals (stream, distribution).
5. **§5 concurrency scenes last.** Most animation work, and BEP-034 needs a
   careful read first so the demo matches real semantics.

---

## 7. Open questions and blockers

- **§4 layout is undefined.** "Comparison gallery, not six flat tabs" — all six
  stacked and scrollable, a 2x3 grid of panels, or tabs with a comparison body?
  This decides part of the shell.
- **§4.1 and §4.2 need measurements someone has to run.** Equivalent programs
  compiled in Rust/Go/TypeScript/BAML, and token counts for human-versus-agent
  compiler errors.
- **Unsourced numbers**: the `go build` comparison, and the observability figures
  (6x vs OTEL in Rust, 200x vs Python, traces 1000x smaller).
- **Live model calls or recorded responses?** Language and runtime examples (§2,
  §4, §5, §6) run fully in WASM with no key. Anything showing model *output*
  (§1, parts of §3) needs a demo key, a proxy, or a canned response.
- **Syntax to confirm against the current toolchain**: streaming, retry policies,
  fallbacks, sandbox/codemode. Greps turn up `retry_policy` and `@stream.done` in
  `.baml` files, but the repo holds both old-BAML and BAML-2 fixtures, so those
  may be legacy.

---

## 8. Working notes

**Snippets can and should be verified.** The `baml` CLI is installed (toolchain
`0.15.1-nightly`). `baml init` a scratch project, write the snippet, run
`baml check`. Everything that ships should pass. Confirmed working:

```baml
type Label = "positive" | "negative" | "neutral";

class Review {
  label: Label,
  reason: string,
}

function classify(text: string) -> Review {
  client: "openai/gpt-4o-mini"
  prompt: `
    Classify this review.
    ${text}
    ${ctx.output_format}
  `
}
```

**BAML runs in WASM**, so examples can genuinely execute in the browser. There is
already an embeddable playground on `/explore` (`LivePlayground`, backed by
`pkg-playground`). This is a real advantage over effect.website, whose examples
are illustrative and do not run.

**Reference material**

- `EXAMPLES_SPEC.md` (this file)
- BEP-023 — test and asserts: `~/Downloads/all-beps 3/BEP-023-test-and-asserts/README.md`
- BEP-034 — concurrency via spawn/await: `~/Downloads/all-beps 3/BEP-034-concurrency-via-spawn-await/README.md`
- effect.website interactive catalog:
  `github.com/Effect-TS/website` → `src/features/visual-effect/catalog`
