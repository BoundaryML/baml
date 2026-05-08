# BAML Agent Guide

This file teaches an AI coding agent how to write idiomatic BAML with minimal context. BAML is a typed language for reliable LLM functions, structured outputs, and small orchestration programs. Prefer typed BAML schemas and functions over prompt-only contracts, hand-written JSON, and host-language glue.

## Agent Loop

Use the tools as part of the edit loop:

```bash
baml run --list                 # compile and list callable functions
baml describe --symbols         # list project symbols
baml describe baml.json.decode  # inspect stdlib/project APIs before guessing
baml test --list
baml test -i "suite::case"
baml run --function Main --json-args @args.json --output json
baml run -e 'SomeHelper("sample")'
baml fmt baml_src/main.baml
baml generate                  # regenerate host-language client code
```

Rules for agents:

- Run `baml describe` instead of inventing stdlib names.
- Keep the entire project compiling; `run -e` still compiles all `.baml` files.
- Use `--json-args` for classes, arrays, maps, optionals, unions, and nested input.
- Use `--output json` when a host program or bridge reads BAML output.
- Format touched `.baml` files before finishing.
- Run `baml generate` after changing BAML functions/types that application code imports.

## Project Shape

BAML has no imports. The CLI loads all `.baml` files under the project root or `--from` directory.

```text
my-app/
  baml.toml
  baml_src/
    main.baml
    types.baml
    clients.baml
    ns_eval/
      metrics.baml
```

Namespaces come from `ns_<name>` directories. Plain folders are only organization.

```baml
// baml_src/types.baml, root namespace
class Config {
  model string
}

// baml_src/ns_eval/metrics.baml, namespace root.eval
class Score {
  passed bool
  reason string
}

function Judge(config: root.Config, output: string) -> Score {
  Score {
    passed: output.length() > 0,
    reason: "checked with " + config.model,
  }
}
```

Same namespace symbols are bare. Cross-namespace symbols use `root.eval.Score`. `root` means "this BAML project's root namespace", not the standard library and not an import alias. Code inside `ns_eval/` uses `root.Config` to refer back to a root-level class, and root code uses `root.eval.Score` to refer into that namespace. Stdlib uses `baml.*`, `assert.*`, `log.*`, and `io.*`. Do not write import statements.

## Syntax Essentials

Write formatted BAML. The parser may accept looser punctuation, but agents should emit canonical code.

```baml
type UserId = string
type Metadata = map<string, json>

enum Priority {
  Low,
  Medium,
  High,
}

class Ticket {
  id UserId
  title string
  priority Priority
  tags string[]
  metadata Metadata?

  function new(id: UserId, title: string) -> Ticket {
    Ticket {
      id: id,
      title: title.trim(),
      priority: Priority.Medium,
      tags: [],
      metadata: null,
    }
  }

  function label(self) -> string {
    self.id + ": " + self.title
  }
}
```

Remember:

- Class fields use `name Type`, with no colon.
- Function parameters and `let` annotations use `name: Type`.
- Object constructors and maps use `key: value`.
- Methods take explicit `self`.
- Factories are ordinary methods without `self`, often `Type.new(...)`.
- The final expression in a block returns. A trailing semicolon discards the value.
- Functions that return nothing should declare `-> null` and end with `null` or `return null;`.
- `for` headers require `let`: `for (let item in items)`.
- Put a semicolon after a statement-style `if` inside loops.

Common types:

```baml
int
float
bool
string
null
unknown
never
json
image
audio
uint8array
Ticket[]
map<string, int>
Ticket?
Ticket | string | null
"open" | "closed"
1 | 2 | 3
```

Use `json` for arbitrary valid JSON. Use `unknown` only when the value may be any BAML value, including non-JSON runtime values.

There is no broad implicit coercion. `int + float` produces a `float`, but `int`, `float`, `bool`, and `string` are distinct types. Convert intentionally, for example with `baml.unstable.string(value)` when building display text.

## Collections And Strings

```baml
function Normalize(raw: string) -> string {
  let normalized = raw.trim().replaceAll("\r", "").replaceAll("\t", " ");

  while (normalized.includes("  ")) {
    normalized = normalized.replaceAll("  ", " ");
  }

  normalized
}

function TagSummary(tags: string[]) -> string {
  let clean: string[] = [];

  for (let tag in tags) {
    let value = Normalize(tag).toLowerCase();
    if (value.length() > 0) {
      clean.push(value);
    };
  }

  clean.join(", ")
}

function CountByPriority(tickets: Ticket[]) -> map<string, int> {
  let counts: map<string, int> = {};
  counts.set("high", 0);
  counts.set("other", 0);

  for (let ticket in tickets) {
    if (ticket.priority == Priority.High) {
      counts.set("high", (counts.get("high") ?? 0) + 1);
    } else {
      counts.set("other", (counts.get("other") ?? 0) + 1);
    };
  }

  counts
}
```

