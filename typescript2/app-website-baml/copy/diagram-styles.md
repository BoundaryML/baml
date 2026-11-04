# Diagram styles copy

Copy for the page at `/diagram-styles`: eight dark styles for the BAML site's diagrams, to pick from before the real ones are drawn. Keep this file in sync with `app/diagram-styles` and `components/diagram-styles`.

**Page title:** BAML | Diagram styles

The page is dark only and marked noindex. Each style is a theme in `app/diagram-styles/styles.css` that sets the colors, borders, radii, shadows, and fonts the ten diagrams draw from, and each style shows a board of six diagrams at once. No style uses gradients. All numbers are example data.

# Diagram styles

Eight dark styles for the BAML site's diagrams. Each style shows a board of diagrams at once, built from the same parts, so the styles can be compared side by side. All data is example data.

A row of links jumps to each style.

## Styles

Each style is a full-width band with its name, a note, and its board. Wide diagrams span the board; the rest sit two to a row on wide screens.

- **Midnight.** The current style. Near-black panels, hairline borders, and clover as the only accent. Board: Hub, Agent session, Call grid, Trace waterfall, Deployment, Orbit.
- **Clover.** Set on the brand's darkest clover, with pasture green as the accent. For a section that should look like BAML and no other dark site. Board: Orbit, Call grid, Agent session, Hub, Stack, Fallback flow.
- **Terminal.** One monospace face, square corners, dashed frames, and a line grid. It reads like a tool that lives in your terminal. Board: Deployment, Agent session, Trace waterfall, Code to output, Call grid, Fallback flow.
- **Ink.** Warm charcoal, paper-white type, and a serif for headings and numbers, with hairlines and no shadows. Quiet and exact, like the figures in a research paper. Board: Fallback flow, Stack, Code to output, Dashboard, Call grid, Trace waterfall.
- **Bold outline.** Thick outlines, hard offset shadows in clover, and Geist Pixel numbers. Loud on purpose. Board: Dashboard, Deployment, Agent session, Hub, Orbit, Call grid.
- **Glass.** Frosted panels over the brand's dusk painting. Soft and atmospheric, for a hero or a section opener. Board: Orbit, Agent session, Fallback flow, Hub, Code to output, Call grid.
- **Arcade.** Geist Pixel for numbers and code, square corners, two-pixel borders, and hard shadows, with pasture green as the accent. Board: Deployment, Call grid, Orbit, Trace waterfall, Agent session, Stack.
- **Mono.** Black and white only. Red is the one color, and only for failure. The plainest of the set. Board: Hub, Deployment, Fallback flow, Dashboard, Agent session, Trace waterfall.

## Diagrams

The ten diagrams the boards are built from. Their words are the same in every style.

### Hub

BAML between your code and the models it calls. Calls run from each language through one function to each provider.

Diagram: "Your code" on the left (Python, TypeScript, Go), the BAML lamb in the center captioned `classify(text) -> Verdict`, and "Any model" on the right (OpenAI, Anthropic, Gemini, Bedrock). Beams run along the wires.

### Code to output

A BAML file next to what it returns. Each field in the class lights up with the value it parsed to.

Diagram: a `triage.baml` card with `type Label`, `class Verdict` (label, confidence, reason), and `function classify`. An "Output" card for the Verdict shows label `billing`, confidence 0.94, and reason "Third refund request". The fields light up in turn, joined to their line in the code by a wire on wide screens.

### Trace waterfall

One request as a trace, span by span, with the model call opened to show its input, output, and tokens.

Diagram: "Trace 7f3a2c" with chips for 6 spans, 2.41s, and $0.0042. Spans: triage, plan, classify (gpt-4o), parse Verdict, reply (claude), parse string. The bars draw in, then the classify span opens: 812ms, gpt-4o. Input "Third time asking. Refund me." Output, Verdict, `{ label: "billing", confidence: 0.94 }`. 412 in, 18 out tokens. Before it opens, the panel reads "Select a span to see its input and output."

### Deployment

A change going through check, test, pack, and query, drawn like a deployment page with its log.

Diagram: "triage" on `main`, commit `a41f9c2` "Read sarcasm as what the customer means", and a status that goes from Running to Live. Steps: Build (`baml check`, 0.8s), Test (`baml test classify`, 6.2s), Deploy (`baml pack triage`, 3.1s), Monitor (`baml query`, Live). The log shows the running step: "Checked 4 files / No errors", "9 tests / 9 passed, 0 failed", "Packed triage v43 / Released to production", "1,204 calls since v43 / 0 sarcastic tickets labelled other".

### Agent session

A coding agent fixing a bug in production with BAML's commands: it queries, edits, tests, and reports.

Diagram: the user asks "Sarcastic tickets are going to the wrong queue. Can you fix it?" Tool calls appear one at a time:

- Ran baml query, 14 rows. `SELECT input FROM spans WHERE output_value['label'] = 'other'`
- Edited triage.baml, +1 line. `+ Read sarcasm as what the customer means.`
- Ran baml test classify, 9 passed. `sarcastic_ticket, and 8 more`

Then the agent replies: "Fixed. 14 sarcastic tickets were labelled other in production. I added one line to the prompt and a test for them, and all 9 tests pass." The input below reads "Ask your agent".

### Dashboard

What BAML records about one function, as dashboard tiles that update as calls come in.

Diagram: four tiles. "Calls, last hour" for `classify`, with a count and a line chart. "p95 latency" with a histogram. "Labels returned" with the share of billing, bug, and other. "Errors, grouped by cause" since v42: `Verdict.confidence missing`, `Timeout on gpt-4o`, `Label not in Label`.

### Call grid

A day of calls as dots. Failures build until a deploy, then stop.

Diagram: "`classify`, calls in the last 24 hours" with a Passed and Failed legend. The grid fills left to right, a line marks "v43 deployed", and the dots after it are clover. Below: "Failed before the fix" with a count, and "Failed since v43", 0.

### Fallback flow

Where an hour of calls went: the primary model, the fallback after a timeout, and the few that failed.

Diagram: "`classify`, where calls went in the last hour", with "fallback: gpt-4o, then claude-sonnet". Bands run from classify (12,480) to gpt-4o (96.0%), claude-sonnet (3.6%), and Failed (0.4%), and the answered calls join at Verdict (99.6%). Below: gpt-4o 11,981, answered first time. claude-sonnet 449, answered after a timeout. Failed 50, both models errored.

### Stack

The layers from your agent down to Boundary. A call goes down through them and its trace comes back up.

Diagram: a label reads "Call going down" or "Trace coming back" as a marker moves along the spine.

- **Your agent.** Calls BAML functions like any other function. triage, support-bot, nightly evals.
- **BAML language.** Prompts as typed functions, with their tests beside them. Types, Functions, Prompts, Tests.
- **Toolchain.** Checks, tests, ships, and queries what you wrote. `baml check`, `baml test`, `baml pack`, `baml query`.
- **Runtime.** One runtime under every language you call it from. Python, TypeScript, Go, Rust.
- **Boundary.** Every call recorded, so you can see and query production. Traces, Dashboards, Alerts.

### Orbit

BAML at the center, the languages that call it on the inner ring and the providers it calls on the outer.

Diagram: the lamb in the center. The inner ring holds Python, TypeScript, Go, and Rust. The outer ring holds OpenAI, Anthropic, Gemini, Bedrock, Azure, Vertex, Ollama, and OpenRouter. The rings turn slowly in opposite directions.
