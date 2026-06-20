// =============================================================================
// Scenario 17 — Within-run history + sessions (the transcript)
// implement.baml  —  STD-LIB / library-author code
//
// The transcript IS the memory. The application appends user AND assistant turns
// (and, once tools enter, tool_use / tool_result turns) and resends the whole
// thing every call. The model is stateless; the array is the state.
//
// We build three things on top of the canonical spine (_conventions.md):
//
//   1. A `Turn` grammar (net-new) and a `Session` (array + id + pluggable store).
//   2. A `Conversational` capability (net-new) that threads a transcript through
//      the existing HttpProvider build_request/send/parse seam — and shows the per-provider divergence
//      (system-prompt placement, message grammar) INSIDE build_request, where the
//      proposal already says it belongs.
//   3. A `Windowed` / `Summarizing` combinator (a Provider that wraps a Provider)
//      for the unbounded-growth problem — windowing/summarization as a wrapper.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. The transcript grammar  (NET-NEW types)
// -----------------------------------------------------------------------------
//
// The background names the irreducible fact: the transcript is user AND assistant
// turns interleaved, and tool calls are *also* turns. We model that as a tagged
// union so a provider's build_request can pattern-match each turn into its own
// wire grammar. `role` is the portable axis; `blocks` carries the divergent
// payloads (text, tool_use, tool_result, reasoning).

enum Role { System, User, Assistant, Tool }

// A turn is a role plus a list of typed blocks. The block union is what lets one
// transcript carry prose, tool calls, tool results, and reasoning at once — the
// "transcript is the complete record" property from the background.
class Turn {
  role: Role
  blocks: Block[]
}

// The block union. Anthropic/OpenAI both nest a content/parts array under a
// message; the Responses API promotes function_call / function_call_output to
// top-level *items*. We keep one union and let each provider decide nesting.
type Block = TextBlock | ToolUseBlock | ToolResultBlock | ReasoningBlock

class TextBlock       { text: string }
class ToolUseBlock    { id: string, name: string, args: map<string, unknown> }   // assistant asked
class ToolResultBlock { id: string, output: unknown }                            // we answered; id correlates
class ReasoningBlock  { text: string, signature: string? }                       // o-series / thinking echo


// -----------------------------------------------------------------------------
// 2. The Session  (NET-NEW type + a 4-method store protocol)
// -----------------------------------------------------------------------------
//
// A Session is exactly what the background describes: the array + an id + a
// pluggable place to hold it. The store is a 4-method protocol so the app can
// swap in-process / Redis / Postgres without the provider knowing.

// The pluggable store. 4 methods — load / save / append / clear. This is the
// "it is the application's job to *hold* the array" seam, made pluggable so the
// next scenario (persistence) drops in a different `implements SessionStore`.
// NET-NEW capability error channel. Session persistence is a capability beyond
// call/stream/realtime/tools, so it gets its own capability-error interface with a
// classifier that matters here: did the store fail because the backing IO was down
// (transient, retryable) versus a genuine missing-session / corruption fault.
interface baml.errors.SessionError {
  function is_store_unavailable(self) -> bool
}

interface SessionStore {
  function load(self, id: string) -> Turn[] throws baml.ExtendUnknownError<baml.errors.SessionError>
  function save(self, id: string, turns: Turn[]) -> null throws baml.ExtendUnknownError<baml.errors.SessionError>
  function append(self, id: string, turns: Turn[]) -> null throws baml.ExtendUnknownError<baml.errors.SessionError>
  function clear(self, id: string) -> null throws baml.ExtendUnknownError<baml.errors.SessionError>
}

// A concrete store fault. The host map IO can fail (e.g. a Redis/Postgres store
// swapped in via this same interface). It is classified as a SessionError; it also
// implements CallError so it is legal where `chat` (a call-path method) loads/records
// a session without forcing the call-path caller to widen its channel.
class StoreUnavailableError {
  detail: string
  implements baml.errors.SessionError {
    function is_store_unavailable(self) -> bool { return true; }
  }
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return true; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// The default store: an in-process map. Background §1 "what's hard": close the
// process and it's gone. That's the honest default; persistence is a swap.
class MemoryStore {
  _cells: $rust_type                                  // host-held map<string, Turn[]>

  function new() -> MemoryStore { MemoryStore { _cells: $rust_io_function } }