Prefer `array.at(i)` and `map.get(key)` when absence is normal. Use direct indexing only when an out-of-bounds index or missing key should panic.

Use `baml.unstable.string(value)` for string conversion unless `baml describe` shows a newer stable API. Do not assume regex, numeric parsing, byte length, UUID, base64, crypto, or date/time helpers exist; check `baml describe`.

## JSON Pattern

Use native JSON helpers. Do not hand-roll JSON escaping or create dummy LLM functions just to parse JSON.

```baml
class Email {
  id string
  from string
  subject string
  body string
}

function LoadEmails(raw: string) -> Email[] {
  baml.json.decode_str<Email[]>(raw)
}

function EncodeTickets(tickets: Ticket[]) -> string {
  baml.json.stringify_pretty(baml.json.encode(tickets))
}

function ReadOptionalString(obj: map<string, json>, key: string) -> string? {
  let value = obj.get(key) ?? null;

  match (value) {
    let s: string => s,
    _ => null,
  }
}
```

Expected helpers:

- `baml.json.parse(s) -> json`
- `baml.json.stringify(j) -> string`
- `baml.json.stringify_pretty(j) -> string`
- `baml.json.encode<T>(value) -> json`
- `baml.json.decode<T>(j) -> T`
- `baml.json.decode_str<T>(s) -> T`

Keep wire data as `json` at the boundary, then decode into classes/enums before domain logic.

## LLM Function Pattern

Define typed inputs, typed outputs, a client, and a prompt. Let the return type drive parsing.

```baml
client<llm> FastOpenAI {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

class Intent {
  kind "billing" | "support" | "sales" | "spam" | "other"
  confidence float
  rationale string
}

class ReplyDraft {
  subject string
  body string
  needs_human bool
}

function ClassifyEmail(email: Email) -> Intent {
  client FastOpenAI
  prompt #"
    Classify the user's email.

    From: {{ email.from }}
    Subject: {{ email.subject }}
    Body:
    {{ email.body }}

    {{ ctx.output_format }}
  "#
}

function DraftReply(email: Email, intent: Intent) -> ReplyDraft {
  client FastOpenAI
  prompt #"
    Draft a concise support reply.

    Intent: {{ intent.kind }}
    Reasoning: {{ intent.rationale }}

    Original email:
    {{ email.body }}

    Preserve quoted IDs and placeholders exactly.
    {{ ctx.output_format }}
  "#
}
```

Rules:

- Always include `{{ ctx.output_format }}` for structured outputs.
- Prefer classes, enums, and literal unions over free-form JSON.
- Optional fields are good for discriminated LLM outputs when a fixed object is easier for the model than a wide union.
- Put exact token-preservation rules in the prompt when strings like `{name}`, `(HOLD)`, IDs, or markup must survive byte-identically.
- Do not hardcode API keys. Use `env.OPENAI_API_KEY`, `env.ANTHROPIC_API_KEY`, or project-specific env vars.
- Provider options differ. For example, Anthropic may require `max_tokens`; inspect working clients or provider docs.

## Prompt Strings And Jinja

LLM prompts use BAML block strings with Jinja templates:

```baml
function SummarizeTickets(tickets: Ticket[]) -> string {
  client FastOpenAI
  prompt #"
    Summarize these tickets.

    {% for ticket in tickets %}
    - {{ ticket.id }}: {{ ticket.title }} ({{ ticket.priority }})
    {% endfor %}

    {{ ctx.output_format }}
  "#
}
```

Prompt rules for agents:

- Use block strings delimited by `#"` and `"#` for multi-line prompts.
- Use `{{ value }}` to insert BAML variables, fields, and simple expressions.
- Use `{% if condition %}...{% endif %}` and `{% for item in items %}...{% endfor %}` for prompt control flow.
- Use `{# comment #}` for comments inside the prompt template.
- Use `{{ ctx.output_format }}` whenever the return type is structured or constrained. It injects the schema/instructions BAML needs for reliable parsing.
- Keep business rules and token-preservation constraints in natural language near the relevant input.
- Do not manually paste JSON schemas into prompts unless you are intentionally overriding the generated output format.

