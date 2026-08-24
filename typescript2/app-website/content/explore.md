# Explore BAML

> BAML is the programming language for agents.

BAML is a Turing-complete programming language designed for a world where agents write and run a growing share of the code. It aims to prevent context pollution and churn when coding with AI: mistakes should be difficult to represent, behavior should be inspectable, and the language should remain dynamic enough for agents to create and execute programs.

In one sentence: BAML feels like TypeScript, but with better error handling, no `any`, and agent-first tooling.

Canonical web page: https://boundaryml.com/explore

## Contents

1. [Design philosophy](#design-philosophy)
2. [A better language](#a-better-language)
3. [Tools for agents](#tools-for-agents)
4. [Tools for humans](#tools-for-humans)
5. [Adopting BAML](#adopting-baml)
6. [Building agents](#building-agents)
7. [Try BAML](#try-baml)

## Design philosophy

### Invent as little as necessary

The more BAML invents, the worse agents will be at using it. Anything that differs from languages agents already know should be a deliberate decision with a clear payoff.

### Read like TypeScript, without the footguns

Agents and humans both work well with TypeScript's familiar types, unions, and generics. But TypeScript sits on top of JavaScript and necessarily retains escape hatches that agents tend to abuse:

```typescript
// The model reply is only a string, but this unchecked cast compiles.
const user = reply as unknown as User
```

BAML parses and validates values instead. There is no unchecked cast to write:

```baml
function ExtractUser(text: string) -> User
```

### Make undesired states unrepresentable

An agent sampling tokens will eventually produce an invalid state. If that state cannot compile, it cannot ship, and a human does not have to catch it during review.

```baml
class Ticket {
  status: "open" | "closed",
}
```

`"opne"` is not a valid status, so it fails before runtime.

### Trace nondeterminism

Today is the most code humans will ever read. As more code is generated, the practical way to understand a system will increasingly be through focused traces and replay rather than reading every source file.

### Leave one obvious way

Every option eventually gets used by some agent. A codebase with five equivalent patterns invites the next agent to add a sixth. BAML prefers one predictable path.

### Keep edits local

When a change is not local, an agent goes on a side quest across the codebase. It returns with a polluted context full of unrelated files. BAML tries to make definitions discoverable by name and to keep changes close to the code they affect.

### Build tools for agents, not only IDEs

Humans still need editors, graphs, and debuggers. Agents also need first-class ways to discover and operate on code without hovering, clicking, or interpreting a visual interface.

## Install BAML

Just want to install it and run something? Install the toolchain and run a project:

```sh
# macOS or Linux (Homebrew)
brew install baml
# or: curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s

baml init
baml agent install
baml run main
```

Full quickstart, editor setup, and more options: https://boundaryml.com/quickstart.md

## A better language

BAML starts with familiar syntax and a type system that resembles TypeScript, then changes the parts that become dangerous when agents write most of the code.

### Types exist at runtime

TypeScript explicitly chose not to be sound, trading correctness for human productivity. That was a reasonable choice for its ecosystem, but it is the wrong default when an agent can confidently add an unchecked cast.

BAML has no `any`. Types mean at runtime what the program says they mean. The language includes unions, generics, recursive types, and interfaces.

In TypeScript, this compiles and fails later:

```typescript
interface User {
  name: string
  email: string
}

const user = JSON.parse(raw) as User
user.email.toLowerCase()
```

In BAML, an `unknown` value must be proven to have the expected type:

```baml
class User {
  name: string,
  email: string,
}

function load(raw: unknown) -> string {
  if (raw is User) {
    raw.email.to_lower_case()
  } else {
    // This does not compile. `raw` is still unknown.
    raw.email.to_lower_case()
  }
}
```

### Match on types or values

BAML pattern matching replaces grab-bags of `typeof`, `instanceof`, and property-existence checks. The compiler can verify that the match is exhaustive.

```baml
function route(msg: Refund | Question | string) -> string {
  match (msg) {
    Refund => `refund ${msg.id}`,
    Question { text } => `answer: ${text}`,
    string => `text: ${msg}`,
  }
}

function grade(n: int) -> string {
  match (n) {
    100 => "perfect",
    let score if score >= 60 => "pass",
    _ => "fail",
  }
}

class Refund { id: string }
class Question { text: string }
```

Design proposal: [BEP-015](https://beps.boundaryml.com/beps/15).

### Typed error handling

TypeScript exceptions are untyped. In BAML, every `throws` declaration contributes to the inferred error set. Catch arms are matched by type, and the compiler can identify missing or impossible arms.

```baml
function show(ok: bool) -> string {
  fetch_page(ok) catch (error) {
    NetError => "recovered: " + error.detail,
    // Compiler warning: this arm is unreachable because fetch_page
    // cannot throw ParseError.
    ParseError => "unreachable",
  }
}

function fetch_page(ok: bool) -> string {
  if (!ok) {
    throw NetError { detail: "timeout" }
  }
  "<html>"
}

class NetError { detail: string }
class ParseError { detail: string }
```

Design proposal: [BEP-002](https://beps.boundaryml.com/beps/02).

### Green threads: async without function coloring

BAML follows Go's approach to concurrency with a TypeScript-like feel. `spawn` can start any function concurrently; the function and its entire call graph do not need separate `async` declarations.

```baml
function main() -> int {
  let a = spawn { work(1) };
  let b = spawn { work(2) };
  let c = spawn { work(3) };

  (await a) + (await b) + (await c)
}

function work(i: int) -> int {
  i * i
}

test "spawn and await" {
  assert.equal(main(), 14)
}
```

This works for slow LLM requests and tool calls, but it is not limited to I/O. CPU-bound BAML work can run across cores too.

In a benchmark that scans 38.4 GB of text:

| Runtime | Time | CPU usage |
| --- | ---: | ---: |
| Bun, one thread | 8.2 s | 1 core |
| BAML, one thread | 6.8 s | 1 core |
| BAML, `spawn` ×16 | 0.87 s | about 10 cores |

The benchmark uses 16 shards, each scanning roughly 48 MB of text 50 times. BAML's standard-library string search is implemented in native Rust; Bun still wins per core for tight arithmetic loops where its JIT has an advantage.

Design proposal: [BEP-034](https://beps.boundaryml.com/beps/34).

## Tools for agents

BAML includes tools that help agents find, run, test, and distribute code without reconstructing the project through repeated grep and file reads.

### The filesystem is the namespace structure

AI agents spend too much time searching large projects. In BAML, namespace directories use an `ns_` prefix, and the directory tree is the program map:

```text
baml_src/
├── ns_catalog/
│   └── product.baml
└── ns_orders/
    └── order.baml
```

There are no imports. Definitions inside a namespace are available throughout that namespace, while definitions in another namespace are addressed by one fully qualified name:

```baml
function line_item() -> root.catalog.Product {
  root.catalog.Product { name: "Keyboard" }
}
```

Classes are always constructed with their names—`Product { ... }`—rather than anonymous record syntax.

Design proposal: [BEP-008](https://beps.boundaryml.com/beps/08).

### `baml describe`: AST-aware discovery

`baml describe` is easier for an agent to use than an LSP and more informative than grep. It returns the resolved definition, dependencies, and call sites without making the agent read whole files into context.

```text
$ baml describe greet

function greet  baml_src/main.baml:5-7

function greet(name: string) -> Greeting {
    Greeting { message: "hi, " + name }
}

dependencies:
  class  Greeting  baml_src/main.baml:1

references (2):
  baml_src/main.baml:10  greet("world").message
  baml_src/main.baml:18  assert.equal(greet("bob")…
```

The resolved reference list is especially useful: an agent can see every call site and spot near-duplicate implementations before writing another copy.

### `baml run <function>`: every function can be a CLI

Agents can run any function directly. BAML derives flags and help text from the function signature.

```text
$ baml run main
"hi, world"

$ baml run greet -- --name "hacker news"
Greeting { message: "hi, hacker news" }

$ baml run greet -- --help
function greet(name: string) -> Greeting

Options:
      --name <string>
```

Design proposal: [BEP-027](https://beps.boundaryml.com/beps/27).

### `baml run -e`: execute a small program inline

For short experiments, an agent does not need to create a file or project:

```text
$ baml run -e '"a,b,c".split(",")'
["a", "b", "c"]

$ baml run -e '{ let t = 0; for (let i = 0; i < 5; i += 1) { t += i * i; }; t }'
30
```

This gives an agent a fast write-run-observe loop for small BAML programs.

### `baml pack`: ship a function as a binary

`baml pack` turns selected BAML functions into a small standalone CLI. Function signatures become subcommands and flags, and the binary can target another architecture.

```text
$ baml pack -f greet -f main -o greet-bin
Finished greet-bin [greet,main, aarch64-apple-darwin]

$ ./greet-bin greet --name "hacker news"
{"message":"hi, hacker news"}

$ ./greet-bin --help
Usage: greet-bin <COMMAND>

Commands:
  greet  function greet(name: string) -> Greeting
  main   function main() -> string
```

In the published comparison, the BAML binary is 12.1 MB, or 5.7 MB compressed. The equivalent Bun 1.3.14 binary is 63.1 MB, or 23.5 MB compressed. Bun embeds a complete JavaScript engine; the BAML runtime does not.

## Tools for humans

Agent-first does not mean human-hostile. Humans still need ways to understand unfamiliar generated systems, inspect execution, and iterate when something fails.

### Workflow View

The BAML Workflow View renders a navigable graph of a program. The Explore page demonstrates it with a Heads Up guessing game containing:

- An LLM-driven guessing loop
- A deterministic binary search
- Classes and methods
- Multiple testsets

Each node maps back to source. The graph is the visual counterpart to `baml describe`: humans click through the relationships that agents retrieve through the CLI.

### Profiler

The BAML profiler displays execution as a flame graph with per-function timing. It helps identify slow functions, excessive fan-out, and time spent in LLM or tool calls. Agents can consume trace data programmatically; humans can inspect the same execution visually.

## Adopting BAML

BAML is pre-1.0, but it is available to use today. Adoption does not require rewriting an existing application.

### Drop it into an existing stack

BAML generates native SDKs so Python and TypeScript applications can call BAML through type-safe interfaces. Functions, methods, classes, and generics survive across the boundary.

The flow is:

```text
baml_src/                     BAML functions, types, and tests
    ↓ baml generate
baml_sdk/                     generated typed client and runtime
    ↓
existing Python or TypeScript application
```

For example, a BAML function returning `Resume` becomes a Pydantic v2 model in Python and exposes both synchronous and asynchronous calls:

```python
from baml_sdk import extract_resume, Resume

resume: Resume = extract_resume(text="Jane Doe, jane@acme.com ...")
print(resume.name)
print(resume.email)
print(resume.model_dump())
```

Teams can move one function at a time. BAML can call into the host application through generated interfaces as well as being called from it.

Design proposal: [BEP-030](https://beps.boundaryml.com/beps/30).

### Recursive self-improvement

Boundary runs thousands of simulated agents against BAML to find places where the language, errors, CLI, and skill are difficult to use. [Agent Tries BAML](https://boundaryml.com/atb) measures these attempts, and the [arena](https://boundaryml.com/atb/arena) compares which version of the BAML skill helps agents succeed faster.

### Supply-chain tradeoff

BAML does not yet have a package manager. That removes one familiar source of dependency and supply-chain risk, but it also means reusable package distribution is still being designed.

## Building agents

BAML is a general-purpose language, but its main focus is software that interacts with nondeterministic models and agents.

One stochastic call affects everything above it in the call graph. If `llm.summarize_chunk()` can return a different result for the same input, then `summarize()`, `run_pipeline()`, and `main()` become nondeterministic too. Ordinary `assert output == expected` tests are no longer enough. BAML therefore makes nondeterministic operations observable, testable, and measurable.

### LLM calls are native functions

An LLM call in BAML is a function: the prompt is its body and the return type is its schema. Because it is a real function, it composes with ordinary code and can be evaluated, optimized, traced, and tested.

```baml
class Resume {
  name: string,
  email: string?,
}

function extract_resume(text: string) -> Resume {
  client: "openai/gpt-4o-mini"
  prompt: `
    Extract the resume.
    ${ctx.output_format}
    ${text}
  `
}
```

BAML retains the error-correcting structured-output parser from earlier versions, which can recover valid typed values from imperfect model output. Workflow tooling records LLM inputs and outputs, including multimodal values such as images.

### Harnesses and delegated agents

Boundary is building a first-class standard library for agent harnesses, realtime voice agents, batch APIs, and delegation to external agents such as Claude Code. This surface is still under development.

### Tests and testsets are code

Tests can live in any BAML file. Testsets can generate tests dynamically from arrays, CSV files, object storage, or another runtime source.

```baml
testset "from a csv" {
  let rows = "text,expect
loved it,positive
absolutely great,positive
worst purchase ever,negative".split("\n");

  for (let row in rows.slice(1, rows.length())) {
    let cols = row.split(",");
    test ("classify: " + cols[0]) {
      assert.equal(classify(cols[0]), cols[1])
    }
  }
}
```

Agents can run these with `baml test`. Humans can also inspect them in the Playground.

Statistical evaluation is code too. A custom runner can execute a nondeterministic test repeatedly and require a quorum of successful runs:

```baml
test "tolerates flaky runs" with quorum {
  assert.is_true(check_inventory())
}
```

Custom runners can implement retries, parallel or sequential execution, LLM-as-judge evaluation, and report uploading.

Design proposal: [BEP-023](https://beps.boundaryml.com/beps/23).

### Typed `eval`

Agents frequently write code and then execute it. Ordinary `eval` is unsafe and discards type information. BAML's planned reflection API compiles generated code against an expected signature and returns typed compiler errors that can be fed back to the agent.

```baml
let package = reflect.new_package("generated");
// Add generated source to the package, then compile it.
let callback = package.build().get<() -> string>("hello");
print(callback())
```

This reflection API is not available yet. The example describes the intended direction.

### Scoped function mocking and sandboxing

Machine-level sandboxing is still necessary when running untrusted code. BAML also plans scoped mocking so a program can replace dangerous functions—such as network or shell access—within one execution scope.

```baml
let net = baml.mock.new(baml.http.fetch);
net.replace((request: baml.http.Request) -> baml.http.Response {
  throw baml.NotImplementedError {
    message: "fetch is disabled in this scope"
  }
});

baml.mock.scope([net], () -> void {
  run_generated()
});
```

Mocking cannot isolate machine state, so it does not replace a process or virtual-machine sandbox. The mocking primitive is also not available yet.

Design proposal: [BEP-058](https://beps.boundaryml.com/beps/58).

## Try BAML

Install the toolchain using one of these commands:

```sh
# Homebrew on macOS or Linux
brew install baml

# Install script on macOS or Linux
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s

# Arch Linux
yay -S baml-bin
```

```powershell
# Windows PowerShell
irm https://pkg.boundaryml.com/install.ps1 | iex
```

Then initialize a project, install the version-matched agent skill, and run its entry point:

```sh
baml init
baml agent install
baml run main
```

Use `baml describe` to discover definitions and relationships in the installed version of the language.

- [Full quickstart](https://boundaryml.com/quickstart.md)
- [BAML on GitHub](https://github.com/boundaryml/baml)
- [Join the BAML Discord](https://boundaryml.com/discord)
