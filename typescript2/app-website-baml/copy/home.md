# BAML homepage copy

## v3

Only the hero is built, at `/v3`, which is marked noindex. The rest of this section is draft copy. It follows the story from sales calls, discussed in the Oct 30 website review: languages keep moving up the stack, and BAML moves into production. The headings and code are written to tell that story to someone who skips the paragraphs.

**Page title:** BAML | The programming language for agents (v3)

### Hero

# The programming language for agents

BAML lets agents close the development loop. Build, test, trace, query, and run experiments with a unified, vertically integrated toolchain.

**Install BAML** · Explore BAML

The hero has eight versions to compare, the shortlist from the earlier rounds. The floating arrows, the arrow keys, and `?diagram=1` to `?diagram=8` in the URL switch between them. The page opens on version 2. The nav's section links still go to `/v2`. All data is example data.

1. **Languages and BAML.** How far each toolchain reaches across Build, Test, Deploy, and Monitor. **C:** `cc`. **Go:** `go build` and `go test`, with `pprof, sampled` under Monitor. **Rust:** `cargo build`, `cargo test`, and `cargo publish`, with "Separate products" under Monitor. **BAML:** `baml check`, `baml test`, `baml pack`, and `baml query`, with a line back to the start labelled "Improve".
2. **Tracker, by language.** Drawn with the parts from `/diagram-styles`, so it has the same style button as versions 4 and 5 (described below). The loop as a delivery tracker, drawn as a product screen with seven stages (Build, Test, Trace, Query, Errors, Dashboards, and Alerts) and headed "Where each toolchain reaches: one change to a classify function, from build to production". One deploy line runs through all the rows.
   - Python (`python`, `pytest`), TypeScript (`tsc`, `vitest`), and Go (`go build`, `go test`) fill Build and Test, then stop at the line. Past it, each row reads "5 separate products, wired by hand".
   - BAML fills all seven stages and runs back to Build.
   - Legend: "In the toolchain" and "Separate products".

Version 3 is a lifeline: a function's life drawn as a line through Written, Checked, Tested, and Deployed, with a dashed rule at deploy. Other toolchains' lines fade after deploy. BAML's goes on in purple, and calls land on it as small ticks, a few in red. It shows the whole comparison at once.

3. **Lifeline, integrations.** The top line is labelled "Python, TypeScript, Go". After deploy it breaks into five short separate pieces: Tracing, Errors, Metrics, Analytics, and Alerts, under "5 integrations, wired by hand". On a phone the five labels are hidden and the header reads "5 integrations".

Versions 4 and 5 are also drawn with the parts from `/diagram-styles`, so each can be shown in any of that board's eight styles (Midnight, Clover, Terminal, Ink, Bold outline, Glass, Arcade, and Mono) or in No boxes, which has no frame, cards, or outlined cells, only the text, the deploy line, and the line back to build. These two and version 2 sit higher on the page than the other versions, so the production panel shows without scrolling. A style button in the floating switcher shows the current style and moves to the next one (shift-click goes back); the up and down arrow keys and `?style=ink` in the URL also pick one. Deploy is a dashed vertical line labelled "deploy", not a column: Build and Test sit to its left and "Production" to its right. TypeScript (`tsc`, `vitest`), Go (`go build`, `go test`), and Python (`mypy`, `pytest`) cover Build and Test. Past the line, one dashed cell spanning all three rows reads "Separate products". On a phone TypeScript is shortened to "TS". BAML's row (`baml check`, `baml test`) crosses the line into a production panel that starts with the command it shows the output of, and a line with a travelling beam runs from the panel back to Build and Test, labelled "Improve". Each variation fills the panel differently.

4. **Split, query.** A taller panel drawn as a terminal at `~/triage`, with four dots for the four commands it runs in turn. Each command types out, runs, and shows its result; then the next one starts. It opens on the live chart. Long SQL is cut off at the edge of the terminal.
    - `baml query --watch "SELECT date_trunc('minute', start_time) AS minute, count(*) FROM spans WHERE span_name = 'classify' GROUP BY minute"`: a live chart. Calls per minute keep arriving, the newest bar fills on the right while the chart slides left, and a few bars are red. Under it: "40 min ago", "calls per minute", and a blinking "now".
    - `baml query "SELECT output_value AS label, count(*) FROM spans WHERE span_name = 'classify' GROUP BY label"`: bars for billing 412, bug 318, and other 31 in red.
    - `baml query "SELECT version, quantile(duration_ms, 0.5) AS p50 FROM spans WHERE span_name = 'classify' GROUP BY version"`: bars for v41 1.61s, v42 1.42s, and v43 1.18s.
    - `baml alerts --list`: "classify errors above 2%" firing on 31 calls in red, then "triage p95 above 5s" and "cost per call above $0.01" marked ok.
