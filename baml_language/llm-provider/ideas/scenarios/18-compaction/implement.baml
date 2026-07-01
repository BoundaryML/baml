// =============================================================================
// implement.baml — context compaction as a provider capability. Library code.
//
// Builds on the canonical primitives in ../_conventions.md (Provider, Tools,
// HttpProvider). The teaching points:
//
//   * Compaction is a CAPABILITY (`requires Provider`) — a method on the few
//     providers that can shrink the live window. It is NOT part of the base.
//   * The live window and the compacted result are PROVIDER-OWNED + OPAQUE
//     (associated types, exactly like `Tools.Transcript`). The caller passes
//     `compacted.output` forward VERBATIM and never edits it.
//   * The three provider shapes diverge hard and the divergence is irreducible:
//       - OpenAI    : provider-produced REPLACEMENT state (`compact()` endpoint).
//       - Anthropic : server-side context EDITING — prune in place, report what
//                     was removed; the "compaction" is a side effect of a normal
//                     turn, not a separate call.
//       - Gemini    : NO endpoint. Roll-your-own LLM summary, built FROM its own
//                     `HttpProvider.call` — so a pure-BAML provider implements
//                     Compaction by calling itself.
//   * Compaction busts the prompt-cache prefix; we surface that as metadata, not
//     as something the model can hide.
//   * Lossy by design — there is no inspection seam on the opaque output, and we
//     say so in the type.
// =============================================================================

// -----------------------------------------------------------------------------
// NET-NEW capability + host surface on top of _conventions.md.
// -----------------------------------------------------------------------------

// NET-NEW error channel: compaction is a capability beyond call/stream/realtime/
// tools, so it gets its OWN capability-error interface (same shape as
// baml.errors.CallError). Both Compaction.{window_of,compact} and
// ContextEditing.edit declare `baml.ExtendUnknownError<baml.errors.CompactionError>`.
// The same classifiers apply: a compaction request can hit the network (Gemini's
// summary round-trips through `call`), be rate-limited, or fail to parse the
// server's compaction report.
// (net-new: the CompactionError capability-error interface — flagged in evaluation.md.)
interface baml.errors.CompactionError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
}

// The live context window, as the model round-trips it. This is the SAME grammar
// `Tools.Transcript` accumulates (messages, tool calls, tool outputs, reasoning
// items) — but compaction operates on a window the CALLER holds, so it is its own
// opaque, provider-owned handle. Each provider binds `type Window` concretely.
// (net-new: a window value-type, sibling to Transcript.)
class WindowItems { _data: $rust_type }   // opaque item list; the wire grammar of one provider

// The result of compacting a window. `output` is OPAQUE machine state — a smaller
// item list in the model's own round-trip grammar. The library contract is the
// same one the background states: DO NOT read, edit, or reorder `output`; pass it
// forward verbatim ahead of the next user turn.
//
// The metadata fields are the ONLY inspectable surface. They are bookkeeping the
// provider reports, not the state itself:
//   * `busts_cache_prefix` — true when the new prefix invalidates the prompt-cache
//     key (OpenAI replacement and Anthropic compaction both do; pure pruning may
//     not move the boundary). The turn after a compaction pays a cache miss.
//   * `before_tokens` / `after_tokens` — coarse size signal for "did it help".
//   * `removed` — for prune-style providers, a human-readable note of what was
//     dropped (Anthropic `applied_edits`); empty for replacement-style providers
//     because their output is not decomposable into "what was removed".
// (net-new: the compaction result type — opaque body + non-authoritative meta.)
class Compacted {
  output: WindowItems      // OPAQUE replacement window — pass forward verbatim
  busts_cache_prefix: bool
  before_tokens: int
  after_tokens: int
  removed: AppliedEdit[]   // empty for summary/replacement shapes
}

class AppliedEdit {
  strategy: string         // e.g. "clear_tool_uses_20250919"
  cleared_items: int       // how many blocks the server pruned this pass
}

