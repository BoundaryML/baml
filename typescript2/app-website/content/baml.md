# BAML is a programming language with built-in telemetry

November 17, 2026 · The Boundary team

It is open source, it is typed like TypeScript, and its runtime records every function call. This post covers why we built a language to get that, what the language looks like, and what the tracing costs.

Web page: https://boundaryml.com/baml

## The short version

- BAML is a statically typed language with its own compiler and virtual machine. Both are written in Rust and licensed under Apache 2.0.
- The virtual machine records every function call: its arguments, its result, how long it took, and what called it. You don't install an SDK or annotate anything.
- The traces go to a SQLite file on your machine, or to Boundary Cloud if you use it, and you query them with SQL.
- You don't have to rewrite your application. BAML generates a typed SDK, so Python, TypeScript, Go, and other languages call BAML functions like their own.

On the web page, each BAML sample is a live editor backed by the real compiler and virtual machine, built as WebAssembly and running in the browser tab.

## Why a language

We did not set out to build a programming language. We set out to make production software easier to debug, and kept reaching the same limit.

Tracing is something you add. You install an SDK, wrap the calls you expect to matter, and decide what to attach to each span. The calls you didn't wrap leave no record, and during an incident those are often the ones you need.

Coding agents make this worse in two ways. An agent instruments what it is told to, and not consistently. It is also the reader that most needs complete data, because it has no other way to learn what happened in production.

A library sees the calls that are routed through it. A runtime sees all of them. So we wrote a runtime, and a runtime needs a language. Owning both also let us make choices that a library can't: types that still exist while the program runs, errors that are part of a function's type, and model calls that the compiler understands.

## The language

The syntax will look familiar if you write TypeScript. The differences are in what the compiler checks and what survives to run time.

### Types exist at run time

TypeScript erases its types before the program runs, so a value that arrives from the network is whatever it happens to be. In BAML a type is there at run time too. There is no `any` and no unchecked cast: to use an `unknown` as a `User`, you test it, and the test inspects the value.

```baml
class User {
    name: string,
    email: string,
}

function email_of(raw: unknown) -> string {
    if (raw is User) {
        // Here raw is a User, because the check above ran.
        raw.email.to_lower_case()
    } else {
        throw "not a user"
    }
}

function main() -> string {
    email_of(User { name: "Ada", email: "ADA@EXAMPLE.COM" })
}
```

The same holds for a union. `match` picks an arm by the value's type, and the compiler checks that every member of the union has one.

```baml
// A union is matched by type. Remove an arm and the compiler
// tells you which case is no longer handled.
function route(msg: Refund | Question | string) -> string {
    match (msg) {
        Refund => `refund ${msg.id}`,
        Question { text } => `answer: ${text}`,
        string => `text: ${msg}`,
    }
}

// A match can also be on values, with guards.
function grade(score: int) -> string {
    match (score) {
        100 => "perfect",
        let s if s >= 60 => "pass",
        _ => "fail",
    }
}

class Refund {
    id: string,
}

class Question {
    text: string,
}

function main() -> string[] {
    [
        route(Refund { id: "r_812" }),
        route(Question { text: "Where is my order?" }),
        route("hello"),
        grade(100),
        grade(72),
        grade(12),
    ]
}
```

### Errors are part of the signature

A function declares what it can throw. `catch` matches on an error's type the way `match` matches on a value's, and the compiler checks that the arms cover what can be thrown. A function declared `throws never` has to handle everything, or it doesn't compile.

```baml
// handle is declared to throw nothing, and the compiler holds it
// to that: delete either arm and this no longer compiles.
function handle(ticket: string) -> string throws never {
    route(ticket) catch (e) {
        Unroutable => `triage queue (${e.label})`,
        Spam => "discarded",
    }
}

function route(ticket: string) -> string throws Unroutable | Spam {
    match (classify(ticket)) {
        "billing" => "support",
        "bug" => "engineering",
        "spam" => throw Spam { reason: "prize" },
        "other" => throw Unroutable { label: "other" },
    }
}

class Unroutable {
    label: Label,
}

class Spam {
    reason: string,
}

type Label = "billing" | "bug" | "spam" | "other";

// Stands in for a model call, so this runs without an API key.
function classify(ticket: string) -> Label {
    let t = ticket.to_lower_case();
    if (t.includes("charged") || t.includes("invoice")) {
        "billing"
    } else if (t.includes("crash") || t.includes("error")) {
        "bug"
    } else if (t.includes("prize")) {
        "spam"
    } else {
        "other"
    }
}

function main() -> string[] {
    [
        handle("Charged twice for one order"),
        handle("App crashes on login"),
        handle("You have won a prize"),
        handle("Do you ship to Canada?"),
    ]
}
```

### Concurrency without async functions

`spawn` runs an expression on a green thread, and `await` joins it. Functions are not divided into synchronous and asynchronous kinds, so making one call concurrent doesn't change the signature of everything above it.

