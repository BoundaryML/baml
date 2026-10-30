# Boundary homepage copy

`/v1` keeps the first version of this page, with its copy frozen in `v1.md`. This file is the working copy for `/`.

Working copy for `typescript2/app-website`, route `/` (boundaryml.com). Edit this file first, then move the copy into the components. Links are as wired today, and `#` means the link has no destination yet. The testimonials, case study, and pricing plans still hold the design's placeholder copy.

## On the site now

The page at `/` follows the outline from "Hero" through the closing call to action below. Everything after the blank gap (from "We literally built…" down) is parked and not on the page. That includes the metrics, testimonials, case study, pricing, enterprise, FAQ, and "Works with OpenTelemetry" sections. The OTEL vs BAML rows from that table are used in the BAML section.

- **Main call to action:** "Book a meeting" in the nav, hero, and closing band. "Start free" stays beside it in the hero, with no destination yet.
- **Pricing links:** removed from the nav and footer, because the pricing section is parked.
- **Heading:** "Use BAML for more powerful queries." was the last of the three options.

Copy written for the gaps in the outline:

- **Build your own …:** the heading ends with the selected workflow. The workflows are user tracking, alerts, AI evals, dashboards, and model release testing, and each one shows its query or agent prompt. The list ends with "Or anything else your team needs."
- **Get started in minutes:** four steps, shown as slides. Send your traces, query them with SQL, build your tools, add BAML for richer data.
- **Closing band:** "Let your agents build faster." and "The AI observability platform for shipping faster to production."

To check before launch:

- OTLP ingest has to ship, because the hero and the first Get started step depend on it.
- "BAML is the only language designed for agents" is your wording, and the positioning audit warns against claims like "first".
- "BAML ↗" and "Explore BAML" still point at `/what-is-baml`, until the BAML site has a domain.
- "Customers" in the nav still opens the placeholder case studies.

## Draft 4 notes

Boundary now leads with OpenTelemetry. Any team can send the traces they already have and query them with SQL. BAML is an optional upgrade for the AI parts of the app: it costs less to record, keeps more about each call, and works with every part of Boundary. The OpenTelemetry path ends in "Book a meeting", so those teams talk to us first.

- **Boundary doesn't ingest OpenTelemetry yet.** The page says it does, so OTLP ingest has to ship before this copy goes live. Until then, "Book a meeting" is the only call to action on that path.
- **"Costs less to record" refers to recording overhead only** (about 24 ns a call). It makes no claim about Boundary's pricing.
- **The example queries use BAML's** `spans` **table.** OpenTelemetry spans may land with different columns. The two tools that need BAML data are marked "with BAML".
- **The page doesn't say "database" or "superpowered".** The first sounds like infrastructure to run, and the brief rules out the second.



## Draft 3 notes

This draft rewrites the prose in the style of Linear's and Stripe's homepages. Headings describe the product in one plain phrase. Section intros are one or two sentences that start with what you can do. Feature lists use a short bold title followed by one sentence.

Style rules for this page:

- No em dashes. Use a period, a comma, or parentheses instead.
- No rhetorical setups or punchlines, such as "Here's one trip around it", "What an agent can't invent is…", or "Your cloud, your data."
- No two-beat headings made of two short sentences.
- No hype words, such as "AI-native", "plugs into everything", or "runs everywhere".
- Use the serial comma, as Linear and Stripe do.
- Use sentence case, and no exclamation marks.

Content decisions, carried over from draft 2:

- The logo wall is removed. Boundary has no customer logos yet. The seven logos we have are BAML users, and they stay on the BAML page.
- The "Today, a person is the glue between six tools" section is removed. The tools table shows the same comparison with real queries.
- Tool names follow the accuracy audit in `app-why-baml/POSITIONING.md`. The page says "Sentry-style alerts for AI functions", not "your own Sentry", and the last column lists the tools teams use today rather than claiming Boundary replaces them.
- The queries use the tables `baml query` reads, `spans` and `profiler`. Four ran against real local recordings with BAML nightly 0.20.2. The per-customer query reads `total_usage`, which is newer than that nightly, so it hasn't been run. It reads only the first provider and model entry, so it assumes one model per call.
- The page says "no tracing SDK", not "no SDK".
- The footer keeps the brand line "Build with BAML. Grow with Boundary."
- The page doesn't call Boundary a database. A database sounds like infrastructure to run and secure, so the hero says what you can do instead. SQL is described as the way you ask questions about your calls.

Open question:

- The "6 → 1 tools, accounts, and API keys" metric reads as "replace your stack", which the positioning audit warns against. Keep it?