## Pipeline Pattern

Use BAML to express typed stages. Keep each stage narrow and inspectable.

```baml
class TriageResult {
  email Email
  intent Intent
  draft ReplyDraft
  score int
}

function JudgeDraft(email: Email, draft: ReplyDraft) -> int {
  client FastOpenAI
  prompt #"
    Score this reply from 1 to 5.

    Email:
    {{ email.body }}

    Reply:
    {{ draft.body }}

    {{ ctx.output_format }}
  "#
}

function TriageOne(email: Email) -> TriageResult {
  let intent = ClassifyEmail(email);
  let draft = DraftReply(email, intent);
  let score = JudgeDraft(email, draft);

  TriageResult {
    email: email,
    intent: intent,
    draft: draft,
    score: score,
  }
}

function TriageBatch(raw_json: string) -> TriageResult[] {
  let emails = LoadEmails(raw_json);
  let results: TriageResult[] = [];

  for (let email in emails) {
    results.push(TriageOne(email));
  }

  results
}
```

This shape worked well in real agent runs: typed load at the boundary, small LLM stages, pure orchestration, typed results.

## `$parse` Companion

LLM functions expose compiler-generated companion functions. `$parse` parses an existing model response into the function's return type without making an LLM call.

```baml
function ExtractTicket(text: string) -> Ticket {
  client FastOpenAI
  prompt #"
    Extract one ticket.
    {{ text }}
    {{ ctx.output_format }}
  "#
}

function ParseCachedTicket(raw_model_output: string) -> Ticket {
  ExtractTicket$parse(raw_model_output)
}
```

Use `$parse` for cached model responses, fixtures, and parse-only tests. Use `baml.json.decode<T>` for ordinary non-LLM JSON data. Do not define your own `$` names.

## Generated Clients

Use `baml generate` when BAML functions should be called from an application written in Python, TypeScript, Go, Ruby, Rust, Java, or another host language. Add a `generator` block, usually in `baml_src/generators.baml`, then run `baml generate`; it writes a `baml_client` package that exposes your BAML functions as normal typed host-language functions and converts BAML classes/enums into native host types.

```baml
generator target {
  // Common native targets: "python/pydantic", "typescript", "go", "ruby/sorbet".
  // For Rust/Java/C#/PHP/etc., use OpenAPI generation when that is the supported route.
  output_type "python/pydantic"

  // Relative to baml_src/.
  output_dir "../"

  // Pick the top-level sync or async export where the target supports both.
  default_client_mode "sync"
}
```

Then call BAML from application code instead of shelling out to `baml run`:

```python
from baml_client.sync_client import b

ticket = b.ExtractTicket("refund request text")
print(ticket.title)
```

```ts
import { b } from "./baml_client";

const ticket = await b.ExtractTicket("refund request text");
console.log(ticket.title);
```

```go
import (
  "context"
  b "example.com/myapp/baml_client"
)

ticket, err := b.ExtractTicket(context.Background(), "refund request text")
```

Use generated clients for product code. Use `baml run` for local debugging, scripts, demos, and CI checks.

## Match And Errors

Use `match` for enums, literals, unions, and type narrowing.

```baml
function PriorityWeight(priority: Priority) -> int {
  match (priority) {
    Priority.Low => 1,
    Priority.Medium => 2,
    Priority.High => 3,
  }
}

function JsonSummary(value: json) -> string {
  match (value) {
    null => "null",
    let s: string => "string:" + s,
    let n: int => "int:" + baml.unstable.string(n),
    let xs: json[] => "array:" + baml.unstable.string(xs.length()),
    let obj: map<string, json> => "object:" + baml.unstable.string(obj.length()),
    _ => "other",
  }
}
```

Throw typed values when callers should handle failures.

```baml
class ParseError {
  message string
}

function RequireNonEmpty(value: string) -> string throws ParseError {
  let trimmed = value.trim();

  if (trimmed.length() == 0) {
    throw ParseError { message: "expected non-empty string" };
  };

  trimmed
}

function SafeTitle(value: string) -> string {
  RequireNonEmpty(value) catch (e) {
    _: ParseError => "untitled",
  }
}
```

`catch` is an expression. Its arms must produce a type compatible with the success path. Unhandled throw types continue upward. Avoid panics for normal control flow; prefer `map.get`, `array.at`, typed throws, and explicit null handling.

## Tests

