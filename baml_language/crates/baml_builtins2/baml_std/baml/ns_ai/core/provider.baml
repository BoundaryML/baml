// The capability marker every provider implements and every capability `requires`.
//
// ── Declaring your own capability (the two-marker recipe) ────────────────────
// A capability is an interface that `requires Provider`, registered with
// `//baml:llm_capability`; its companion driver is a top-level generic
// function registered with `//baml:llm_companion(<suffix>)`:
//
//   //baml:llm_capability
//   interface Moderated requires baml.ai.Provider {
//     function call_moderated<T>(self, messages: baml.ai.ChatMessage[], policy: string) -> T
//         throws baml.errors.UnknownError
//   }
//
//   //baml:llm_companion(moderated)
//   function drive_moderated<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst, policy: string) -> T
//       throws baml.errors.Unsupported | baml.errors.UnknownError {
//     let p = client;
//     match (p) {
//       let m: Moderated => m.call_moderated<T>(baml.ai.prompt_to_messages(prompt), policy),
//       _ => throw baml.errors.Unsupported { message: "client's provider is not Moderated" },
//     }
//   }
//
// Every LLM function then grows `Foo$moderated(args…, policy, client?)`.
// Driver convention (checked by E0150–E0152): first params `client` +
// `prompt`; generic `T` is the return-type slot, `TPartial` (optional) the
// stream-expanded slot, other generics pass through; suffixes are unique
// session-wide. The driver is also directly callable:
// `drive_moderated<T>(client, Foo$render_prompt(…), policy)`.

/// The bare capability marker. Carries no interaction method — `call`/`call_with`
/// live on `HttpProvider`, `stream` on `Streaming`, etc. It is the type a client
/// returns and the bound every capability `requires`. Hosts the inherited combinator
/// factories every provider gets for free (see `combinators.baml`).
interface Provider {
  /// D2 effect marker: true iff invoking this provider commits external effects
  /// (server-stored state, billed jobs, live sessions) that a blind re-drive would
  /// duplicate. Combinators consult it and refuse-then-error instead of silently
  /// re-driving. An author assertion (default: safe).
  function is_effectful(self) -> bool throws never {
    false
  }

  function with_retry(self, max: int) -> Retry throws never {
    Retry { inner: self, max: max, base_delay_ms: null }
  }
  function fallback_to(self, other: Provider) -> Fallback throws never {
    Fallback { members: [self, other] }
  }
}