  implements SessionStore {
    function load(self, id: string) -> Turn[] throws baml.ExtendUnknownError<baml.errors.SessionError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SessionError>(e, "memory store load failed");
    }
    function save(self, id: string, turns: Turn[]) -> null throws baml.ExtendUnknownError<baml.errors.SessionError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SessionError>(e, "memory store save failed");
    }
    function append(self, id: string, turns: Turn[]) -> null throws baml.ExtendUnknownError<baml.errors.SessionError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SessionError>(e, "memory store append failed");
    }
    function clear(self, id: string) -> null throws baml.ExtendUnknownError<baml.errors.SessionError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.SessionError>(e, "memory store clear failed");
    }
  }
}

// The Session value the app passes into a call. It binds an id to a store; the
// turns themselves live in the store, fetched on demand. Static helpers mirror
// the std lib's no-self constructors (ArrayIterator.new).
class Session {
  id: string
  store: SessionStore

  function new(id: string, store: SessionStore) -> Session { Session { id: id, store: store } }

  // ergonomic default — an ephemeral in-process session.
  function ephemeral(id: string) -> Session { Session { id: id, store: MemoryStore.new() } }

  function history(self) -> Turn[] throws baml.ExtendUnknownError<baml.errors.SessionError> { self.store.load(self.id) }
  function record(self, turns: Turn[]) -> null throws baml.ExtendUnknownError<baml.errors.SessionError> { self.store.append(self.id, turns) }
  function reset(self) -> null throws baml.ExtendUnknownError<baml.errors.SessionError> { self.store.clear(self.id) }
}


// -----------------------------------------------------------------------------
// 3. The Conversational capability  (NET-NEW interface that `requires Provider`)
// -----------------------------------------------------------------------------
//
// Why a new capability and not just HttpProvider.call? Because the prompt is no longer
// the whole input — the transcript is. `call<T>(prompt)` takes a PromptAst (the
// freshly rendered turn). A conversational provider needs to *prepend* prior turns
// before building the request, and *append* the new user+assistant turns after.
// That threading is the capability. It still bottoms out on the existing
// build_request/send/parse seam, so the per-provider wire divergence lives exactly
// where the proposal already puts it.

interface Conversational requires Provider {
  // Turn a PromptAst (the new user turn) + the prior transcript into the full
  // request. THIS is where system-prompt placement and message grammar diverge.
  // build_chat is on the call path → the CallError channel.
  function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>

  // The threaded call: load history, build, send, parse, then persist BOTH the
  // user turn and the assistant turn (the recurring bug the background warns about
  // is appending only the user turn — here it's impossible to forget, the default
  // method does both). chat is a call-path method → CallError; the session faults it
  // surfaces are StoreUnavailableError, which implements CallError so they ride this
  // same channel without a separate one.
  function chat<T>(self, prompt: baml.llm.PromptAst, session: Session) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let user_turn: Turn = self.to_user_turn(prompt);
    let history: Turn[] = session.history() catch (e) {
      // a session-load failure on the call path is a classified store fault, not an
      // opaque unknown — surface it as StoreUnavailableError (CallError + SessionError).
      let s: baml.errors.SessionError => throw StoreUnavailableError { detail: "session load failed" };
    };
    let req = self.build_chat<T>(history, user_turn);
    let value: T = self.parse_chat<T>(self.send_chat(req));
    // persist user + assistant atomically; forgetting the assistant turn is the
    // classic contradiction bug — the seam makes it the default, not a choice.
    session.record([user_turn, self.to_assistant_turn(value)]) catch (e) {
      let s: baml.errors.SessionError => throw StoreUnavailableError { detail: "session record failed" };
    };
    return value;
  }

  // Adapters the default method leans on. A provider supplies them once. All are on
  // the call path → the CallError channel.
  function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function to_assistant_turn<T>(self, value: T) -> Turn
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function send_chat(self, request: baml.http.Request) -> baml.http.Response
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function parse_chat<T>(self, from: baml.http.Response) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError>
}


// -----------------------------------------------------------------------------
// 4. Two concrete providers — the per-provider divergence, fully written
// -----------------------------------------------------------------------------
//
// The teaching point: `build_chat` is where the providers actually disagree.
//   - OpenAI Chat Completions: system prompt is an in-array {role:"system"} message;
//     turns are flat {role, content}; tool calls round-trip as assistant.tool_calls
//     + a {role:"tool", tool_call_id} message.
//   - Anthropic Messages: system prompt is a TOP-LEVEL `system` field OUTSIDE the
//     array; turns nest a content-block array; tool_use / tool_result are blocks,
//     and tool_result correlates by `tool_use_id` (id, not positional name).