Write deterministic tests for pure logic and parse fixtures. Keep live LLM calls out of normal unit tests unless the project explicitly requires a smoke test.

```baml
testset "triage" {
  test "load emails fixture" {
    let raw = #"[{"id":"1","from":"a@example.com","subject":"Hi","body":"Need help"}]"#;
    let emails = LoadEmails(raw);

    assert.equal(emails.length(), 1);
    assert.equal(emails[0].subject, "Hi");
  }

  test "parse cached ticket" {
    let raw = #"{"id":"T-1","title":"Refund","priority":"High","tags":["billing"],"metadata":null}"#;
    let ticket = ExtractTicket$parse(raw);

    assert.equal(ticket.id, "T-1");
    assert.equal(ticket.priority, Priority.High);
  }
}
```

Testing rules:

- Use many small `testset` cases.
- Use `$parse` or `baml.json.decode<T>` over committed fixture strings.
- Use `baml test -i "suite::case"` for focused iteration.
- If bridges or providers make a full suite slow/flaky, add smaller filtered commands for CI.

## Files, HTTP, Shell, Env

Use stdlib APIs directly when available.

```baml
function ReadText(path: string) -> string {
  baml.fs.read(path)
}

function WriteReport(path: string, results: TriageResult[]) -> int {
  baml.fs.write(path, baml.json.stringify_pretty(baml.json.encode(results)))
}

function FetchJson(url: string) -> json {
  let resp = baml.http.fetch(url);

  if (!resp.ok()) {
    throw "HTTP " + baml.unstable.string(resp.status_code);
  };

  baml.json.parse(resp.text())
}

function RequiredKey(name: string) -> string {
  baml.env.get_or_panic(name)
}
```

Use `baml.sys.shell` sparingly. It is useful for temporary bridges, but repeated shell calls can dominate runtime and make tests brittle. Prefer one structured bridge call over many tiny shell calls.

## Bridge Pattern

Use a host bridge only when the task needs a capability BAML does not yet provide well: databases, sockets, cryptography, advanced byte processing, browser/server runtimes, complex vector operations, or a missing stdlib primitive.

The bridge should be thin, generic, and protocol-driven.

```baml
class BridgeRequest {
  op "query" | "execute" | "sign"
  payload json
}

class BridgeResponse {
  ok bool
  data json?
  error string?
}

function CallBridge(req: BridgeRequest) -> BridgeResponse {
  let input = baml.json.stringify(baml.json.encode(req));
  let request_path = ".baml_bridge_request.json";

  baml.fs.write(request_path, input);
  let output = baml.sys.shell("python3 bridge.py " + request_path);

  baml.json.decode_str<BridgeResponse>(output)
}
```

Bridge rules:

- Keep domain policy in BAML; keep external capability in the bridge.
- Do not hardcode one component, route, table, or test case in the bridge.
- Use JSON or a documented line protocol. Avoid fragile string scraping.
- When the host calls BAML, use `baml run --output json`.
- Surface bridge errors with stderr, exit code, and context.
- Add tests for BAML-side protocol classes and bridge happy/error paths.

## Design Defaults

Prefer:

- typed classes/enums/literal unions for LLM schemas
- small typed LLM stages composed by BAML functions
- `json` and `baml.json.*` at external boundaries
- `map.get`, `array.at`, and explicit null handling
- deterministic tests for parsing, validation, scoring, and formatting
- a single generic bridge for missing platform capabilities
- `baml describe`, `baml run --list`, `baml test --list`, and formatting as the normal agent loop
- `baml generate` when the host app should call BAML through a typed client

Avoid:

- free-form prompt contracts where typed output would work
- hand-written JSON concatenation
- per-call shell-outs inside tight loops
- shelling out to `baml run` from product code when a generated client is available
- component-specific or schema-specific bridge code
- live LLM calls in deterministic CI tests
- assuming TypeScript methods, regex behavior, or stdlib modules exist without checking
- blocking automation with `io.input`
- claiming completion without running the acceptance command

## When To Use A Bridge

BAML works well for typed structured-output calls, multi-stage LLM pipelines, parse-only tests, validation/scoring logic, line-oriented string processing, small CLIs, and typed APIs over external systems.

Reach for a bridge when the problem is mostly long-lived network serving, database connection lifetime, filesystem watching, cryptography, binary protocols, high-concurrency fanout before BAML exposes the needed runtime primitive, or stdlib functionality that `baml describe` confirms is missing.

Keep the BAML surface clean even when you bridge. The bridge should be replaceable when the stdlib catches up.