```baml
// Nothing here is marked async. spawn starts a green thread,
// and await joins it.
function main() -> int {
    let a = spawn { score(3) };
    let b = spawn { score(4) };
    let c = spawn { score(5) };
    (await a) + (await b) + (await c)
}

// One shard of work.
function score(seed: int) -> int {
    let total = 0;
    for (let i = 0; i < seed * 1000; i += 1) {
        total += i % 7;
    }
    total
}

test "three shards add up" {
    assert.is_true(main() > 0)
}
```

### A model call is a function

Much of the code people write in BAML calls language models, so the language has a construct for it. A function can have a model, a prompt, and tools in place of a body. Its return type is the contract: the model's reply is parsed and checked against it before your code sees it. Tools are ordinary functions, and their doc comments are what the model reads.

```baml
// An agent is a function with a model, a prompt, and tools.
function support_agent(ticket: string) -> Answer | FollowUp {
    client: anthropic.Client.new(model = "claude-sonnet-5-5")
    tools: [lookup_customer, search_tickets]
    prompt: `
        Answer this ticket, or ask one question.
        Look up the customer first.
        ${ticket}
    `
}

class Answer {
    reply: string,
}

class FollowUp {
    question: string,
}

// Tools are plain functions. Their docs tell the model what they do.
/// Look up a customer's plan and billing status.
function lookup_customer(email: string) -> string {
    `${email}: pro plan, paid through March`
}

/// Search past tickets for ones like this one.
function search_tickets(query: string) -> string[] {
    ["Charged twice", "Wrong plan on invoice"]
}
```

### Tests are part of the language

A test sits in the same file as the code it tests, and `baml test` runs it. A `testset` builds its tests when it runs, which is how you turn a file of production examples into one named test for each.

```baml
test "a double charge is billing" {
    assert.equal(classify("Charged twice for one order"), "billing")
}

// A testset builds its tests when it runs: one for each case here,
// named for the ticket.
testset "tickets" {
    let cases = [
        Case { ticket: "Wrong plan on invoice", label: "billing" },
        Case { ticket: "App crashes on login", label: "bug" },
        Case { ticket: "Do you ship to Canada?", label: "other" },
    ];
    for (let c in cases) {
        test c.ticket {
            assert.equal(classify(c.ticket), c.label)
        }
    }
}

class Case {
    ticket: string,
    label: Label,
}

type Label = "billing" | "bug" | "other";

// Stands in for a model call, so this runs without an API key.
function classify(ticket: string) -> Label {
    let t = ticket.to_lower_case();
    if (t.includes("charged") || t.includes("invoice")) {
        "billing"
    } else if (t.includes("crash") || t.includes("error")) {
        "bug"
    } else {
        "other"
    }
}
```

## What the runtime records

Every function call becomes a span: its name, arguments, return value, start time, duration, status, and parent. On your machine the spans are stored in SQLite, and `baml query` runs SQL over them.

```sh
baml query "SELECT span_name, duration, input_args
            FROM spans
            ORDER BY duration DESC
            LIMIT 5"
```

The arguments and results are stored as values with their types, not as strings, so a query can reach inside them: `WHERE input_args['customer']['name'] = 'bob'`.

Recording every call is only practical if it is cheap. Observing one function call costs 32 nanoseconds in BAML. On the same machine, an Apple M2 Max, the same measurement with OpenTelemetry comes to 186 ns from Rust, 702 ns from TypeScript, and 9,288 ns from Python.

## The toolchain

One binary, `baml`, holds the compiler, the test runner, the formatter, and the tools below. They are built for coding agents as much as for people, which mostly means they are commands that print something an agent can read.

- `baml check` type-checks the project.
- `baml run greet -- --name "world"` runs any function as a command. Its parameters are the flags.
- `baml run -e '"a,b,c".split(",")'` evaluates an expression without a file.
- `baml describe` looks up a definition, its signature, what it depends on, and what refers to it.
- `baml test` runs the tests, and `baml query` queries the traces.

## Using it from the code you have

You can write a whole service in BAML, but we recommend starting with one function: the part of the application you most need to watch. BAML generates a typed SDK for the host language, and the function is imported and called like any other.

```ts
import { classify_ticket } from './baml_sdk';

const label = await classify_ticket(ticket);
```

The call is traced like any other BAML call. There is a longer guide to the three ways to adopt BAML at https://boundaryml.com/adopt.

## Status

BAML is pre-1.0. The language is usable today, and some APIs and tools will still change. These are not finished:

- **Compiling to native code.** Today BAML runs on its virtual machine. A native backend is in progress: https://github.com/BoundaryML/baml/pull/5218
- **Durable execution.** Pausing a run on one machine and resuming it on another exists as a prototype: https://github.com/BoundaryML/baml/pull/4956
- **Sandboxes** are proposed (https://beps.boundaryml.com/beps/63), feature flags are being designed, and formal verification is research.

## Install it

```sh
brew install baml
# or: curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s

baml init
baml run main
```

- [Quickstart](https://boundaryml.com/quickstart.md): Windows and editor setup
- [Source code](https://github.com/boundaryml/baml)
- [Community Discord](https://boundaryml.com/discord)