class OpenAIChat {
  model: string
  api_key: string
  system: string                                  // where it lives is per-provider — here, in-array

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function                            // single-shot path (no history) — unchanged
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai build_request failed");
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }

  implements Conversational {
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      Turn { role: User, blocks: [TextBlock { text: prompt_text(prompt) }] }
    }
    function to_assistant_turn<T>(self, value: T) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // the assistant turn is the serialized answer; for prose this is the text,
      // for structured T it's the canonical JSON the next turn will see.
      Turn { role: Assistant, blocks: [TextBlock { text: baml.json.to_string<T>(value) }] }
    }

    // ★ DIVERGENCE: OpenAI puts the system prompt as the FIRST in-array message,
    // flattens every turn to {role, content}, and encodes tool turns as
    // assistant.tool_calls + {role:"tool", tool_call_id: <id>}.
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let msgs: json[] = [];
      // system prompt: IN the array, position 0.
      msgs.push({ "role": "system", "content": self.system });
      for (let turn in history) { msgs.push(openai_encode_turn(turn)); }
      msgs.push(openai_encode_turn(next));
      let body: map<string, json> = {
        "model": self.model,
        "messages": msgs,
        "response_format": sap_response_format<T>(),   // schema goes in response_format for OpenAI
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.stringify(body),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai build_chat failed");
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http send failed");
      }
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // navigate the OpenAI envelope, then SAP-parse the content string into T.
      let env: OpenAIChatEnvelope = baml.json.from_json<OpenAIChatEnvelope>(baml.json.parse(from.text()));
      baml.sap.parse<T>(openai_message_content(env))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse_chat failed");
    }
  }
}

class AnthropicChat {
  model: string
  api_key: string
  max_tokens: int
  system: string                                  // SAME intent, DIFFERENT position (top-level field)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic build_request failed");
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }

  implements Conversational {
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      Turn { role: User, blocks: [TextBlock { text: prompt_text(prompt) }] }
    }
    function to_assistant_turn<T>(self, value: T) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      Turn { role: Assistant, blocks: [TextBlock { text: baml.json.to_string<T>(value) }] }
    }

    // ★ DIVERGENCE: Anthropic HOISTS the system prompt to a top-level `system`
    // field OUTSIDE the messages array; every turn nests a content-block array;
    // tool_result blocks correlate by `tool_use_id` (the id, never the name).
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let msgs: json[] = [];
      // NOTE: no system message in the array — it lives in the top-level field below.
      for (let turn in history) { msgs.push(anthropic_encode_turn(turn)); }
      msgs.push(anthropic_encode_turn(next));
      let body: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        "system": self.system,                  // ← top-level, OUTSIDE messages
        "messages": msgs,
        "tools": sap_forced_tool<T>(),          // schema goes in a forced tool for Anthropic
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key,
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: baml.json.stringify(body),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic build_chat failed");
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http send failed");
      }
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // forced-tool answers arrive in content[0].input, not a text block. Navigate the
      // parsed envelope, re-stringify the tool input, and SAP-parse it into T.
      let env: AnthropicMsgEnvelope = baml.json.from_json<AnthropicMsgEnvelope>(baml.json.parse(from.text()));
      baml.sap.parse<T>(baml.json.stringify(anthropic_tool_input(env)))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse_chat failed");
    }
  }
}


// -----------------------------------------------------------------------------
// 5. Per-turn wire encoders — the id-vs-name / nesting divergence, fully written
// -----------------------------------------------------------------------------
//
// These are the functions build_chat calls per turn. They are where tool_use /
// tool_result threading actually diverges. The ToolResultBlock.id correlates to
// the ToolUseBlock.id (id, OPAQUE to the loop) — OpenAI keys it `tool_call_id`,
// Anthropic keys it `tool_use_id`, Gemini (not shown) has no ids and correlates
// positionally by function name. Same transcript, three correlation schemes.