5. **Split, agent chat.** The production panel is a terminal chat with an agent, headed "agent" and "~/triage". New turns appear at the bottom and older ones scroll up out of view, and with each turn the panel grows a little upward, over the bottom of the "Separate products" cell. The chat runs for about fifteen seconds, then starts over:
    - "> Tickets are landing in the wrong queue. Can you look?"
    - `$ baml query "SELECT output_value['label'] AS label, count(*) FROM spans …"` types out and returns bars: billing 412, bug 318, and other 31 in red.
    - "31 sarcastic tickets were labelled other since v42."
    - `$ baml alerts --list` returns the alert list from 4.
    - `$ baml query "SELECT date_trunc('minute', start_time) …"` returns a bar chart of calls per minute.
    - "Added a prompt rule for sarcasm and 31 tests from these calls."
    - `$ baml test classify` returns "40 passed, 0 failed".
6. **Tabs, file and terminal.** This version draws the whole hero, so its headline, subhead, and buttons differ from the shared ones above. The headline and buttons sit on the left, and the demo on the right; on a phone the demo goes under the buttons.
    - Headline: "The programming language for agents"
    - Subhead: "BAML records every call your code makes in production. You and your agent can watch it live, read a trace, query the calls, and set alerts, all from the terminal."
    - Buttons: "Install BAML" (to the quickstart) and "Build it with your agent" (to the quickstart's agent step).
    - The demo is `triage.baml` (the sample code below) with a terminal titled "~/triage" over its lower corner. Above the file are four tabs, Dashboard, Traces, Query, and Alerts. They advance every six seconds, with a line under the current tab filling as it runs, and a click picks one. Each tab types its command into the terminal, and as each row of output appears, the code it is about gets a yellow highlight (red for a problem):
        - **Dashboard** marks `route`. `$ baml query --watch "SELECT strftime('%H:%M', start_time) AS minute, count(*) AS calls, median(duration) AS p50 …"` (cut off at the edge) shows three tiles, Calls, p50, and Failed, over a bar chart of calls per minute. Every two seconds the chart moves one minute along, the tiles update, and "Watching. Refreshed 14:32:04" counts up.
        - **Traces** queries one call by its `process_id`. It shows the ticket ("Export to CSV has been broken since Monday."), then a waterfall: `route` 1,342ms, `classify` 1,318ms, and `openai/gpt-6-luna` 1,296ms, then "returned "engineering"". Each row lights up its line: `route`, the `classify(ticket)` call, the `"openai/gpt-6-luna"` client, and `"engineering"`.
        - **Query** marks `classify`. `$ baml query "SELECT output_value AS label, count(*) AS calls FROM spans …"` returns label and calls with a bar each: billing 412, bug 318, and other 31 in red. As each row lands, its literal in `Label` lights up with the count after it ("other" in red). On a phone the counts in the code are hidden.
        - **Alerts.** `$ baml alerts` lists `other_labels` on `classify`, "other above 2%", now 4.1%, firing in red; and `slow_route` on `route`, "p95 above 3s", now 1.3s, in green. Then "other_labels is firing. 31 of 761 calls in the last hour were labelled other." In the code, `classify` gets a red alarm clock with a pulse, `route` a green one, and `"other"` a red highlight.
    - A "Pause motion" button under the demo stops the tabs and shows each tab's finished output; it then reads "Play motion". Under reduced motion it starts paused.
7. **Video, code lights up.** An autoplaying video of BAML, drawn live in the page rather than recorded, that loops. Two windows sit side by side and hold still: an editor titled "triage.baml" with the sample code below, and one terminal titled "~/triage". The windows are as tall as the file, and the terminal's lines start at the top, beside the code they highlight; once the terminal is full, the oldest lines scroll off the top. There are no captions. Under the frame the chapters fill in as the video plays; clicking one jumps to it. It runs for 38 seconds. Each query highlights the code it is about in yellow, with a small count after it. Chapters are Calls, Labels, Routes, Alert, and Fix. Under reduced motion it holds while the agent queries the calls labelled other.
    - **Calls.** `$ baml query --watch "SELECT span_name, date_trunc('minute', start_time) AS minute, count(*) AS calls …"` shows a live chart of calls per minute: the newest bar fills on the right while the chart slides left, and a few bars are red. Under it are "30 min ago", "calls per minute", and a pulsing "watching", then route and classify with their calls and p50. Each function's first line gets its count and p50 at the end ("1,204 calls  1.4s"), and the counts there and in the table go up as the bars arrive. When the next command starts, the chart stops and reads "stopped".
    - **Labels.** The label query returns bars, and the `Label` type gets "billing" 855, "bug" 318, and "other" 31 in red on its literals.
    - **Routes.** `$ baml query "SELECT output_value AS route, count(*) … 'route' GROUP BY route"` returns support 74% and engineering 26%, and each `return` in `route` gets its share.
    - **Alert.** `$ baml alerts` prints a red alarm, "firing", "classify", and "other is 3.1% of calls, above 2%". In the editor a red alarm clock appears in the gutter next to `function classify`, pinging, with "other 3.1%, above 2%" at the end of the line.
    - **Fix.** `$ agent`, then "Fix the alert on classify." Its `baml query` for the other tickets returns three sarcastic ones, and while it reads them the prompt and "other" are highlighted. It adds the sarcasm line to the prompt. From then on that line is marked as a diff: the whole line is green, with a green bar along its left edge and a "+" in the gutter, and it stays that way under any other highlight until the video loops. Then `baml test classify` returns "40 passed, 0 failed", and `baml pack route` builds the executable. After "deployed, a day later", `baml alerts` prints a green "resolved" for classify at 0.3%, the alarm in the gutter turns green with "other 0.3%, resolved", and the agent says "The alert on classify resolved. Calls labelled other fell from 3.1% to 0.3%."
8. **Queries pick calls.** Production calls stream out of a file while a terminal like the one in 4 queries them. On the left is `triage.baml` with three functions: `classify` as in the sample, `reply(ticket: string, label: Label) -> string` on `"openai/gpt-6.1-sol"` with the prompt "Write a short reply to this ${label} ticket.", and `handle`, which returns "Sent to engineering." for a bug and calls `reply` otherwise. Production calls slide out from under the file and cross the stream slowly (about eleven seconds), all at the same speed, each on its own lane, so they never change lanes or overlap. Spans are clover: a faint fill under a fine outline. Calls that failed are red from the start. Everything is flat, with square corners and no blur or trails. On the right, a terminal at `~/triage` runs four `baml query --watch "SELECT * FROM spans WHERE …"` commands in turn, seven and a half seconds each, with a dot for each. Each query lights up the code it is about. As matching calls pass, each one turns solid with an outline around it, then flies down. Each query adds a widget under the terminal whose count goes up as each one lands. The terminal shows the same count as "{n} rows watching".
    - **Labelled other** (red), from 31: `output_value = 'other'` on `classify`. Marks `classify` and `"other"` in red.
    - **Labelled bug** (yellow), from 318: `output_value = 'bug'`. Marks `"bug"` in `Label` and in the `if`, and `"Sent to engineering."`.
    - **Replies over 3s** (yellow), from 12: `span_name = 'reply' AND duration > 3e9`. Marks `reply`, `"openai/gpt-6.1-sol"`, and the `reply(ticket, label)` call. With spans, these are the long ones.
    - **Failed calls** (red), from 4: `status != 'return'`. Marks the `Label` return type of `classify` in red. These are the calls already drawn in red.
    - The version menu has one more button for this version. **Shape** switches between Spans (outlined rectangles as wide as the call took), Dots (round, solid clover), and Pixels (small solid squares). `?shape=dot` in the URL picks one too.
    - After the fourth query the widgets clear and it starts again. On a phone the file, terminal, stream, and widgets stack, and the widgets sit two to a row. Under reduced motion all four widgets show.

The versions cut from the shortlist are still in `components/v3`, unused, until one is chosen.

Sample code for later sections:

`triage.baml`

```baml
type Label = "billing" | "bug" | "other";

function classify(ticket: string) -> Label {
    client: "openai/gpt-6-luna"
    prompt: `
        Classify this support ticket.
        ${ticket}
    `
}

function route(ticket: string) -> string {
    let label = classify(ticket);
    if (label == "bug") {
        return "engineering";
    }
    return "support";
}
```

```
$ baml query "SELECT output_value AS label, count(*) AS calls
              FROM spans WHERE span_name = 'classify' GROUP BY label"
```



### Used by engineers at

Stitch Fix, Feather, SAP, AMD, EY, Akamai, and Vetcove.

### Languages keep moving up the stack.

Each generation of languages has absorbed more of the work around your code. Go added formatting and testing. Rust added package management and publishing, and tools such as uv and Bun brought that experience to other languages. BAML extends the toolchain into production.

Hero-sized diagram, using this table as its data:


| Stage                  | C (1972) | Go (2009)        | Rust (2015)              | **BAML**                       |
| ---------------------- | -------- | ---------------- | ------------------------ | ------------------------------ |
| Compile                | ✓        | ✓                | ✓                        | `baml check`                   |
| Format                 |          | `gofmt`          | `rustfmt`                | `baml fmt`                     |
| Test                   |          | `go test`        | `cargo test`             | `baml test`                    |
| Build                  |          | `go build`       | `cargo build`            | `baml pack`                    |
| Dependencies           |          | `go mod`         | `cargo`, `cargo publish` | Coming soon                    |
| Production profiling   |          | `pprof`, sampled |                          | **Exact counts per call path** |
| A record of every call |          |                  |                          | **On by default**              |
| SQL over production    |          |                  |                          | `baml query`                   |




### Production still takes a separate tool for every job.

Most teams use one product for errors, another for metrics, and another for product data, and they maintain their own OpenTelemetry instrumentation to feed them. Each tool sees your program from the outside, so it doesn't know your types or your code paths.

### Agents made production data part of writing code.

Teams used to invest in telemetry once they had enough traffic and enough engineers to justify it. Now a coding agent writes much of your code, and you judge that code by what it does in production. Every team needs that data, and agents need to read it to fix what they shipped.

### BAML connects development and production.

You write your application logic, and the toolchain handles the recording. The same query works on your laptop and, with a Boundary key, over production.

- **Recorded by the runtime.** BAML records each call's typed arguments and result, the step that failed, the call path, the code version, and any user or metadata you attach.
- **Queryable without setup.** You don't have to decide what to make queryable, or ask your agent to add it.
- **One CLI for people and agents.** A coding agent can find code with `baml describe`, test a change with `baml test`, and check the results with `baml query`.

<Feature flags, A/B tests, and adding logs at runtime belong here once they ship, or labelled as planned.>

### One function, from your editor to production.

The same walkthrough as `/v2`, with real code in every step.

### Adopt it one function at a time.

BAML runs inside Python, Node, Rust, or on the web. You don't need to rewrite your app in BAML, and you don't need a separate container.

### Build anything.

The same six examples as `/v2`, with each card showing its code.

### Boundary runs the same toolchain in production.

Send your recordings to Boundary, hosted for you or in your own AWS account. Your team and your agents can run `baml query` over live traffic, and build dashboards and alerts on the results.

### Before you install.

The same questions as `/v2`, with these added after "Is BAML production ready?":

**Can I add OpenTelemetry myself instead?**
Yes. You can also write your own package manager or test runner. BAML includes recording in the toolchain, so you don't write or maintain instrumentation, and your queries can use your types.

**What does recording cost?**


### Start using BAML.

The same as `/v2`.

### v3 open questions

- **Which site tells this story?** The review didn't settle this. One suggestion put it on boundaryml.com and the language features on this site. This draft puts it here, with code on the page.
- **Can the hero claim "faster" and "run experiments"?** No speed comparison has been measured, and `baml experiment start` isn't shipped.- **Should the page name competitors?** Sales calls name Sentry, Datadog, and PostHog. This draft doesn't.
- **Do we claim "first"?** The sales story says BAML is the first language to solve production. This draft doesn't, because of the accuracy rule below, and because Go can profile production.
- **Does the missing package manager hurt the diagram?** BAML has a gap in a row that Go and Rust fill, and the table shows it as "Coming soon".
- **What is the status of feature flags, A/B tests, and runtime logs?** A name search of `baml_language/crates` found no feature flag, A/B test, or log injection code, so the page doesn't claim them.

---

`/v1` keeps the first version of this page, with its copy frozen in `v1.md`. This file is the working copy for `/`.

Working copy for `typescript2/app-website-baml`, route `/`. Edit this file first, then move the copy into the components. Links are as wired today.

## On the site now

The page at `/` follows the outline from "Hero" through "Start using BAML" below. Everything after the `---` lines is parked and not on the page, except where noted here. The GitHub stats, "Your agent drives the toolchain", and "Open source and pre-1.0" sections are off the homepage.

- **Hero diagram:** a four-step row under the hero. Your program, recorded, `baml query`, anything.
- **Designed for observability:** uses the C, Go, Rust, and BAML table and the three points from the parked draft under "BAML builds the richest telemetry". The "Toolchain" nav link now goes here.
- **What's in the language:** uses the draft 3 feature list.
- **Before you install:** the five questions from the outline, with answers from draft 3. The export answer says OpenTelemetry export is in progress.
- **Hero wording:** keeps "builds a database of your program's executions", as written in the outline.



## Draft 3 notes

This draft rewrites the prose in the style of Linear's and Stripe's homepages. Headings describe the product in one plain phrase. Section intros are one or two sentences that start with what you can do. Feature lists use a short bold title followed by one sentence.

Style rules for this page:

- No em dashes. Use a period, a comma, or parentheses instead.
- No rhetorical setups or punchlines, such as "X. Y is next.", "BAML does.", "nobody calls it bloat", or "What an agent can't invent is…".
- No two-beat headings made of two short sentences.
- No "no X, no Y, no Z" lists. Say what the product does.
- Use the serial comma, as Linear and Stripe do.
- Use sentence case, and no exclamation marks.

Accuracy rules, carried over from draft 2 and `app-why-baml/LANDING-BRIEF.md`:

- Don't say "every language stops at deploy". Go and Java can profile production. The page says that no language records what a function decided.
- Don't say "first".
- Say "no tracing SDK", not "no SDK". You still install a generated client, and host functions use `@trace.instrument`.
- Give both overhead numbers: about 24 ns a call for timing and profiles, and about 60 ns with captured values.
- Say "code version", not "git version". It is the revision of the BAML source each run used.
- The example queries use the `spans` table that `baml query` reads. The first ran against real recordings with BAML nightly 0.20.2. The per-customer query reads `total_usage`, which is newer than that nightly, so it hasn't been run. It reads only the first provider and model entry, so it assumes one model per call.

Boundary is described as accepting OpenTelemetry traces, with BAML as the way to record more. Boundary doesn't ingest OpenTelemetry yet, so this has to ship before the copy goes live.

The footer keeps the brand line "Build with BAML. Grow with Boundary."

Open question:

- Are 24 ns and 60 ns still the measured costs in the current runtime?

**Page title:** BAML | The scripting language for AI workflows
**Meta description:** Write prompts, tools, and agents as typed functions, and call them from the code you already have. BAML records every call, and `baml query` reads the records with SQL.

---



## Nav

- [Language](/#features-title)
- [Toolchain](/#toolchain-title)
- [Integrations](/#stack-title)
- [Boundary](/#boundary-title)

[GitHub](https://github.com/boundaryml/baml) · **[Install BAML](/quickstart)**

---



## Hero



## The programming language for agents

BAML is a language for writing AI agents, with efficient built-in telemetry. BAML builds a database of your program's executions that you can query locally (or in the cloud) to build anything -- evals, cost alerts, error-tracking, and more.

## < some sort of diagram to showcase this>

<show some code, ppl want to see code>

### Used by engineers at:



## Designed for observability

Each generation of languages added more capabilities into their toolchains. A unified toolchain lets language designers build more efficient programs, and a better experience.

< maybe show the other langs here? >

BAML builds the richest telemetry

## Whats in the language (same thing as bfore)



## Incrementally adoptable

BAML is designed to be runnable anywhere -- inside Python, Node, Rust or on the Web.

That means you don't need to write your whole app in BAML. And you don't need a separate container.

<show the same content of the baml site we currently have that says 'one function, startt to finish'>

## Write it once. Run anywhere.



## Query locally, or in the cloud

<here is where we introduce boundary, similar stuff as the 'baml is free. boundary is where its calls go'.>

## Before you install

1. Is BAML production ready? Yes. BAML is in beta, which means the language and standard library may still change, but it's stable for production use. It doesn't have a package manager yet, and we plan to add one soon.
2. Do i have to rewrite my app in baml
3. What does baml cost
4. Which models does it work with
5. Where does my data go
6. Can I export telemetry elsewhere? Yes, we are working on OTEL telemetry export 



## Start using BAML



---

---

---

---

---

BAML vs OTEL.

BAML helps agents ship faster by helping them query program data *after* it's deployed.

<diagram of langs keep absorbing tools, baml's is telemetry + baml query>

[note: maybe like something along lines of we not only build lang but also a DB? or like baml helps you not only with traces but data layout, query performance, etc]

## Build

Used by engineers at: ...

---

--

# The scripting language for AI workflows.

Write prompts, tools, and agents as typed functions, and call them from Python, TypeScript, Go, and more. The runtime records every call, so there is no tracing SDK to install.

**[Install BAML](/quickstart)** · [Explore BAML](/#features-title)

Install panel, tabs **for agents** and **for humans**, with a **Copy prompt** button:

*for agents*

```
Set up BAML in this repo, then write and run a function.
1. brew install baml
2. baml init
3. baml agent install
4. baml run main
```

*for humans*

```
brew install baml
# or: curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
baml init
baml agent install
baml run main
```

Editor (files labelled `classify.baml`, `main.py`, and `app.ts`, with a **▶ Run** button):

```baml
type Label = "positive" | "negative" | "neutral";

class Verdict {
  label: Label,
  confidence: float,
}

function classify(text: string) -> Verdict {
  client: "openai/gpt-6-luna"
  prompt: `
    Classify the sentiment of the text. Sarcasm counts
    as the sentiment actually expressed.
    Text: ${text}
    ${ctx.output_format()}
  `
}
```

Run output:

```
$ baml run classify -- --text "Oh great, another Monday."
Verdict { label: "negative", confidence: 0.94 }
```

---



## Logos

Used by engineers at

Stitch Fix · Feather · SAP · AMD · EY · Akamai · Vetcove

---



## Proof

- **9,400+** stars on GitHub ↗ ([GitHub](https://github.com/boundaryml/baml))
- **1.6M** SDK downloads a month ↗ ([npm](https://www.npmjs.com/package/@boundaryml/baml))
- **7** languages that `baml generate` targets ↗ ([/#toolchain-title](/#toolchain-title))
- **Apache 2.0** open source license, free to run locally ↗

How BAML compares:

- the syntax and readability of **TypeScript**
- the correctness and tooling of **Rust**
- the compile times and concurrency of **Go**
- the flexibility of **Python**

---



## Tests, builds, and production in one toolchain.

Go added `go test` to the language, and Rust added `cargo`. Each moved work that used to need separate tools into the toolchain. BAML's toolchain also keeps a typed record of every call your AI code makes in production.

AI code often fails in ways that tests don't catch, such as an unusual input or a model that changes its output format. Go and Java can profile production, but they don't record what a function decided. BAML records each call with its arguments and its typed result.


| Included with the language   | C (1972) | Go (2009)            | Rust (2015)  | **BAML**                                            |
| ---------------------------- | -------- | -------------------- | ------------ | --------------------------------------------------- |
| Compiler and types           | ✓        | ✓                    | ✓            | **✓, also checked at runtime**                      |
| Formatter                    |          | `gofmt`              | `rustfmt`    | `baml fmt`                                          |
| Test runner                  |          | `go test`            | `cargo test` | `baml test`                                         |
| Build and dependencies       |          | `go build`, `go mod` | `cargo`      | `baml generate`, with typed clients for 7 languages |
| Production profiling         |          | `pprof`, sampled     |              | **Exact counts per call path**                      |
| A typed record of every call |          |                      |              | **On by default**                                   |
| SQL over recorded calls      |          |                      |              | `baml query`                                        |
| Code discovery for agents    |          |                      |              | `baml describe`                                     |


**Recorded by the runtime.** BAML records each call's typed arguments and result, the step that failed, the call path, the code version, and any user or metadata you attach.

**Stored with your types.** Each value is stored with the type that produced it, so production data uses the same schema as your code. A coding agent that has read your `.baml` files already knows the schema.

**One CLI for people and agents.** `baml test`, `baml describe`, and `baml query` are part of the same toolchain, so a coding agent can write a change, run it, and check the results from the terminal.

---



## A language and toolchain for AI code.

BAML combines a familiar syntax, types that hold at runtime, and a command-line toolchain that people and coding agents both use.

- **TypeScript-like syntax.** Write unions, generics, and classes in a syntax TypeScript developers already read.
- **Types that hold in production.** Model replies are parsed into your types, and the recorded data keeps those types.
- **Recording in the runtime.** BAML records every call, error, call path, and code version by default, along with the user and metadata you attach. One line above a function changes how much it keeps.
- `baml query`**.** Query recorded calls with SQL, on your laptop or in Boundary.
- `baml describe`**.** Find functions, types, and tests by their place in the syntax tree, so an agent doesn't have to search text.
- `baml test` **and** `baml run`**.** Run any function or test from the command line.
- **Typed errors and green threads.** Handle every error case explicitly, and run work concurrently with `spawn` and `await`.
- **Incremental adoption.** Generate a typed client for Python, TypeScript, Go, Rust, Java, Swift, or C++, and keep the rest of your app as it is.

---



## From one function to production.

Write the AI parts of your app in BAML, call them like any other function, and query each call as soon as it finishes.

### 1. Write the function

`classify.baml`

```baml
class Verdict {
  label: Label,
  confidence: float,
}

function classify(text: string) -> Verdict {
  client: "openai/gpt-6-luna"
  prompt: `Classify the sentiment …`
}
```

The type, the model, and the prompt live in one file, so each change is a single diff.

### 2. Generate the client

`zsh  ~/support-triage`

```
$ baml generate
  baml_client/ · typescript, python

$ baml test classify
  2 tests · 2 passed
```

`baml generate` writes a typed client for every language in the project. `baml test` runs the tests stored next to the function.

### 3. Call it from your app

Tabs **app.ts** and **main.py**:

*app.ts*

```ts
import { b } from "./baml_client";

const v = await b.classify(
  "Oh great, another Monday.",
);
// v is a Verdict, and v.label is a union, not a string

if (v.label === "negative") escalate(v);
```

*main.py*

```python
from baml_client import b

v = await b.classify(
  "Oh great, another Monday.",
)
# v is a Verdict, and v.label is a union, not a str

if v.label == "negative": escalate(v)
```

The result comes back typed in your language, without a parsing step or a cast. The rest of the app stays the same.

### 4. Query the call

BAML recorded the call when it ran, including its arguments, its typed result, and its duration.

```
$ baml query "SELECT input_args, output_value, duration / 1e6 AS ms
              FROM spans WHERE span_name = 'classify'"
input_args                            | output_value                                | ms
--------------------------------------+---------------------------------------------+-----
{"text":"Oh great, another Monday."}  | {"label":"negative","confidence":0.94}      | 384
```

Attach a user and metadata when you call a function, and you can query by customer. This query adds up token usage per customer:

```sql
SELECT context_metadata['customer'] AS customer,
       sum(total_usage[0]['input_tokens'])  AS input_tokens,
       sum(total_usage[0]['output_tokens']) AS output_tokens
FROM spans
WHERE parent_span_id IS NULL
GROUP BY customer
```

On your machine, recordings are files in your project. Add a Boundary key to run the same queries over production.

---



## Built for coding agents.

`baml agent install` adds guidance for your project's BAML version. A coding agent then finds code with `baml describe`, runs functions with `baml run`, and checks the results with `baml query`.

[Read the quickstart](/quickstart)

Terminal (`zsh  ~/invoices`, with a **Copy** button):

```
$ baml agent install
  agent guidance installed for this project's BAML version

$ baml describe ExtractInvoice
  function ExtractInvoice(file: pdf) -> Invoice
  extract.baml:7 · 2 tests

$ baml run main
  Invoice { vendor: "Acme", total: 1240.00 }
```

Trace panel, titled **Recorded by the runtime, with no tracing SDK** · `baml run main`:

- main, 412ms
  - ExtractInvoice, 401ms
    - LLM call, 388ms
    - parse → Invoice, 4ms

---



## Works with the stack you already have.

`baml generate` compiles your BAML into a typed client for the language you ship. BAML works with the models and agent tools you already use.

BAML runs on macOS, Linux, and Windows. Support for C# is coming soon.

- **Languages.** Python, TypeScript, Go, Rust, Java, Swift, and C++, through `baml generate`.
- **AI integrations.** OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure, Ollama, OpenRouter, Vercel, Claude Code, MCP tools, and any compatible API.

---



## Free to run locally, with Boundary for production.

BAML records every run to files in your project at no cost. In production, send the recordings to Boundary, hosted for you or in your own AWS account. Your team and your coding agents can then run `baml query` over live traffic and build dashboards and alerts on the results. Boundary also accepts OpenTelemetry traces, so your BAML calls and the rest of your services land in the same place.

**[Explore Boundary](https://boundaryml.com)**

Stack diagram, top to bottom:

- **Studio.** Span trees, the profiler, and the dashboards and alerts that you or your agent build.
- `baml query`**.** SQL over every call, from Studio, the CLI, curl, or an agent.
- **Your data.** Hosted by Boundary or in your AWS account.
- *↑ recorded by the runtime*
- **BAML.** Apache 2.0, free on your machine, with no tracing SDK.

---



## Questions before you install.

**Is BAML production ready?**

Yes. BAML is in beta, which means the language and standard library may still change, but it's stable for production use. It doesn't have a package manager yet, and we plan to add one soon.

**Do I have to rewrite my app in BAML?**
No. `baml generate` writes a typed client for Python, TypeScript, Go, Rust, Java, Swift, or C++, and you call BAML functions from your existing code. You can adopt BAML one function at a time.

**What does BAML cost?**
BAML is free. The language, the toolchain, SDK generation, and the local playground are licensed under Apache 2.0 and run on your machine. Boundary, the hosted service that stores recordings from production, is a separate product that BAML never requires.

**Which models does it work with?**
BAML works with OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure, Ollama, OpenRouter, Vercel, Claude Code, MCP tools, and any compatible API. The model is one line in the function, such as `client: "openai/gpt-6-luna"`, so switching models is a one-line change. The tests next to the function show what the change affects.

**Where does my data go?**
Prompts go to the model provider named in the function. Recordings stay in files in your project, and they reach Boundary only if you add a key. Set `BAML_TELEMETRY=off` to turn recording off.

**How much does recording cost at run time?**
Recording costs about 24 ns a call for timing and profiles, and about 60 ns with captured values. A model call usually takes hundreds of milliseconds.

**Can I keep my current tracing?**
Yes. BAML records its own calls and works alongside your existing APM. Python and TypeScript functions marked with `@trace.instrument` are recorded alongside your BAML calls, and Boundary also accepts the OpenTelemetry traces you already send.

**Does my editor support BAML?**
`baml ide install` sets up VS Code and Cursor. Zed, JetBrains, and Neovim connect through the language server. `baml playground` opens an editor in your browser.

**What happens when BAML changes?**
BAML is pre-1.0, so some APIs and tooling will change. The information an agent reads is versioned with the binary. `baml describe` and the guidance from `baml agent install` always match your installed version, and the CLI prompts you to reinstall the guidance after an update.

---



## Open source and pre-1.0.

BAML is ready to use today, and some APIs and tooling may still change. The agent skill, the CLI, and `baml describe` always match your installed version.

- **Source code.** [github.com/boundaryml/baml](https://github.com/boundaryml/baml)
- **Demo projects.** [github.com/boundaryml/baml-demos](https://github.com/boundaryml/baml-demos)
- **Community Discord.** [discord.gg/yzaTpQ3tdT](https://discord.gg/yzaTpQ3tdT)

---



## Get started with BAML.

Install the toolchain, add the agent guidance, and run your first function. You can also copy the prompt below and give it to your coding agent.

**[Install BAML](/quickstart)** · [Explore BAML](/#features-title)

(The same install panel as the hero, with both tabs.)

---



## Footer

The scripting language for AI workflows.

**Language**

- [Quickstart](/quickstart)
- [Explore BAML](/#features-title)

**Open source**

- [Source code](https://github.com/boundaryml/baml)
- [Demo projects](https://github.com/boundaryml/baml-demos)
- [Discord](https://discord.gg/yzaTpQ3tdT)

**Boundary**

- [boundaryml.com ↗](https://boundaryml.com)

© 2026 Boundary ML · Build with BAML. Grow with Boundary.