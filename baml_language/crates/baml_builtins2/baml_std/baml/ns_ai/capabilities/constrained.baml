// Constrained decoding — a by-construction output guarantee (scenario 03).

//baml:llm_capability
/// Constrained (grammar/regex/choice) decoding with a by-construction guarantee
/// (scenario 03). Deliberately has NO default method: a token-level guarantee cannot be
/// faked on top of an unconstrained `.call`, so a provider either has real constrained
/// decoding (self-hosted vLLM/llama.cpp/Outlines) or it does not implement this at all.
/// A client that lacks it fails the capability `match` at runtime — the guarantee is a
/// runtime promise the type system can't lift (design gap B1).
interface Constrained requires Provider {
  function decode<T>(self, prompt: string, pattern: string) -> T
      throws root.errors.CallError | root.errors.UnknownError
}
