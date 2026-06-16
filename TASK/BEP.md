---
title: "BEP-023: Test and asserts"
status: implemented
version: 7
created: 2026-03-23
updated: 2026-04-23
shepherds:
  - aaronvg
---
# BEP-023: Test and asserts

# BEP-023: Test and asserts

# BEP-023: BAML Test Syntax and Assertion Model

## Summary

BAML tests today are declarative blocks that pair a function with static arguments. They support compile-time prompt rendering but lack assertions, parameterization, external data, and any concept of evaluation.

This proposal redesigns BAML tests to support:

- **Imperative test bodies** with hard assertions
- **Dynamic test generation** via `testset` groups and `for` loops
- **Shared setup** via `let` bindings in testset bodies
- **Custom test runners** via the `with` clause
- **LLM-as-judge scoring** via regular BAML function calls

```baml
test "smoke test" {
  let result = Translate("French", "hello")
  assert.contains(result, "bonjour")
}
```

A data-driven testset with shared setup and nested tests:

```baml
class TranslationCase {
  language string
  text string
  expected string
}

testset "translations" {
  let cases = [
    TranslationCase { language: "French", text: "hello", expected: "bonjour" },
    TranslationCase { language: "Spanish", text: "hello", expected: "hola" },
  ]

  for (let case in cases) {
    test "translate " + case.language {
      let result = Translate(case.language, case.text)
      assert.contains(result, case.expected)
    }
  }
}
```

The design follows two principles:

1. **No new DSL.** LLM-as-judge is just a function call. Data sources are functions. Tests use the same expression language as the rest of BAML.
2. **Explicit phase boundaries.** Importing a module never runs user code. Collection and execution are separate phases with clear rules about what runs when.

## Motivation

### The Problem

You're testing a translation function. You have 50 input cases across 3 languages. Today in BAML, that means writing 150 nearly identical test blocks by hand:

```baml
test TranslateTest1 {
  functions [Translate]
  args {
    language "French"
    text "hello"
  }
}

test TranslateTest2 {
  functions [Translate]
  args {
    language "French"
    text "goodbye"
  }
}

// ... 148 more
```

If you want to compare GPT-4o vs Claude on the same inputs, you double that. And after all that work, one flaky LLM response fails your CI -- even though the function works 95% of the time. There's no way to say "this test passes if it works 3 out of 5 times."

Here's what's missing:

| Gap | Impact |
|:----|:-------|
| No parameterization | 10 inputs = 10 test blocks |
| No external data | All test data must be inlined in source |
| No test matrix | Can't compare the same test across GPT-4o vs Claude |
| No multi-run evaluation | LLM output is nondeterministic; single runs are meaningless for quality measurement |

### Why LLM Testing Is Different

Traditional software testing is straightforward. Call a function, check the output: `assertEqual(add(2, 3), 5)`. The function is deterministic, the answer is unambiguous, and one passing test means the code works.

Now consider how you'd test Face ID.

You can't write `assertEqual(faceID(myFace), true)` and call it done. One passing case tells you almost nothing. What you actually need to know is: does it unlock for the right person wearing sunglasses? In low light? After a haircut? Does it stay locked when a sibling tries? When someone holds up a photo?

Each of those is a *scenario* -- a slice of a larger dataset. And within each scenario, no single case matters much. What matters is the *rate*: across 10,000 twins attempts, how often does it incorrectly unlock? That's the metric. And you're tracking several of them independently -- false positive rate, correct unlock rate, latency -- because a change that speeds up recognition but lets more impostors through isn't simply "pass" or "fail." It's a tradeoff that needs to be visible.

This is the same problem BAML functions face.

**Nondeterminism is fundamental.** Agent workflows produce variable output across runs. A single pass/fail result from one run is a sample, not a proof -- you need repeated runs and statistical aggregation to measure quality meaningfully.

**Quality is multi-dimensional.** A translation can be fluent but inaccurate, or accurate but stilted. An extraction function might get 9 out of 10 fields right. A single pass/fail verdict loses this information.