// The capability. Only providers that can shrink a window implement it. There is
// NO default `call`-fallback for this companion (see evaluation.md (c)): a model
// that has no compaction endpoint and no `HttpProvider.call` cannot fake
// replacement state. A pure-BAML provider that DOES implement HttpProvider can
// still implement this by summarizing through its own `call` — but that is a real
// implementation, not a generic fallback.
//
// (net-new: the `Compaction` capability interface.)
interface Compaction requires Provider {
  type Window

  // Wrap a raw item list into this provider's window handle. The caller obtains
  // its starting window from its session/transcript store (out of scope here).
  function window_of(self, items: WindowItems) -> Window
      throws baml.ExtendUnknownError<baml.errors.CompactionError>

  // Shrink the window. Returns opaque replacement state + cache/size metadata.
  function compact(self, window: Window) -> Compacted
      throws baml.ExtendUnknownError<baml.errors.CompactionError>

  // Unwrap the compacted output back into the item-list handle the NEXT request
  // consumes. The two-step (compact -> output -> window_of) lets a combinator sit
  // in the middle without ever decoding the bytes.
}

// -----------------------------------------------------------------------------
// Provider #1: OpenAI Responses — provider-produced REPLACEMENT state.
//
// Divergence shown:
//   * `compact()` is a dedicated endpoint: POST a long window, get back a SMALLER
//     item list in `compacted.output`. The output is machine state — reasoning and
//     tool history folded into a compressed form. We wrap it opaque and forbid
//     edits by never exposing a reader.
//   * Server-managed mode is ALSO possible (context_management.compact_threshold
//     on a stored chain), but that is invisible from the client — there is nothing
//     to call. So this provider exposes BOTH: the manual `compact` capability, and
//     a `compact_threshold` field that rides on the normal request and lets the
//     server compact its own stored chain. The field is config; the method is the
//     manual seam.
//   * Replacement ALWAYS busts the cache prefix.
// -----------------------------------------------------------------------------

class OpenAIResponses {
  model: string
  api_key: string
  compact_threshold: int?      // server-managed auto-compaction on a stored chain; null = off

  // Normal chat behaves like any HttpProvider; compaction is the extra capability.
  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // server-managed compaction: if compact_threshold is set, attach
      // context_management so the server compacts its stored chain past the bound.
      // This is the "client keeps doing exactly what it did before" mode.
      openai_responses_request(self.model, self.api_key, prompt, self.compact_threshold)
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }

  implements Compaction {
    type Window = OpenAIWindow

    function window_of(self, items: WindowItems) -> OpenAIWindow
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      OpenAIWindow { model: self.model, api_key: self.api_key, items: items }
    }

    function compact(self, window: OpenAIWindow) -> Compacted
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      // POST /v1/responses/compact { model, input: window.items } — the manual
      // primitive. The host returns the raw `compacted.output` (opaque) and the
      // token deltas the server reports. We DO NOT decode `output`.
      let raw: OpenAICompactResp = openai_compact_call(window.model, window.api_key, window.items);
      Compacted {
        output: raw.output,             // OPAQUE machine state — forward verbatim
        busts_cache_prefix: true,       // replacement always moves the prefix
        before_tokens: raw.before_tokens,
        after_tokens: raw.after_tokens,
        removed: [],                    // replacement is not decomposable into edits
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CompactionError>(e, "openai compact failed");
    }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
}

class OpenAIWindow {
  model: string
  api_key: string
  items: WindowItems
}
class OpenAICompactResp {
  output: WindowItems
  before_tokens: int
  after_tokens: int
}

