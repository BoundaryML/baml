# BAML homepage copy

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
- [Pricing](/pricing)

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





## Write it once. run it in your stack.





## Query locally, or in the cloud

<here is where we introduce boundary, similar stuff as the 'baml is free. boundary is where its calls go'.>



## Before you install

1. Do i have to rewrite my app in baml
2. What does baml cost
3. Which models does it work with
4. Where does my data go
5. Can I export telemetry elsewhere? Yes, we are working on OTEL telemetry export <somewhere along those lines>



## Start using BAML

<same as website>





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
  client: "openai/gpt-4o"
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
- **Apache 2.0** open source license, free to run locally ↗ ([/pricing](/pricing))

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


**Recorded by the runtime.** BAML runs your functions, so it records every call with its typed arguments and result, the step that failed, the call path, the code version, and the user and metadata you attach. Recording costs about 24 ns a call for timing and profiles, and about 60 ns with captured values.

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
  client: "openai/gpt-4o"
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
              FROM spans WHERE span_name = 'user.classify'"
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
- **Agent interfaces.** The CLI, the language server, the agent skill, the playground, VS Code, and Cursor.

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

**Do I have to rewrite my app in BAML?**
No. `baml generate` writes a typed client for Python, TypeScript, Go, Rust, Java, Swift, or C++, and you call BAML functions from your existing code. You can adopt BAML one function at a time.

**What does BAML cost?**
BAML is free. The language, the toolchain, SDK generation, and the local playground are licensed under Apache 2.0 and run on your machine. Boundary, the hosted service that stores recordings from production, is a separate product that BAML never requires.

**Which models does it work with?**
BAML works with OpenAI, Anthropic, Gemini, Vertex, Bedrock, Azure, Ollama, OpenRouter, Vercel, Claude Code, MCP tools, and any compatible API. The model is one line in the function, such as `client: "openai/gpt-4o"`, so switching models is a one-line change. The tests next to the function show what the change affects.

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
- [Pricing](/pricing)

**Open source**

- [Source code](https://github.com/boundaryml/baml)
- [Demo projects](https://github.com/boundaryml/baml-demos)
- [Discord](https://discord.gg/yzaTpQ3tdT)

**Boundary**

- [boundaryml.com ↗](https://boundaryml.com)

© 2026 Boundary ML · Build with BAML. Grow with Boundary.