**Evaluation IS testing.** The line between "test" and "evaluation" has blurred. Frameworks like Braintrust, LangSmith, and DeepEval all converge on the same pattern: run a function across a dataset, score outputs on multiple dimensions, track scores as metrics over time. BAML should support this pattern natively, with BAML's unique advantages: type-safe functions, compile-time prompt rendering, and declarative module semantics.

**Metrics drive optimization.** In systems like DSPy, metrics aren't just for pass/fail -- they're objective functions used to optimize prompts automatically. BAML's test metrics should be usable for the same purpose.

### Collection and Execution

Any test runner must answer "what tests exist?" before running them. This matters for BAML because:

1. **Playground integration.** The playground needs to show a test tree and render prompts *before* any test runs.
2. **Filtering.** `baml test --filter "parser/*"` needs to know all test names before deciding which to run.
3. **Cost estimation.** Before running an eval that makes 800 LLM calls, you might want to see a cost estimate.
4. **IDE integration.** The LSP needs to show test names, run buttons, and code lenses.

BAML makes this boundary explicit: `test` bodies are captured during collection but executed later. `testset` bodies execute during collection to discover tests. This two-phase model means you always know the full test tree before anything runs.

## Proposed Design

### Tests

Tests are blocks with executable bodies. The old declarative syntax is supported for backwards compatibility and can be auto-migrated (see [Legacy Syntax and Migration](#legacy-syntax-and-migration)).

```baml
test "translates hello to French" {
  let result = Translate("French", "hello")
  assert.not_null(result)
  assert.contains(result, "bonjour")
}
```

#### Test Name Expressions

Test names accept any expression that evaluates to a string:

```baml
// String literal
test "simple test" { ... }

// Raw string literal
test #"test with \"quotes\""# { ... }

// Concatenation
test "prefix_" + "suffix" { ... }

// Variable (from loop binding)
test case_name { ... }
```

There is no `{var}` template interpolation in the name position. Use `+` concatenation for dynamic names:

```baml
testset "translations" {
  let languages = ["French", "Spanish", "Japanese"]

  for (let lang in languages) {
    test "translate " + lang {
      let result = Translate(lang, "hello")
      assert.not_null(result)
    }
  }
}
```

### Testsets

`testset` declares a group of tests. Its body executes during collection to discover tests; `test` bodies inside are captured for later execution.

```baml
testset "basic group" {
  test "first" {
    assert.is_true(true)
  }
  test "second" {
    assert.is_true(true)
  }
}
```

#### Nesting

Testsets nest to organize tests into logical groups:

```baml
testset "URL parser" {
  testset "valid URLs" {
    test "with scheme" {
      let result = ParseURL("https://example.com")
      assert.not_null(result)
    }
    test "without scheme" {
      let result = ParseURL("example.com")
      assert.not_null(result)
    }
  }
  testset "invalid URLs" {
    test "empty string" {
      let result = ParseURL("")
      assert.equal(result, null)
    }
  }
}
```

#### Shared Setup

`let` bindings in a testset body are shared across all tests in the group:

```baml
testset "with shared setup" {
  let base_url = "https://api.example.com"
  let timeout = 5000

  test "uses setup vars" {
    assert.not_null(base_url)
    assert.equal(timeout, 5000)
  }

  test "also uses setup" {
    assert.contains(base_url, "example")
  }
}
```

#### Dynamic Test Generation

Testset bodies support `for` loops and `if` conditions to generate tests dynamically:

```baml
testset "dynamic tests" {
  let cases = ["hello", "world", "foo"]

  for (let case in cases) {
    test "check " + case {
      assert.not_null(case)
    }
  }
}
```

Combined with nesting, this lets you generate entire testsets from external data. For example, suppose you have a sentiment classifier and want to test it across multiple categories, each with its own set of examples:

```baml
testset "sentiment classifier" {
  let categories = ["positive", "negative", "neutral"]

  for (let category in categories) {
    // Each category gets its own testset with independent pass/fail reporting
    testset "category: " + category {
      // Load labeled examples for this category from a CSV
      let examples = csv("./testdata/sentiment_" + category + ".csv")

      for (let example in examples) {
        test example.text {
          let result = ClassifySentiment(example.text)
          assert.equal(result.label, category)
        }
      }
    }
  }
}
```

#### Synthetic Test Generation

Since testset bodies can call any BAML function, you can use an LLM to generate test cases. This is useful when you want broad coverage but don't want to hand-write every example:

```baml
class TestCase {
  input string
  expected_label string
}

function GenerateTestCases(category: string, count: int) -> TestCase[] {
  client GPT4oMini
  prompt #"
    Generate {{ count }} realistic examples of {{ category }} text
    for testing a sentiment classifier. Include tricky edge cases.

    {{ ctx.output_format }}
  "#
}

testset "synthetic sentiment tests" {
  let categories = ["positive", "negative", "sarcastic"]

  for (let category in categories) {
    testset category {
      // Ask an LLM to generate test cases during collection
      let cases = GenerateTestCases(category, 10)

      for (let case in cases) {
        test case.input {
          let result = ClassifySentiment(case.input)
          assert.equal(result.label, case.expected_label)
        }
      }
    }
  }
}
```

Because testset bodies run during collection, `GenerateTestCases` is called before any test executes -- the generated cases appear in the test tree and can be filtered, inspected, and cost-estimated like any other test.

### The `with` Clause (Test Runners)

Agent workflows are nondeterministic -- running a test once doesn't tell you much. You often want to run the same test body multiple times, retry on transient failures, or decide whether a *suite* passes based on a pass rate rather than all-or-nothing. The `with` clause attaches a **runner** to a `test` or `testset` that controls how it executes. You can think of it like a decorator that wraps your test with extra behavior -- retries, repetition, or pass-rate thresholds -- without changing the test body itself.

#### Quorum

Run a test N times, pass if at least M runs pass:

```baml
test "translate hello" with testing.Quorum(5, 3) {
  let result = Translate("French", "hello")
  assert.not_null(result)
  assert.contains(result, "bonjour")
}
```

This runs the test body 5 times and reports pass if at least 3 runs succeed. Each run's metrics are aggregated into a single report with mean, stddev, and pass rate.

#### Retry: for transient failures

Different from Quorum -- Retry stops on the first success:

```baml
test "flaky integration" with testing.Retry(3) {
  let result = FlakyExternalAPI("hello")
  assert.not_null(result)
}
```

Quorum measures nondeterministic *quality distributions*. Retry handles transient *infrastructure failures*. These are not the same thing.

#### PassRate on testsets

`with` on a `testset` controls how the entire suite is evaluated. The most common pattern: pass the suite if a threshold percentage of children pass:

```baml
testset "training eval" with testing.PassRate(0.7) {
  let cases = csv("./testdata/training.csv")

  for (let case in cases) {
    test "case " + case.id {
      let result = RunModel(case.input)
      assert.is_true(JudgeQuality(case.input, result) >= 0.8)
    }
  }
}
```

The testset passes if at least 70% of the leaf tests pass. This is the standard ML evaluation pattern -- each training run has different subsets pass, and overall success is defined by a threshold over the collection.

#### Composing test and testset runners

Test runners and testset runners are orthogonal -- they compose naturally:

```baml
testset "training eval" with testing.PassRate(0.7) {
  let cases = csv("./testdata/training.csv")

  for (let case in cases) {
    test "case " + case.id with testing.Quorum(5, 3) {
      let result = RunModel(case.input)
      assert.is_true(JudgeQuality(case.input, result) >= 0.8)
    }
  }
}
```

The execution flow:

1. Each leaf test runs through its runner (Quorum: 5 runs, pass if 3+ pass) -- produces a per-test verdict
2. All per-test verdicts are collected
3. The testset runner (PassRate) decides the suite verdict -- pass if 70%+ of tests passed

#### Writing your own runners

Runners are ordinary BAML functions. A `testing.TestRunner` takes a thunk (a `() -> testing.TestReport` that runs the test once) and returns a new thunk that wraps it with custom behavior. For example, here's how you'd implement Retry -- try up to N times, pass on the first success:

```baml
function Retry(max_attempts: int) -> testing.TestRunner {
  // A TestRunner takes a run-once thunk and returns a wrapped thunk
  (run_once: testing.TestReportThunk) -> testing.TestReportThunk {
    return () -> testing.TestReport {
      let last_result: testing.TestReport? = null
      for (let i = 0; i < max_attempts; i += 1) {
        let result = run_once()
        if (result.outcome == "pass") {
          return result
        }
        last_result = result
      }
      // All attempts failed -- return the last result
      return last_result ?? testing.TestReport {
        outcome: "fail",
        runs: [],
      }
    }
  }
}
```

No special syntax or registration -- if you know how functions and lambdas work, you know how runners work.

#### Testset runners with scheduling control

Testset runners can also control execution order. For example, `Sequential` runs children one at a time instead of in parallel:

```baml
testset "ordered pipeline" with testing.Sequential() {
  test "setup" { ... }
  test "migrate" { ... }
  test "validate" { ... }
  test "teardown" { ... }
}
```

And `FailFast` stops the suite on the first child failure:

```baml
testset "smoke tests" with testing.FailFast() {
  test "health check" { ... }
  test "auth flow" { ... }
  test "core feature" { ... }
}
```

#### No lifecycle hooks

There is no `before_each`, `after_each`, or `after_all`. Tests should be self-contained and readable top-to-bottom. When setup and teardown live in hooks defined elsewhere, a reader looking at a test body can't understand what it does without mentally reconstructing the hidden context.

Repeated setup is just a function call:

```baml
function fresh_test_db() -> TestDB {
  let db = createTestDB()
  db.reset()
  return db
}

testset "db tests" {
  let cases = csv("./testdata/cases.csv")

  for (let case in cases) {
    test "case " + case.id {
      let db = fresh_test_db()
      let result = RunFlowWithDB(db, case.input)
      assert.not_null(result)
    }
  }
}
```

### Assertions

The `assert.*` package provides four assertion functions. Every BAML package has access to them -- no import needed.

```baml
assert.is_true(condition: bool) -> null
assert.not_null(value: unknown?) -> null
assert.equal(actual: unknown, expected: unknown) -> null
assert.contains(haystack: string, needle: string) -> null
```

All assertions abort the test on failure. The test runner catches the failure and marks the test as failed.

```baml
test "assertions demo" {
  let x = 42
  assert.equal(x, 42)

  let name = "hello world"
  assert.contains(name, "world")
  assert.is_true(x > 0)

  let result = SomeFunction()
  assert.not_null(result)
}
```

There is no standalone `assert <expr>` statement form. Assertions are always package-qualified function calls: `assert.is_true(...)`, `assert.equal(...)`, etc.

### Control Flow in Test Bodies

All standard BAML control flow works inside test and testset bodies:

- **`let` bindings** -- declare local variables
- **`for` loops** -- C-style (`for (let i = 0; i < n; i += 1)`) and iterator-style (`for (let case in cases)`)
- **`if` conditions** -- conditional logic
- **Function calls** -- call any BAML function

```baml
test "control flow" {
  let items = [1, 2, 3, 4, 5]
  let sum = 0

  for (let item in items) {
    sum = sum + item
  }

  assert.equal(sum, 15)

  if (sum > 10) {
    assert.is_true(true)
  }
}
```

### Parallel Execution

Tests run in parallel by default. Serial execution is available when needed via the `Sequential` testset runner:

```baml
testset "migration steps" with testing.Sequential() {
  test "step 1: create tables" { ... }
  test "step 2: migrate data" { ... }
  test "step 3: validate" { ... }
}
```

### Identity and Naming

Every test has an identity path using `/` as the separator. The path includes the namespace, testset chain, and test name:

```
namespace / testset / testset / test_name
```

Examples:

```
test "smoke"                                        →  smoke
testset "translations" > test "French"              →  translations/French
testset "parser" > testset "fixtures" > test "001"  →  parser/fixtures/001

// In ns_billing/tests.baml:
testset "reports" > test "monthly"                  →  billing/reports/monthly

// In ns_e2e/smoke.baml:
test "translate"                                    →  e2e/translate
```

Root namespace tests have no prefix. Tests in `ns_billing/` are prefixed with `billing/`.

Rules:
- Test and testset names must not contain `/` (compile error for literals, collection error for dynamic names)
- Path order follows source order, then iteration order in loops

#### Duplicate names

Duplicate identity paths are allowed. When duplicates occur, the runner disambiguates by appending `#N` (0-indexed) — the same strategy Go uses for duplicate `t.Run()` subtests:

```baml
testset "parser" {
  test "valid" { ... }    // parser/valid
  test "valid" { ... }    // parser/valid#1
  test "valid" { ... }    // parser/valid#2
}
```

This matters most for generated tests where names might collide:

```baml
testset "cases" {
  for (let case in cases) {
    test case.category {        // if two cases share a category,
      ...                       // they become "positive" and "positive#1"
    }
  }
}
```

#### Filtering

Filters use glob-style matching against identity paths. `*` matches within a segment, `**` matches across segments:

```bash
baml test "translations/*"                             # direct children
baml test "translations/**"                            # all descendants
baml test "**/monthly"                                 # "monthly" at any depth
baml test "billing/**" "auth/**"                       # OR-combined
baml test --exclude "**/slow*"                         # exclude a pattern
baml test --exclude "**/slow*" --exclude "**/flaky*"   # stacked excludes
baml test "billing/**" --exclude "**/slow*"            # (billing) AND NOT slow
```

- Multiple positional args OR together to build the candidate set
- `--exclude` removes from the candidate set (stacked excludes all apply)
- No positional args means "everything"

### Namespaces and Tests

Tests can call functions from any namespace. Within a namespace, no qualification is needed. Cross-namespace access uses the namespace name:

```baml
// In ns_e2e/smoke.baml
testset "smoke" {
  test "translate" {
    let result = root.Translate("French", "hello")      // root namespace
    assert.contains(result, "bonjour")
  }
  test "billing" {
    let report = billing.BillingReport("test-123")      // billing namespace
    assert.not_null(report)
  }
}
```

Namespace filtering is just a glob pattern on the identity path — no special flag needed:

```bash
baml test "billing/**"          # all tests in billing namespace
baml test "e2e/**"              # all tests in e2e namespace
```

### CLI

```bash
baml test                              # collect + run all tests
baml test [globs...]                   # collect all, run matching
baml test --collect                    # run collection phase, print test names
baml test --collect --tree             # run collection phase, print test tree
```

#### Filtering

Three mechanisms, applied in order: globs → exclude → filter.

**Globs** match against identity paths. `*` matches within a segment, `**` matches across segments. Multiple globs OR together:

```bash
baml test "translations/*"                         # direct children
baml test "billing/**" "auth/**"                    # OR-combined
baml test "**/monthly"                              # "monthly" at any depth
```

**`--exclude`** removes from the candidate set. Multiple excludes stack:

```bash
baml test --exclude "**/slow*"
baml test --exclude "**/slow*" --exclude "**/flaky*"
baml test "billing/**" --exclude "**/slow*"
```

**`--filter`** takes a BAML expression that evaluates to a `(string) -> bool` closure. The expression is type-checked before any tests run — type errors are caught immediately, not mid-execution:

```bash
# Inline closure
baml test --filter '(name) => name.contains("French")'

# Stateful — first 100 tests
baml test --filter '
  let n = 0
  (name) => { n += 1; n <= 100 }
'

# Reference a function defined in your project
baml test --filter 'MyNightlyFilter'
```

Since `--filter` is a BAML expression, you can define reusable filter factories as regular functions:

```baml
function First(n: int) -> (string) -> bool {
  let count = 0
  return (name) => { count += 1; count <= n }
}

function MatchesAny(patterns: string[]) -> (string) -> bool {
  return (name) => patterns.any(p => name.contains(p))
}
```

```bash
baml test --filter 'First(100)'
baml test --filter 'MatchesAny(["billing", "auth"])'
```

All three compose. Globs build the candidate set, `--exclude` removes from it, `--filter` narrows further:

```bash
baml test "billing/**" --exclude "**/slow*" --filter '(name) => name.contains("v2")'
```

No positional args means "everything" — so `baml test --filter '(name) => ...'` filters across all tests.

#### Execution control

```bash
baml test --fail-fast                  # stop after first failure
baml test --jobs 8                     # cap parallel workers (default: num CPUs)
baml test --timeout 30s               # per-test timeout
```

#### Output

```bash
baml test --output pretty              # human-readable (default)
baml test --output json                # structured JSON for CI/agents
baml test --output junit               # JUnit XML for CI systems
```

#### Exit codes

| Code | Meaning |
|:-----|:--------|
| `0` | All tests passed |
| `1` | One or more tests failed |
| `2` | Collection error (malformed data, duplicate names, parse error) |

### LLM-as-Judge

There is no special scorer type -- a judge is a regular BAML function:

```baml
class QualityScore {
  value float
  reason string?
}

function JudgeQuality(original: string, translation: string) -> QualityScore {
  client GPT4oMini
  prompt #"
    Rate this translation from 0.0 to 1.0. Be strict.

    Original: {{ original }}
    Translation: {{ translation }}

    {{ ctx.output_format }}
  "#
}
```

Used in a test, the judge result feeds directly into an assertion:

```baml
test "translate quality" {
  let result = Translate("French", "hello")
  let score = JudgeQuality("hello", result)
  assert.is_true(score.value >= 0.7)
}
```

The judge is a function whose prompt is renderable in the playground -- you can see exactly what the judge will see, with actual test output filled in, before spending tokens.

### Legacy Syntax and Migration

The old declarative syntax (`functions [...]` / `args { ... }`) continues to parse and run -- existing tests don't break. But the language has one test form going forward: imperative tests with expression bodies.

Every declarative test has a mechanical imperative equivalent. The BAML formatter can automatically rewrite declarative tests into the new form:

```baml
// Before (declarative)
test TranslateTest {
  functions [Translate]
  args {
    language "French"
    text "hello"
  }
}

// After (imperative) -- produced by `baml format`
test "TranslateTest" {
  let result = Translate(language = "French", text = "hello")
}
```

Multi-function declarative tests expand to one test per function:

```baml
// Before
test MyTest {
  functions [Foo, Bar]
  args { x 1 }
}

// After
test "MyTest_Foo" {
  let result = Foo(x = 1)
}

test "MyTest_Bar" {
  let result = Bar(x = 1)
}
```

The formatter preserves the original test's behavior and gives teams a one-command migration path (`baml format`) when they're ready.

## Design Tradeoffs

### Why `test` and `testset` as keywords

BAML's `test "name" { }` syntax is directly inspired by Zig, which uses `test "description" { }` as a first-class language construct rather than a naming convention (Go's `TestX`) or attribute (Rust's `#[test]`). The keyword makes tests grep-able, parseable without type resolution, and visually distinct from production code.

`testset` is inspired by Julia's `@testset` macro, which groups related tests under a name and reports aggregate pass/fail. BAML makes it a keyword rather than a macro, but the semantics are similar: a named scope that contains tests and can nest.

### Why string names instead of identifiers

Most test frameworks use identifiers for test names: Go's `func TestParseURL(...)`, Rust's `fn test_parse_url()`. This forces names into the language's identifier rules — no spaces, no punctuation, often snake_case or CamelCase. The result is names that describe the implementation (`TestParseURL`) rather than the behavior (`"parses valid URLs"`).

Zig chose string names: `test "parse valid URLs" { }`. BAML follows the same approach. String names are:

- **Human-readable in output** — test reports read as prose, not code
- **Expression-compatible** — dynamic names via concatenation (`"translate " + lang`) work naturally
- **Free from identifier restrictions** — spaces, punctuation, and unicode are all valid

The tradeoff is that string names can't be referenced as symbols (no `@test_name` pointer). This is fine — tests are addressed by identity path in filters, not by programmatic reference.

### Why `testset` over Go's `t.Run()`

In Go, dynamic subtests require threading a `*testing.T` handle through every test function. Discovery and execution happen in one pass. BAML's `testset` avoids this by keeping the `test` keyword uniform everywhere -- same syntax at module scope, inside `testset`, inside `for` loops. No handle needed, and collection is separate from execution.

### Why expression names instead of template interpolation

Test names are expressions because the parser already has a full expression language. String concatenation (`"prefix " + var`) is explicit and composable. Once BAML supports string interpolation (e.g. `f"prefix {var}"`), test names will support it too -- but that's a language-wide feature, not something specific to tests.

### Why hard assertions only (for now)

The current `assert.*` package provides a simple, correct foundation: if something is wrong, the test fails immediately with a clear error. Soft assertions (where all checks run regardless of earlier failures) are a more complex system involving metric collection, aggregation, and reporting -- these are planned as a future extension (see Future Ideas).

### Why two-phase (collection vs execution)

The collection/execution boundary is explicit because BAML's primary use case -- LLM evaluation -- requires knowing what tests exist before running them. The playground needs a test tree, filtering needs names, and cost estimation needs a test count. Running user code during collection is a deliberate tradeoff that enables data-driven test generation.

### Why `with` is a contextual keyword

Making `with` a hard keyword would break existing code that uses `with` as a variable or field name. As a contextual keyword, it's only recognized in the specific position between a test/testset name expression and the body block.

## Considered Alternatives

### Lifecycle hooks (`before_each`, `after_each`, `after_all`)

Most test frameworks (Jest, pytest, Go's `t.Cleanup()`) provide hooks that run setup/teardown code around test bodies. We considered this and rejected it. Tests should be self-contained and readable top-to-bottom. When setup lives in a `before_each` defined 50 lines above the test body, a reader looking at a test can't understand what it does without mentally reconstructing the hidden context.

The alternative is simple: write a helper function, call it at the top of the test. Testset bodies already serve as "before_all" since they run during collection before any test executes.

### Soft assertions / `check.*` (metrics-as-assertions)

The original design included `check.*` -- soft assertions that always run and report as named metrics, even when earlier checks fail. `assert.*` would be hard (stop on failure), `check.*` would be soft (collect all results). This is the pattern used by Go's `assert` vs `require` in testify.

We deferred this. Hard assertions via `assert.*` are a simpler, correct foundation. The `with` clause and test runners (Quorum, PassRate) already provide the aggregation and multi-run patterns that motivate soft assertions in other frameworks. Metric collection may be added later once we understand the reporting and dashboard needs better (see Future Ideas).

### Decorator-based test modifiers (`@serial`, `@timeout`, `@retry`)

Rather than using decorators to modify test behavior, all execution control is handled through the `with` clause and test runners. `@serial` becomes `with testing.Sequential()`, `@retry(3)` becomes `with testing.Retry(3)`, `@repeat(5)` becomes `with testing.Quorum(5, 3)`. This avoids introducing a second mechanism for controlling execution and keeps runners as the single point of behavioral customization.

### Special scorer / dataset / eval keywords

Frameworks like Promptfoo and Braintrust have dedicated concepts for scorers, datasets, and evaluations. We considered dedicated syntax (`scorer`, `dataset`, `eval` keywords) and rejected it. An LLM-as-judge is a regular BAML function -- call it, assert on the result. A dataset is a function that returns an array. An eval is a testset with a PassRate runner. No new concepts needed. The existing primitives (`test`, `testset`, `with`, functions, `for` loops) compose to express any evaluation pattern.

### `before_all` as a keyword

Since `testset` bodies execute during collection, code written directly in the body *is* before_all -- it runs once before any test executes. A dedicated keyword would be redundant.

## Prior Art

| Feature | BAML | Zig | Julia | Go + testify | Rust | Swift Testing | Jest | pytest |
|:--------|:-----|:----|:------|:-------------|:-----|:-------------|:-----|:-------|
| Test declaration | `test "name" { }` | `test "name" { }` | `@test` | `func TestX(t *T)` | `#[test] fn` | `@Test func` | `test("name", fn)` | `def test_x():` |
| Grouping | `testset "name" { }` | N/A | `@testset "name"` | `t.Run("name", fn)` | `mod tests` | `@Suite struct` | `describe` | `class TestX:` |
| Hard assert | `assert.equal(...)` | `expect(a == b)` | `@test a == b` | `require.Equal` | `assert!()` | `#expect()` | `expect().toBe()` | `assert` |
| Parallel default | Yes | Yes (comptime) | No | Opt-in | Yes | Yes | Per-file | Opt-in |
| Discovery | Static + collection | Static | Runtime | Static | Static | Static | Runtime | Runtime |
| Data-driven | `for` in `testset` | Comptime loops | Loops in `@testset` | Table-driven | Proc macros | `@Test(arguments:)` | `test.each` | `@parametrize` |

## Future Ideas

### Metric Collection

The current system only has hard assertions -- a test either passes or panics. In practice, you often want to **collect named metrics** without aborting on failure -- quality scores, latency, cost, fluency, accuracy -- and aggregate them across a testset.

Metrics are also useful outside of tests. You might want to record metrics in production workflows, monitoring dashboards, or optimization loops. Because the scope of metrics extends beyond testing into arbitrary BAML code, we're designing them as a general-purpose language feature rather than a test-specific one. A future `metric.*` or similar API would enable:

```baml
// Hypothetical future syntax
test "translate quality" {
  let result = Translate("French", "hello")
  assert.not_null(result)  // hard gate -- must pass to continue

  let score = JudgeQuality("hello", result)
  metric.record("quality", score.value)  // collect, don't abort
  metric.record("length", len(result))

  // Still passes even if quality is low -- metrics are informational
}
```

CLI output could then show aggregated metrics:

```
Results: 49 passed, 1 failed (50 total)

Metrics:
  quality:  mean 0.82 +/- 0.11 (50 samples)
  length:   mean 12.4 +/- 3.2 (50 samples)
```

Note that multi-run evaluation -- running the same test N times to account for nondeterminism and reporting mean/stddev/pass-rate -- doesn't require any new test syntax. The execution side is already handled by `testing.Quorum`. What's missing is the reporting side: recording and aggregating metrics across those runs. Once metric collection exists as a general-purpose feature, it composes directly with the runners from this BEP:

```baml
// Hypothetical: Quorum + metrics compose into full multi-run evaluation
test "translate quality" with testing.Quorum(5, 3) {
  let result = Translate("French", "hello")
  let score = JudgeQuality("hello", result)
  metric.record("quality", score.value)
  assert.is_true(score.value >= 0.7)
}
// Each of the 5 runs records a quality metric.
// The report shows: pass rate 4/5, quality mean 0.84 +/- 0.09
```

### Data Source Builtins

Built-in functions for loading external test data: `csv("path")`, `load_file("path")`, and potentially `google_sheet(...)`, `s3(...)`. Currently test data must be inline literals or loaded via user-defined functions.

### CLI Enhancements

- `--output` format extensions (TAP, GitHub Actions annotations)
- `--watch` mode with incremental re-collection on file change
- Cost estimation before running expensive LLM evaluations