// -----------------------------------------------------------------------------
// Provider #2: Anthropic Messages — server-side context EDITING (and compaction).
//
// Divergence shown (the reason this can't share OpenAI's shape):
//   * There is no "POST a window, get a smaller window" endpoint for the EDITS
//     mode. Pruning happens AS A SIDE EFFECT of a normal `messages.create` call:
//     you attach `context_management.edits`, the server clears stale tool/thinking
//     blocks in place, and reports `applied_edits` on the RESPONSE. So "compaction"
//     here is observed AFTER a turn, not produced by a separate call.
//   * Strategy ordering matters: clear_thinking MUST precede clear_tool_uses.
//   * Anthropic ALSO has a real summarize-style compaction primitive
//     (compact_20260112) that emits a `compaction` block you must echo back. We
//     expose that as the `compact` method; the prune-as-you-go editing is exposed
//     as a separate `edit` method, because the two have different shapes and
//     different cache behavior.
//   * Pure pruning may NOT bust the cache prefix if the boundary doesn't move;
//     summarize-style compaction does. We report each honestly.
// -----------------------------------------------------------------------------

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int
  edits: ContextEdit[]         // clearing strategies applied server-side, in order

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // attach the beta header + context_management.edits. The server prunes in
      // place during this very turn; the pruning is NOT a separate compact call.
      // edits MUST be ordered clear_thinking-before-clear_tool_uses; we sort to
      // guarantee it rather than trusting the field order.
      anthropic_messages_request(self.model, self.api_key, self.max_tokens, prompt,
                                 order_edits(self.edits))
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }

  implements Compaction {
    type Window = AnthropicWindow

    function window_of(self, items: WindowItems) -> AnthropicWindow
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      AnthropicWindow { model: self.model, api_key: self.api_key, max_tokens: self.max_tokens,
                        edits: order_edits(self.edits), items: items }
    }

    // `compact` on Anthropic uses the summarize-style primitive (compact_20260112):
    // it folds earlier turns into a `compaction` block the next request must echo.
    // That block IS new prefix state, so it busts the cache.
    function compact(self, window: AnthropicWindow) -> Compacted
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      let raw: AnthropicCompactResp =
        anthropic_compact_call(window.model, window.api_key, window.max_tokens, window.items);
      Compacted {
        output: raw.output,             // items + the echo-back `compaction` block — opaque
        busts_cache_prefix: true,       // a summary block is new prefix
        before_tokens: raw.before_tokens,
        after_tokens: raw.after_tokens,
        removed: [],                    // summarize-style: not reported as discrete edits
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CompactionError>(e, "anthropic compact failed");
    }
  }

  // The prune-in-place mode is its OWN capability surface, because it is observed
  // after a normal turn rather than produced by `compact`. An app that wants
  // edit-as-you-go uses `edit` + the per-turn `applied_edits` report.
  implements ContextEditing {
    function edit(self, window: AnthropicWindow) -> Compacted
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      // run ONE normal turn with the edits attached, then read what the server
      // pruned off the front. The "shrink" is the response's reported deletions.
      let resp: AnthropicEditedTurn =
        anthropic_edited_turn(window.model, window.api_key, window.max_tokens, window.items, window.edits);
      // the cache prefix only moves if the server actually cleared a leading block.
      let moved = resp.applied_edits.length() > 0;
      Compacted {
        output: resp.pruned_items,      // the window minus the cleared blocks — opaque
        busts_cache_prefix: moved,
        before_tokens: resp.before_tokens,
        after_tokens: resp.after_tokens,
        removed: resp.applied_edits,    // THIS shape DOES report what it removed
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CompactionError>(e, "anthropic edit failed");
    }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
}

class AnthropicWindow {
  model: string
  api_key: string?
  max_tokens: int
  edits: ContextEdit[]
  items: WindowItems
}
class ContextEdit {
  // e.g. { strategy: "clear_tool_uses_20250919", trigger_tokens: 50000, keep: 3 }
  strategy: string
  trigger_tokens: int?
  keep: int?
}
class AnthropicCompactResp { output: WindowItems, before_tokens: int, after_tokens: int }
class AnthropicEditedTurn {
  pruned_items: WindowItems
  applied_edits: AppliedEdit[]
  before_tokens: int
  after_tokens: int
}

// The prune-in-place capability. Distinct from Compaction because its result is a
// SIDE EFFECT of a normal turn, and because it CAN report `removed` precisely.
// (net-new: a second, prune-shaped capability — see evaluation.md on whether this
// should have been one interface with two shapes or two interfaces.)
interface ContextEditing requires Provider {
  function edit(self, window: AnthropicWindow) -> Compacted
      throws baml.ExtendUnknownError<baml.errors.CompactionError>
}

