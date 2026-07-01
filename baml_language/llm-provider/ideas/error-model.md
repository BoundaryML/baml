# BAML LLM error model

How errors are typed and handled across packages: the universal wrapper (`baml`), the
capability interfaces (`ai`), the concrete provider errors (`openai`, `anthropic`), and the
app that catches them. Self-contained.

Dependency direction: **`app` → `openai`/`anthropic` → `ai` → `baml`**.

---

## `package baml` — the universal wrapper

```baml
// The channel every fallible method uses: a domain error E, OR the universal wrapper.
type ExtendUnknownError<E> = E | UnknownError;

// NON-GENERIC on purpose: one UnknownError type, so `E | UnknownError` collapses across
// layers instead of exploding into `UnknownError<A> | UnknownError<B> | ...`.
class UnknownError {
  data: unknown;        // the original thrown value, untouched
  message: string[];    // breadcrumb of context, accumulated as it bubbles up

  // Reassert the channel: a known T passes through; an UnknownError already wrapping a T is
  // unwrapped back to T; anything else is wrapped fresh.
  function from<T>(data: unknown) -> T | UnknownError {
    match data {
      T => return data;
      Self { data: let inner: T } => return inner;
      _ => return UnknownError { data: data, message: [] };
    }
  }

  // Same, but annotate context. Known errors are NOT annotated (they keep their identity);
  // only unknown / already-wrapped values accumulate a message.
  function with_message<T>(data: unknown, message: string) -> T | UnknownError {
    match data {
      T => return data;
      Self { data: let inner: T } => return inner;
      Self => { data.message.push(message); return data; }
      _ => return UnknownError { data: data, message: [message] };
    }
  }
}
```

## `package ai` — capability interfaces + the provider interface + combinators

One **independent** interface per capability (**not** a hierarchy), each with common
classification methods so callers can triage without knowing the concrete class.

```baml
interface CallError {
  function is_network_error(self) -> bool;
  function is_rate_limit(self) -> bool;
  function is_parse_error(self) -> bool;
}
interface StreamError   { function is_network_error(self) -> bool; /* ... */ }
interface ToolError     { /* ... */ }
interface RealtimeError { /* ... */ }

interface LlmProvider {
  function call(self, prompt: Prompt) -> Response
      throws baml.ExtendUnknownError<CallError>;
}

// Combinators live here too, and declare the SAME channel — no narrow / widen / forward.
class Fallback {
  members: LlmProvider[];
  implements LlmProvider {
    function call(self, prompt: Prompt) -> Response
        throws baml.ExtendUnknownError<CallError> { /* try each member in order */ }
  }
}
```

## `package openai` — a concrete provider + its concrete errors

A concrete error `implements` **whichever capability interfaces apply** (often several — the
same error can arise on the call path *and* mid-stream). The method declares the channel,
throws concrete errors, and normalizes anything foreign in a trailing `catch`.

```baml
class OpenAiRateLimitError {
  retry_after_secs: int;
  implements ai.CallError   { function is_rate_limit(self)    -> bool { return true; }
                              function is_network_error(self) -> bool { return false; }
                              function is_parse_error(self)   -> bool { return false; } }
  implements ai.StreamError { /* same error surfaces mid-stream */ }
}

class OpenAiProvider {
  implements ai.LlmProvider {
    function call(self, prompt: Prompt) -> Response
        throws baml.ExtendUnknownError<ai.CallError> {
      let raw = http.post(...);                          // foreign errors possible here
      if (rate_limited) { throw OpenAiRateLimitError { retry_after_secs: 5 }; }
      return parse(raw);
    } catch (e) {
      // Known ai.CallErrors pass through unchanged; anything else is wrapped + annotated.
      _ => throw baml.UnknownError.with_message(e, "OpenAI call failed");
    }
  }
}
```

## `package anthropic` — a different provider, different concrete error, SAME interfaces

```baml
class AnthropicOverloadedError {
  implements ai.CallError { function is_rate_limit(self)    -> bool { return true; }
                            function is_network_error(self) -> bool { return false; }
                            function is_parse_error(self)   -> bool { return false; } }
}

class AnthropicProvider {
  implements ai.LlmProvider {
    function call(self, prompt: Prompt) -> Response
        throws baml.ExtendUnknownError<ai.CallError> {
      let raw = http.post(...);
      if (overloaded) { throw AnthropicOverloadedError {}; }
      return parse(raw);
    } catch (e) {
      _ => throw baml.UnknownError.with_message(e, "Anthropic call failed");
    }
  }
}
```

## `package app` — the consumer catches across all of them

Runtime match, most-specific first: concrete (from a provider package) → interface (`ai`) →
`baml.UnknownError`. **The same catch works under a concrete *or* an interface-typed handle** —
concrete arms are runtime refinements even though the static error type is `ai.CallError | baml.UnknownError`.

```baml
let p: ai.LlmProvider = ai.Fallback {
  members: [openai.OpenAiProvider.new(), anthropic.AnthropicProvider.new()],
};

p.call(prompt) catch (e) {
  openai.OpenAiRateLimitError => backoff(e.retry_after_secs);     // concrete, when known
  ai.CallError                => { if (e.is_network_error()) retry(); else fail(e); }
  baml.UnknownError           => report(e.message);               // the escape hatch
}
```

---

## The rules
1. Every fallible method's channel is `baml.ExtendUnknownError<CapErr>` = `CapErr | baml.UnknownError`.
2. A throw is legal iff it `implements` the capability interface (in `ai`) **or** is routed through `baml.UnknownError`.
3. Normalize foreign errors with a trailing `catch (e) { _ => throw baml.UnknownError.with_message(e, "…") }`.
4. Combinators (in `ai`) declare the same channel — no per-combinator error-type gymnastics.
5. Consumers catch by runtime match, most-specific first.

## Why this shape
- **Nothing is unrepresentable.** `baml.UnknownError` is a universal escape hatch, so a method never fails to typecheck because a dependency throws something exotic.
- **Unions stay flat.** Non-generic `UnknownError` makes `CapErr | UnknownError` collapse across every package boundary.
- **Precision when you want it, ergonomics when you don't.** Hold a concrete provider → catch concrete types; hold `ai.LlmProvider` → catch the interface + use `is_network_error()` / `is_rate_limit()` / ….
- **No combinator plumbing.** Concreteness is recovered at the `catch`, so `ai.Fallback` / `ai.Retry` never forward or widen error types — the variance story disappears.