**Page title:** Boundary | Query your production traces with SQL
**Meta description:** Send Boundary your OpenTelemetry traces and query them with SQL, or write the AI parts of your app in BAML to record typed results, errors, and code versions with every call.

---



## Nav

- [Product](/#product)
- [Customers](/case-studies)
- [Pricing](/#pricing)
- [Docs](https://docs.boundaryml.com)
- [Blog](/blog)

[BAML ↗](/what-is-baml) · [Sign in](#) · **[Start free](#)**

---



Some notes:

- main call to action is a call with us for now btw.



## Hero



# Agentic Observability

[note: hero tells you exactly WHAT we do in 4 seconds, and what you can do with us].

Query OTEL traces to create alerts, dashboards, AI evals, or build your own tooling. Write your AI workflows in the BAML language to record rich, cheap telemetry data built for agents.

**[Book a meeting](https://cal.com/boundaryml/30min)** · [Start free](#)

---



<image of the observability app>



## Build your own [User tracking pipeline / Alerts system / anything]

[internal note: this is more on why you should care, and direct value prop]





<types of workflows you can do>

- one of them we should show is a proactive system -- like 'track model releases, and test them automatically'.





<this next section, i think the last header might be the best? for why you should baml. [note: reintroduces BAML, and WHY you would care about it. The strategy here is to compare against why youd use this vs OTEL]>

## Use BAML to make even more powerful tooling -- no SDKs required

## Use BAML for the first fully integrated AI toolchain

## Use BAML for more powerful queries

BAML is the language for agents. BAML programs emit even richer data, more efficiently than OTEL.



<OTEL vs BAML>



link to the baml website here.





### [alt2, we show you how baml solves not only the traces problem, but the whole loop]

### We 







## Get started in minutes

< how it works, a set of slides or something. >









## The all-in-one toolchain for agents

Existing languages leave you hanging once you want to deploy to production. There is no unified toolchain to debug issues in production.

BAML is the only language designed for agents. 

1. A sound type system like Rust
2. Fast compilation times, like Go
3. Low-cost telemetry, built into the Runtime
4. Built-in AI capabilities
5. Fully integrated with Boundary Cloud (or bring your own).
6. Cross-compatible with Python, Typescript, Rust, and more



BAML covers the full development lifecycle

< image of each language and the things they captured, and how baml now tackles the rest with baml query etc>





## <some call to action? idk what to put. Maybe: Let your agents build faster>

[i do want text: AI observability platform for shipping faster to production]

call to action is a call with us btw.



















































We literally built a scripting language that can be called from other languages.

No other language has been built

It is built with telemetry and agents in mind. Write the AI parts in BAML, for a fully vertically integrated experience with Boundary Cloud.

BAML allows you to write AI workflows and agents. Telemtry is built directly into the BAML runtime for the most efficient, richest telemetry you can get from any app.

<diagram on BAML

We didn't only build a telemetry database -- 









## Works with OpenTelemetry, and records more with BAML.

Boundary accepts the OpenTelemetry traces your services already send. BAML is an optional, open-source language for the AI parts of your app. It costs less to record, keeps more about each call, and works with every part of Boundary.

**OpenTelemetry.** Point your existing OTLP exporter at Boundary. Your spans, timing, errors, and attributes become queryable with SQL.
[Book a meeting](https://cal.com/boundaryml/30min)

**BAML.** Write AI functions in BAML and call them from Python, TypeScript, Go, and four other languages. The runtime records every call without a tracing SDK.
[Explore BAML ↗](/what-is-baml)


| What Boundary receives                             | OpenTelemetry traces                   | BAML                                       |
| -------------------------------------------------- | -------------------------------------- | ------------------------------------------ |
| Spans, timing, and errors                          | ✓                                      | ✓                                          |
| Model requests and token usage                     | When your instrumentation records them | ✓ on every call                            |
| Arguments and results                              | As the attributes you add              | ✓ stored with their BAML types             |
| The step that failed (request, provider, or parse) |                                        | ✓                                          |
| Exact call counts for each call path               |                                        | ✓                                          |
| The code version of each run                       |                                        | ✓                                          |
| Recording cost per call                            | Depends on your SDK                    | About 24 ns, or 60 ns with captured values |
| The same queries on your laptop with `baml query`  |                                        | ✓                                          |
| Tests built from production calls with `baml test` |                                        | ✓                                          |


---



## Build your own tools with SQL.

Coding agents can write an alert, a dashboard, or an eval runner from a single prompt. Boundary keeps every call your program makes in production, so each of these tools is one SQL query. The last two use data that only BAML records.


| What you build                                                                                         | Query                                                                                                                                                                                                                   | Tools teams use for this today |
| ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| **Sentry-style alerts for AI functions.** A Slack message when a function starts failing.              | `SELECT span_name, count(*) AS errors FROM spans WHERE status = 'user_error' GROUP BY span_name`                                                                                                                        | Sentry, Rollbar                |
| **Token usage per customer.** Input and output tokens for each customer, from the metadata you attach. | `SELECT context_metadata['customer'] AS customer, sum(total_usage[0]['input_tokens']) AS input_tokens, sum(total_usage[0]['output_tokens']) AS output_tokens FROM spans WHERE parent_span_id IS NULL GROUP BY customer` | PostHog, Mixpanel              |
| **Latency and volume by function.** A dashboard tile for each function.                                | `SELECT span_name, count(*) AS calls, avg(duration) / 1e6 AS avg_ms FROM spans GROUP BY span_name`                                                                                                                      | Datadog                        |
| **A single call in detail.** The arguments, typed result, and error for one call.                      | `SELECT input_args, output_value, error_value FROM spans WHERE span_id = '…'`                                                                                                                                           | Langfuse, Arize                |
| **Eval sets from real failures, with BAML.** Failed calls, exported as `baml test` cases.              | `SELECT input_args, output_value FROM spans WHERE span_name = 'user.ExtractInvoice' AND status = 'user_error'`                                                                                                          | Braintrust, Promptfoo          |
| **A dead-code map, with BAML.** Your functions, shaded by how often each one ran.                      | `SELECT function_name, sum(invocation_count) AS runs FROM profiler GROUP BY function_name`                                                                                                                              |                                |


Run a query in Studio to see the result as a chart or a table. Run it with `baml query` from the CLI or over HTTP to send the result to Slack, GitHub Actions, or your admin panel.

---



## Change, test, and roll out with your agent.

With BAML, the whole workflow runs from one CLI. You set the goal and approve the result. A coding agent does the work in between, and every step refers to the same function name.

1. **Write.** You ask an agent to lower the cost of ExtractInvoice without losing accuracy. The agent switches it to a cheaper model with a one-line change in BAML.
  *Today: prompts stored as strings in code*
2. **Test.** `baml test` runs the change against real examples captured from production, and all tests pass.
  *Today: Promptfoo and Braintrust*
3. **Release** (coming soon). A feature flag rolls the cheaper model out to 5% of users. During an incident, you can also change what the code does for a single customer.
  *Today: LaunchDarkly and Statsig*
4. **Observe.** Every call is traced in your own cloud, without a tracing SDK or manual spans.
  *Today: Datadog, Sentry, and Langfuse*
5. **Compare.** `baml query` compares the two models side by side. In this example, cost drops 32% and accuracy stays the same.
  *Today: PostHog and notebooks*
6. **Learn.** Studio dashboards show your team what changed, using the same data the agent used. After you approve, the change rolls out to all users.
  *Today: Slack threads and Looker*
7. **Repeat.** Start the next change from what you learned.

Code panel (`extract.baml`, labelled next to `main.py`):

```diff
 class Invoice {
   vendor: string,
   total: float,
   due_date: string?,
 }

 function ExtractInvoice(file: pdf) -> Invoice {
-  client: "openai/gpt-4o"
+  client: "openai/gpt-4o-mini"
   prompt: `
     Pull the fields out of ${file}.
     ${ctx.output_format()}
   `
 }
```

---



## One source for every dashboard, alert, and agent.

Every call your code makes is stored in one place, hosted by Boundary or in your own AWS account. You query it with SQL, and your dashboards, metrics, and alerts read from the same data, including the ones your agent builds.

Stack diagram, top to bottom:

- **Dashboards, metrics, and alerts.** Studio dashboards, metrics, and alerts, plus custom views your agent builds through the API.
- **SQL over every call.** One query language for people and agents, from Studio, `baml query`, curl, or an agent with a read key.
- **Your data.** Every call, input, output, and error in one place, hosted by Boundary or in your own AWS account.
- *↑ every call recorded*
- **Your traces.** OpenTelemetry from any service, or BAML for typed records of your AI functions.

---



## Record more with BAML.

BAML is a free, open-source language for the AI parts of your app. Recording costs about 24 ns a call, and each record includes typed arguments and results, the step that failed, and the code version. BAML also works with all of Boundary, from `baml query` on your laptop to `baml test` cases built from production calls.

**[Explore BAML ↗](/what-is-baml)** · [Star on GitHub](https://github.com/boundaryml/baml)

Code panel (`extract.baml`, labelled next to `main.py`):

```baml
class Invoice {
  vendor: string,
  total: float,
  due_date: string?,
}

function ExtractInvoice(file: pdf) -> Invoice {
  client: "openai/gpt-4o-mini"
  prompt: `
    Pull the fields out of ${file}.
    ${ctx.output_format()}
  `
}
```

Install line: `$ brew install baml` [copy]

---



## Metrics

- **6 → 1** tools, accounts, and API keys, down to one install
- **0** tracing SDKs to set up with BAML, because recording is built into the runtime
- **7** languages from one BAML codebase
- **10+** model providers, plus any compatible API

---



## Testimonials

> “Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua, ut enim ad minim.”

Firstname Lastname, Head of Engineering, Ledgerline

> “Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis aute irure dolor in reprehenderit.”

Firstname Lastname, Staff Engineer, Kestrel Labs

> “Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim.”

Firstname Lastname, CTO, Sundial

---



## Case study

Ledgerline

### Lorem ipsum dolor sit amet, consectetur adipiscing elit sed do.

- **00%** lorem ipsum dolor
- **0 days** lorem ipsum sit amet

[Read the case study](/case-studies/ledgerline)

---



## Start for free, and upgrade as your team grows.

[See full pricing](#)

**Free.** $0 per month.
Lorem ipsum dolor sit amet, consectetur adipiscing.

- ✓ Lorem ipsum dolor sit
- ✓ Consectetur adipiscing
- ✓ Sed do eiusmod tempor

[Start free](#)

**Team.** $00 per month.
Lorem ipsum dolor sit amet, consectetur adipiscing.

- ✓ Lorem ipsum dolor sit
- ✓ Consectetur adipiscing
- ✓ Sed do eiusmod tempor

[Start a trial](#)

**Enterprise.** Custom pricing.
Lorem ipsum dolor sit amet, consectetur adipiscing.

- ✓ Lorem ipsum dolor sit
- ✓ Consectetur adipiscing
- ✓ Sed do eiusmod tempor

[Contact sales](https://cal.com/boundaryml/30min)

---



## Run Boundary in your own cloud.

**[Book a meeting](https://cal.com/boundaryml/30min)**

- **Hosting.** Hosted by Boundary or in your own AWS account.
- **Access.** Query with SQL from Studio, the CLI, curl, or an agent with a read key.
- **Capture.** Every call is profiled. Inputs and outputs are captured only when you opt in, and private values stay in your data plane.

---



## Frequently asked questions.

[Can't find your question? Email the team](#)

**How is Boundary different from Datadog or Langfuse?**
Each of those tools covers one part of the workflow, and you connect them yourself. Boundary accepts your OpenTelemetry traces and lets you query them with SQL. With BAML, it also keeps writing, testing, releasing, and comparing in one place, connected by your function names, so a coding agent can run the whole workflow.

**Do I have to write my code in BAML?**
No. Boundary works with the OpenTelemetry traces you already send. BAML adds richer records for the functions you write in it, and you can adopt it one function at a time.

**How do I send OpenTelemetry traces to Boundary?**
[Book a meeting](https://cal.com/boundaryml/30min) with the team, and we'll help you point your OTLP exporter at Boundary.

**Where is my data stored?**
Your data is stored in one place, hosted by Boundary or in your own AWS account. People query it from Studio, the CLI, or curl, and agents query it with a read key.

**What is captured, and what stays private?**
Every call is profiled. Inputs, outputs, and errors are captured when you opt in, and private values stay in your data plane.

**Is BAML ready for production?**
BAML is open source and pre-1.0. It is ready to use today, and some APIs and tooling may still change. The agent skill, the CLI, and `baml describe` always match your installed version.

**What is available today, and what is coming?**
Writing in BAML, tracing, SQL queries, and Studio dashboards are available today. Feature flags are next. They let you roll a change out to a share of users, or change what the code does for one customer during an incident. Datasets are also planned.

---



## Footer

Build with BAML. Grow with Boundary.

**Product**

- [Profiling](/#product)
- [Boards](/#product)
- [SQL](/#product)
- [Pricing](/#pricing)

**Resources**

- [Docs](https://docs.boundaryml.com)
- [Blog](/blog)
- [Changelog](/changelog)
- [BAML ↗](/what-is-baml)

**Company**

- [About](/who-are-we)
- [Careers](/jobs)
- [Contact](#)
- [GitHub](https://github.com/boundaryml/baml)

© 2026 Boundary · [Privacy](/privacy-policy) · [Terms](/tos) · [Security](#)