// -----------------------------------------------------------------------------
// Provider #3: Gemini — NO compaction endpoint. Roll-your-own LLM summary.
//
// Divergence shown (the payoff of capabilities composing on the `Provider` marker):
//   * Gemini has no compact() and no server-side editing. The ONLY way to shrink
//     a Gemini window is to ask an LLM to rewrite the old turns into a synopsis —
//     i.e. implement Compaction FROM its own `HttpProvider.call`. This provider
//     therefore implements Compaction by calling ITSELF (it implements both
//     HttpProvider and Compaction; the latter is built out of the former).
//   * The output here is NOT opaque machine state — it is prose the app could
//     read. But to keep ONE portable surface, we still wrap it in `WindowItems`
//     and still forbid editing through the type. The honesty cost (a summary IS
//     inspectable, yet we hide it) is flagged in evaluation.md.
//   * A summary rewrite always busts the cache prefix.
// -----------------------------------------------------------------------------

class GeminiSummary {
  model: string
  api_key: string
  keep_last: int               // how many recent turns to leave verbatim, un-summarized

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      gemini_generate_request(self.model, self.api_key, prompt)
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini parse failed");
    }
  }

  implements Compaction {
    type Window = GeminiWindow

    function window_of(self, items: WindowItems) -> GeminiWindow
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      GeminiWindow { items: items }
    }

    // The hand-rolled compaction: split the window into [old | recent], render the
    // old part as a summarization prompt, and ask THIS SAME provider's `call` to
    // produce a synopsis. Then splice synopsis + recent back into a new window.
    function compact(self, window: GeminiWindow) -> Compacted
        throws baml.ExtendUnknownError<baml.errors.CompactionError> {
      let split: WindowSplit = split_window(window.items, self.keep_last);
      let before = count_tokens(window.items);

      // Build a summarization prompt over the OLD turns and run it through `self`.
      // This is the load-bearing line: a pure-BAML provider implements one capability
      // (Compaction) out of another it also implements (HttpProvider.call). `self.call`
      // throws on the CallError channel; the trailing catch re-homes any such failure
      // onto the CompactionError channel.
      let summary_prompt: baml.llm.PromptAst = summarize_prompt(split.old);
      let synopsis: string = self.call<string>(summary_prompt);

      let new_items: WindowItems = splice_summary(synopsis, split.recent);
      Compacted {
        output: new_items,              // prose synopsis + recent turns, re-wrapped opaque
        busts_cache_prefix: true,       // the rewritten prefix is brand new
        before_tokens: before,
        after_tokens: count_tokens(new_items),
        removed: [],
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CompactionError>(e, "gemini compact failed");
    }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
}

class GeminiWindow { items: WindowItems }
class WindowSplit { old: WindowItems, recent: WindowItems }

// -----------------------------------------------------------------------------
// Combinator: `AutoCompact` — a Provider wrapper that compacts the live window
// at a milestone, then keeps calling the inner provider with the compacted state.
//
// This is the orchestration layer the proposal promises: it WRAPS a provider and
// adds a policy (when to compact), forwarding the `HttpProvider` capability
// (`call`/`call_with`) through — by `match`ing the inner provider for HttpProvider,
// since `inner` is typed as the bare Compaction marker (../provider-as-marker.md)
// which carries no `call` — and exposing a `checkpoint` method the app calls after
// a milestone. It works for ANY provider that implements Compaction — OpenAI,
// Anthropic, or Gemini — because it only ever touches the opaque seam
// (compact -> output -> window_of). It never decodes.
//
// It carries the live window AS STATE between turns, which is the one wrinkle:
// a combinator that holds mutable conversational state, not just config (see
// evaluation.md (c) on where stateful session state lives now that no provider
// method threads it).
// -----------------------------------------------------------------------------

class AutoCompact {
  inner: Compaction            // the existential Compaction provider it wraps
  window: WindowItems          // the live window it carries forward between turns
  last_busted_cache: bool      // diagnostics: did the most recent checkpoint bust cache