function openai_encode_turn(turn: Turn) -> json
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  match (turn.role) {
    Tool => {
      // OpenAI: a tool result is its OWN flat message with role "tool".
      let r: ToolResultBlock = first_tool_result(turn.blocks);
      return { "role": "tool", "tool_call_id": r.id, "content": baml.json.to_string<unknown>(r.output) };
    },
    Assistant => {
      // assistant text + any tool_use become assistant.content + assistant.tool_calls.
      let text: string = collect_text(turn.blocks);
      let calls: json[] = [];
      for (let b in turn.blocks) {
        match (b) {
          let u: ToolUseBlock => {
            calls.push({
              "id": u.id, "type": "function",
              "function": { "name": u.name, "arguments": baml.json.to_string<map<string, unknown>>(u.args) },
            });
          },
          _ => {},
        }
      }
      if (calls.length() > 0) {
        return { "role": "assistant", "content": text, "tool_calls": calls };
      }
      return { "role": "assistant", "content": text };
    },
    _ => { return { "role": role_string(turn.role), "content": collect_text(turn.blocks) }; },
  }
}

function anthropic_encode_turn(turn: Turn) -> json
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // Anthropic NESTS everything into a content-block array; tool_result is a block
  // INSIDE a user turn, correlated by tool_use_id.
  let content: json[] = [];
  for (let b in turn.blocks) {
    match (b) {
      let t: TextBlock       => { content.push({ "type": "text", "text": t.text }); },
      let u: ToolUseBlock    => { content.push({
                                    "type": "tool_use", "id": u.id, "name": u.name,
                                    "input": baml.json.to_json<map<string, unknown>>(u.args) }); },
      let r: ToolResultBlock => { content.push({
                                    "type": "tool_result", "tool_use_id": r.id,
                                    "content": baml.json.to_string<unknown>(r.output) }); },
      let g: ReasoningBlock  => { content.push({
                                    "type": "thinking", "thinking": g.text, "signature": g.signature }); },
    }
  }
  // a turn carrying only tool_result blocks is addressed to role "user" in Anthropic.
  let role: string = if (turn.role == Assistant) { "assistant" } else { "user" };
  return { "role": role, "content": content }
}


// -----------------------------------------------------------------------------
// 6. Unbounded growth → windowing / summarizing as a Provider wrapper
// -----------------------------------------------------------------------------
//
// Background §1 "what's hard": the array grows every turn → cost, latency, the
// context window. Mitigations are windowing / summarizing / token-budget trimming.
// In this model a mitigation is a Provider that WRAPS a Conversational provider
// and rewrites the history before delegating — the Iterator.map wrapper pattern,
// applied to the transcript instead of the stream.
//
// Because these wrap and re-expose Conversational, they chain: Summarizing over
// Windowed over OpenAIChat is a legal stack, read outside-in like a combinator.

// Keep only the last `keep` turns (plus any leading system turn). Lossy-by-forgetting.
class Windowed {
  inner: Conversational
  keep: int

  implements Conversational {
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.build_chat<T>(window_tail(history, self.keep), next)
    }
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.to_user_turn(prompt)
    }
    function to_assistant_turn<T>(self, value: T) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.to_assistant_turn<T>(value)
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.send_chat(request)
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.parse_chat<T>(from)
    }
  }

  // Windowed is still a Provider (the marker) because Conversational requires Provider.
  // It ALSO forwards the request/response capability: its single-shot call/call_with
  // has no history, so it routes to the inner provider IF that provider is an
  // HttpProvider. The combinator factories (with_retry/…) are inherited via the marker
  // and are NOT redefined here.
  implements HttpProvider {
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
    // Forward the projection to the inner member so usage/sidecar is not dropped (P1).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => { throw baml.errors.Unsupported { message: "windowed inner cannot do request/response" }; },
      }
    }
  }
}

// Replace turns older than `keep` with one LLM-written synopsis turn. Lossy-by-
// distortion. The summarizer is ITSELF a Provider — summarization is just a call.
class Summarizing {
  inner: Conversational
  summarizer: Provider
  keep: int

