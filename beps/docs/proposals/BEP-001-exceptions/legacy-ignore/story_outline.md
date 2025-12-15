# The Story of BAML Error Handling

## 1. The Reality of AI Engineering
*   **LLMs = Probabilistic Failure**: Unlike traditional code, every LLM call is a potential failure point (refusals, timeouts, hallucinations).
*   **The Need**: You *must* handle errors, but you shouldn't have to rewrite your code to do it.
*   **Our Proposal**: "Universal Catch". A simple, additive way to handle errors that works on functions, blocks, and expressions.

## 2. Design Evolution (Alternatives We Considered)
We'll evaluate each attempt against two common scenarios:
1.  **Declarative**: `ExtractResume` (defining an LLM call).
2.  **Imperative**: `ProcessBatch` (looping over items).

*   **The Base Cases**:
    ```rust
    // 1. Declarative
    function ExtractResume(text: string) -> Resume {
      client "gpt-4o"
      prompt #"..."#
    }

    // 2. Imperative
    function ProcessBatch(urls: string[]) -> Resume[] {
      // If this fails, we still want to process resumes!
      let aggregator = MetricsAggregator.new() 
      let results = []
      
      for (url in urls) {
        let resume = ExtractResume(url)
        aggregator.record(resume)
        results.append(resume)
      }
      return results
    }
    ```

*   **Attempt 1: The Classic `try/catch` Statement**
    *   *Imperative*: The "Hoisting Tax". To handle `aggregator` failing, you must declare `let aggregator;` outside, then `try { aggregator = ... }`, then check `if (aggregator) { ... }` inside the loop.
    *   *Declarative*: Forces wrapping the client definition (awkward).
    *   *Why we moved on*: Structural refactoring hurts both.
*   **Attempt 2: Result Types (`Result<T, E>`)**
    *   *Imperative*: `MetricsAggregator.new()` returns `Result`. You must `match` or `unwrap` it.
    *   *Declarative*: `ExtractResume` now returns `Result`.
    *   *Why we moved on*: Viral complexity.
*   **Attempt 3: Expression-Oriented Try (`let x = try { ... }`)**
    *   *Imperative*: `let aggregator = try { ... }` (Great for this case!).
    *   *Declarative*: `let resume = try { client ... }` (Confusing).
    *   *Why we moved on*: Great for imperative, but confusing for declarative.
*   **Attempt 4: Function Modifiers (`function ... try`)**
    *   *Both*: Syntactically awkward.
*   **Attempt 5: Wrapper Functions**
    *   *Both*: Boilerplate explosion.

## 3. The Solution: Universal Catch
*   **The Concept**: `catch` is an operator that can be attached to **ANY** block.
*   **The Unification**:
    *   Attach to a **Function** -> Implicit Try (Scope = Function Body).
    *   Attach to a **Block** -> Explicit Try (Scope = Block).
    *   Attach to an **Expression** -> Inline Catch.

## 4. Learn by Example
*   **Scenario 1: The "Prototype to Production" Flow**
    *   *Show*: A clean LLM function.
    *   *Action*: Append `catch` to handle a timeout. Zero indentation change.
*   **Scenario 2: The "Resilient Loop"**
    *   *Show*: A loop processing a batch of URLs.
    *   *Action*: Attach `catch` to the loop body to prevent one failure from crashing the batch.
*   **Scenario 3: The "Pipeline" (Inline Catch)**
    *   *Show*: A chain of operations where one step is optional.
    *   *Action*: Use `catch { _ => null }` on a single expression.
*   **Scenario 4: The "Complex Logic" (Imperative Try)**
    *   *Show*: A mix of safe and risky code in one function.
    *   *Action*: Use an explicit `try { ... }` block for the risky part to signal intent.