  // `call`/`call_with` are an HttpProvider capability now (Provider is a bare
  // marker — see ../provider-as-marker.md). AutoCompact forwards them to its inner
  // provider WHEN that provider is itself request/response. `inner` is typed as the
  // Compaction marker, so we `match` it for HttpProvider at the call site rather
  // than calling a base `.call` that no longer exists on the marker.
  implements HttpProvider {
    type Body = unknown

    // call_with is the PRIMITIVE (value-sidecar-model.md): forward the projection to
    // the inner provider so its usage/sidecar is NOT dropped (the P1 fix). The window
    // is threaded by the host turn machinery, not re-implemented here.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => throw baml.errors.Unsupported { message: "AutoCompact inner provider cannot call" },
      }
    }
    // plain call delegates straight to the inner provider's call.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => { return h.call<T>(prompt); },
        _ => throw baml.errors.Unsupported { message: "AutoCompact inner provider cannot call" },
      }
    }
  }

  // The milestone seam: shrink the carried window and adopt the compacted output
  // VERBATIM as the new live window. The app calls this after a sub-task finishes.
  // The opaque output is round-tripped through window_of WITHOUT inspection.
  // It forwards the inner provider's CompactionError channel unchanged (rule 4:
  // combinators declare the SAME channel as what they wrap — no widen/forward).
  function checkpoint(self) -> Compacted
      throws baml.ExtendUnknownError<baml.errors.CompactionError> {
    let w = self.inner.window_of(self.window);
    let result = self.inner.compact(w);
    self.window = result.output;               // adopt verbatim — DO NOT edit
    self.last_busted_cache = result.busts_cache_prefix;
    result
  }

  // Append a new user turn onto the carried (possibly-compacted) window. The
  // ordering contract — compacted.output FIRST, new turn AFTER — lives here so the
  // app never has to remember it. Pure local bookkeeping, but it declares the
  // CompactionError channel for uniformity with the rest of the session surface.
  function append_turn(self, items: WindowItems) -> null
      throws baml.ExtendUnknownError<baml.errors.CompactionError> {
    self.window = concat_items(self.window, items);
    null
  }
}

// AutoCompact is constructed by the `auto_compact` combinator helper, mirroring
// `with_retry`/`cached`. It is NOT a default method on Provider because it
// requires the Compaction capability, which the base does not have — so it is a
// free function gated on the capability (see evaluation.md (c)).
function auto_compact(p: Compaction, initial: WindowItems) -> AutoCompact {
  AutoCompact { inner: p, window: initial, last_busted_cache: false }
}

// -----------------------------------------------------------------------------
// Host boundaries (genuine I/O / wire-format seams only).
// -----------------------------------------------------------------------------

function openai_responses_request(model: string, key: string, prompt: baml.llm.PromptAst, threshold: int?) -> baml.http.Request { $rust_io_function }
function openai_compact_call(model: string, key: string, items: WindowItems) -> OpenAICompactResp { $rust_io_function }

function anthropic_messages_request(model: string, key: string?, max_tokens: int, prompt: baml.llm.PromptAst, edits: ContextEdit[]) -> baml.http.Request { $rust_io_function }
function anthropic_compact_call(model: string, key: string?, max_tokens: int, items: WindowItems) -> AnthropicCompactResp { $rust_io_function }
function anthropic_edited_turn(model: string, key: string?, max_tokens: int, items: WindowItems, edits: ContextEdit[]) -> AnthropicEditedTurn { $rust_io_function }
function order_edits(edits: ContextEdit[]) -> ContextEdit[] { $rust_io_function }   // clear_thinking before clear_tool_uses

function gemini_generate_request(model: string, key: string, prompt: baml.llm.PromptAst) -> baml.http.Request { $rust_io_function }
function split_window(items: WindowItems, keep_last: int) -> WindowSplit { $rust_io_function }
function summarize_prompt(old: WindowItems) -> baml.llm.PromptAst { $rust_io_function }
function splice_summary(synopsis: string, recent: WindowItems) -> WindowItems { $rust_io_function }

function count_tokens(items: WindowItems) -> int { $rust_io_function }
function concat_items(a: WindowItems, b: WindowItems) -> WindowItems { $rust_io_function }