  implements Conversational {
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let n: int = history.length();
      if (n <= self.keep) { return self.inner.build_chat<T>(history, next); }
      // older turns get folded into a single rolling-synopsis system-ish turn. The
      // summarizer is a Provider (marker); `call` lives on HttpProvider now, so we
      // match it for the request/response capability before summarizing.
      let old: Turn[] = window_head(history, n - self.keep);
      let synopsis: string = (match (self.summarizer) {
        let h: HttpProvider => h.call<string>(summarize_prompt(old)),
        _ => throw baml.errors.Unsupported { message: "summarizer cannot do request/response" },
      }) catch (e) {
        // if summarization fails (for ANY reason — call error, Unsupported, or unknown),
        // fall back to a hard window rather than dropping state. Recovery, not rethrow.
        _ => { return self.inner.build_chat<T>(window_tail(history, self.keep), next); }
      };
      let folded: Turn[] =
        [Turn { role: System, blocks: [TextBlock { text: "Earlier conversation summary: " + synopsis }] }]
          .concat(window_tail(history, self.keep));
      return self.inner.build_chat<T>(folded, next);
    }
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.to_user_turn(prompt)
    }
    function to_assistant_turn<T>(self, value: T) -> Turn
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.to_assistant_turn<T>(value)
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.send_chat(request)
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.inner.parse_chat<T>(from)
    }
  }

  // Summarizing is a Provider (the marker) via Conversational; it also forwards the
  // request/response capability to its inner member when that member is an HttpProvider,
  // routing the projection so usage/sidecar survives (P1). Factories are inherited.
  implements HttpProvider {
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => { throw baml.errors.Unsupported { message: "summarizing inner cannot do request/response" }; },
      }
    }
  }
}


// -----------------------------------------------------------------------------
// 7. Small host-boundary / pure helpers used above
// -----------------------------------------------------------------------------

// genuine host boundary: pull the rendered text out of the opaque PromptAst.
function prompt_text(p: baml.llm.PromptAst) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  $rust_io_function
} catch (e) {
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "prompt_text failed");
}

// schema-encoding helpers — the FOUR structured-output encodings live in build_chat
// (response_format for OpenAI, forced-tool for Anthropic). These produce native `json`
// fragments that nest directly into the request-body map.
function sap_response_format<T>() -> json
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  $rust_io_function
} catch (e) {
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "sap_response_format failed");
}
function sap_forced_tool<T>() -> json[]
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  $rust_io_function
} catch (e) {
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "sap_forced_tool failed");
}

// -----------------------------------------------------------------------------
// Response envelopes + navigation — typed coercion replaces the invented
// Response.json_path. parse_chat decodes the provider envelope into one of these
// classes, then reads the field where the answer lives.
// -----------------------------------------------------------------------------

class OpenAIChatEnvelope { choices: OpenAIChoice[] }
class OpenAIChoice       { message: OpenAIMessage }
class OpenAIMessage      { content: string }

// content lives at choices[0].message.content; a missing choice is a malformed envelope.
function openai_message_content(env: OpenAIChatEnvelope) -> string
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  match (env.choices.at(0)) {
    null => throw MalformedTurnError { detail: "openai response had no choices" },
    let c: OpenAIChoice => c.message.content,
  }
}

class AnthropicMsgEnvelope { content: AnthropicContentBlock[] }
class AnthropicContentBlock { input: map<string, json> }

// the forced-tool answer is content[0].input; absent content is a malformed envelope.
function anthropic_tool_input(env: AnthropicMsgEnvelope) -> json
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  match (env.content.at(0)) {
    null => throw MalformedTurnError { detail: "anthropic response had no content blocks" },
    let b: AnthropicContentBlock => b.input,
  }
}

// pure transcript utilities. slice clamps out-of-range indices, so the
// max(0, …) guards keep a `keep`/`take` larger than the array (or zero) honest.
function window_tail(turns: Turn[], keep: int) -> Turn[] {
  turns.slice(baml.math.max(0, turns.length() - keep), turns.length())
}
function window_head(turns: Turn[], take: int) -> Turn[] {
  turns.slice(0, baml.math.max(0, take))
}

function collect_text(blocks: Block[]) -> string {
  let out: string = "";
  for (let b in blocks) { match (b) { let t: TextBlock => { out = out + t.text; }, _ => {} } }
  out
}

// A malformed transcript turn is a deliberate domain failure on the call path: the
// turn claimed role Tool but carried no tool_result block. A concrete CallError, not
// a transport fault — is_parse_error is true (the transcript shape is wrong).
class MalformedTurnError {
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return true; }
  }
}

function first_tool_result(blocks: Block[]) -> ToolResultBlock
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  for (let b in blocks) { match (b) { let r: ToolResultBlock => { return r; }, _ => {} } }
  throw MalformedTurnError { detail: "tool turn had no tool_result block" }
}

function role_string(r: Role) -> string {
  match (r) { System => "system", User => "user", Assistant => "assistant", Tool => "tool" }
}

function summarize_prompt(old: Turn[]) -> baml.llm.PromptAst
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  $rust_io_function
} catch (e) {
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "summarize_prompt failed");
}
