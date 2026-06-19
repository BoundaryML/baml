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
//      the existing Provider.call seam — and shows the per-provider divergence
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
interface SessionStore {
  function load(self, id: string) -> Turn[] throws baml.errors.Io
  function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io
  function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io
  function clear(self, id: string) -> null throws baml.errors.Io
}

// The default store: an in-process map. Background §1 "what's hard": close the
// process and it's gone. That's the honest default; persistence is a swap.
class MemoryStore {
  _cells: $rust_type                                  // host-held map<string, Turn[]>

  function new() -> MemoryStore { MemoryStore { _cells: $rust_io_function } }

  implements SessionStore {
    function load(self, id: string) -> Turn[] throws baml.errors.Io { $rust_io_function }
    function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function clear(self, id: string) -> null throws baml.errors.Io { $rust_io_function }
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

  function history(self) -> Turn[] throws baml.errors.Io { self.store.load(self.id) }
  function record(self, turns: Turn[]) -> null throws baml.errors.Io { self.store.append(self.id, turns) }
  function reset(self) -> null throws baml.errors.Io { self.store.clear(self.id) }
}


// -----------------------------------------------------------------------------
// 3. The Conversational capability  (NET-NEW interface that `requires Provider`)
// -----------------------------------------------------------------------------
//
// Why a new capability and not just Provider.call? Because the prompt is no longer
// the whole input — the transcript is. `call<T>(prompt)` takes a PromptAst (the
// freshly rendered turn). A conversational provider needs to *prepend* prior turns
// before building the request, and *append* the new user+assistant turns after.
// That threading is the capability. It still bottoms out on the existing
// build_request/send/parse seam, so the per-provider wire divergence lives exactly
// where the proposal already puts it.

interface Conversational requires Provider {
  // Turn a PromptAst (the new user turn) + the prior transcript into the full
  // request. THIS is where system-prompt placement and message grammar diverge.
  function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request throws Error

  // The threaded call: load history, build, send, parse, then persist BOTH the
  // user turn and the assistant turn (the recurring bug the background warns about
  // is appending only the user turn — here it's impossible to forget, the default
  // method does both).
  function chat<T>(self, prompt: baml.llm.PromptAst, session: Session) -> T throws Error {
    let user_turn: Turn = self.to_user_turn(prompt);
    let history: Turn[] = session.history() catch (e) {
      _ => { throw baml.errors.LlmClient { message: "session load failed" }; }
    };
    let req = self.build_chat<T>(history, user_turn);
    let value: T = self.parse_chat<T>(self.send_chat(req));
    // persist user + assistant atomically; forgetting the assistant turn is the
    // classic contradiction bug — the seam makes it the default, not a choice.
    session.record([user_turn, self.to_assistant_turn(value)]) catch (e) {
      _ => { throw baml.errors.LlmClient { message: "session record failed" }; }
    };
    return value;
  }

  // Adapters the default method leans on. A provider supplies them once.
  function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn throws Error
  function to_assistant_turn<T>(self, value: T) -> Turn throws Error
  function send_chat(self, request: baml.http.Request) -> baml.http.Response throws Error
  function parse_chat<T>(self, from: baml.http.Response) -> T throws Error
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                            // single-shot path (no history) — unchanged
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Conversational {
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn throws baml.errors.LlmClient {
      Turn { role: User, blocks: [TextBlock { text: prompt_text(prompt) }] }
    }
    function to_assistant_turn<T>(self, value: T) -> Turn throws baml.errors.LlmClient {
      // the assistant turn is the serialized answer; for prose this is the text,
      // for structured T it's the canonical JSON the next turn will see.
      Turn { role: Assistant, blocks: [TextBlock { text: baml.sap.to_string<T>(value) }] }
    }

    // ★ DIVERGENCE: OpenAI puts the system prompt as the FIRST in-array message,
    // flattens every turn to {role, content}, and encodes tool turns as
    // assistant.tool_calls + {role:"tool", tool_call_id: <id>}.
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request throws baml.errors.LlmClient {
      let msgs: map<string, unknown>[] = [];
      // system prompt: IN the array, position 0.
      msgs = baml.json.push(msgs, { "role": "system", "content": self.system });
      for (let turn in history) { msgs = baml.json.push(msgs, openai_encode_turn(turn)); }
      msgs = baml.json.push(msgs, openai_encode_turn(next));
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "authorization": "Bearer " + self.api_key, "content-type": "application/json" },
        body: baml.json.encode({
          "model": self.model,
          "messages": msgs,
          "response_format": sap_response_format<T>(),   // schema goes in response_format for OpenAI
        }),
      }
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Conversational {
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn throws baml.errors.LlmClient {
      Turn { role: User, blocks: [TextBlock { text: prompt_text(prompt) }] }
    }
    function to_assistant_turn<T>(self, value: T) -> Turn throws baml.errors.LlmClient {
      Turn { role: Assistant, blocks: [TextBlock { text: baml.sap.to_string<T>(value) }] }
    }

    // ★ DIVERGENCE: Anthropic HOISTS the system prompt to a top-level `system`
    // field OUTSIDE the messages array; every turn nests a content-block array;
    // tool_result blocks correlate by `tool_use_id` (the id, never the name).
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request throws baml.errors.LlmClient {
      let msgs: map<string, unknown>[] = [];
      // NOTE: no system message in the array — it lives in the top-level field below.
      for (let turn in history) { msgs = baml.json.push(msgs, anthropic_encode_turn(turn)); }
      msgs = baml.json.push(msgs, anthropic_encode_turn(next));
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
          "x-api-key": self.api_key,
          "anthropic-version": "2023-06-01",
          "content-type": "application/json",
        },
        body: baml.json.encode({
          "model": self.model,
          "max_tokens": self.max_tokens,
          "system": self.system,                  // ← top-level, OUTSIDE messages
          "messages": msgs,
          "tools": sap_forced_tool<T>(),          // schema goes in a forced tool for Anthropic
        }),
      }
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // forced-tool answers arrive in content[0].input, not a text block.
      baml.sap.parse<T>(from.json_path("content.0.input"))
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

function openai_encode_turn(turn: Turn) -> map<string, unknown> throws baml.errors.LlmClient {
  match (turn.role) {
    Tool => {
      // OpenAI: a tool result is its OWN flat message with role "tool".
      let r: ToolResultBlock = first_tool_result(turn.blocks);
      return { "role": "tool", "tool_call_id": r.id, "content": baml.json.encode(r.output) };
    },
    Assistant => {
      // assistant text + any tool_use become assistant.content + assistant.tool_calls.
      let text: string = collect_text(turn.blocks);
      let calls: map<string, unknown>[] = [];
      for (let b in turn.blocks) {
        match (b) {
          let u: ToolUseBlock => {
            calls = baml.json.push(calls, {
              "id": u.id, "type": "function",
              "function": { "name": u.name, "arguments": baml.json.encode(u.args) },
            });
          },
          _ => {},
        }
      }
      if (baml.array.len(calls) > 0) {
        return { "role": "assistant", "content": text, "tool_calls": calls };
      }
      return { "role": "assistant", "content": text };
    },
    _ => { return { "role": role_string(turn.role), "content": collect_text(turn.blocks) }; },
  }
}

function anthropic_encode_turn(turn: Turn) -> map<string, unknown> throws baml.errors.LlmClient {
  // Anthropic NESTS everything into a content-block array; tool_result is a block
  // INSIDE a user turn, correlated by tool_use_id.
  let content: map<string, unknown>[] = [];
  for (let b in turn.blocks) {
    match (b) {
      let t: TextBlock       => { content = baml.json.push(content, { "type": "text", "text": t.text }); },
      let u: ToolUseBlock    => { content = baml.json.push(content, {
                                    "type": "tool_use", "id": u.id, "name": u.name, "input": u.args }); },
      let r: ToolResultBlock => { content = baml.json.push(content, {
                                    "type": "tool_result", "tool_use_id": r.id,
                                    "content": baml.json.encode(r.output) }); },
      let g: ReasoningBlock  => { content = baml.json.push(content, {
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
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request throws baml.errors.LlmClient {
      self.inner.build_chat<T>(window_tail(history, self.keep), next)
    }
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn throws baml.errors.LlmClient {
      self.inner.to_user_turn(prompt)
    }
    function to_assistant_turn<T>(self, value: T) -> Turn throws baml.errors.LlmClient {
      self.inner.to_assistant_turn<T>(value)
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      self.inner.send_chat(request)
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      self.inner.parse_chat<T>(from)
    }
  }

  // Windowed is still a Provider (Conversational requires Provider). Its single-shot
  // call has no history, so it just delegates to whatever the inner provider is.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
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
    function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request throws baml.errors.LlmClient {
      let n: int = baml.array.len(history);
      if (n <= self.keep) { return self.inner.build_chat<T>(history, next); }
      // older turns get folded into a single rolling-synopsis system-ish turn.
      let old: Turn[] = window_head(history, n - self.keep);
      let synopsis: string = self.summarizer.call<string>(
        summarize_prompt(old)
      ) catch (e) {
        // if summarization fails, fall back to a hard window rather than dropping state.
        _ => { return self.inner.build_chat<T>(window_tail(history, self.keep), next); }
      };
      let folded: Turn[] = baml.array.concat(
        [Turn { role: System, blocks: [TextBlock { text: "Earlier conversation summary: " + synopsis }] }],
        window_tail(history, self.keep),
      );
      return self.inner.build_chat<T>(folded, next);
    }
    function to_user_turn(self, prompt: baml.llm.PromptAst) -> Turn throws baml.errors.LlmClient {
      self.inner.to_user_turn(prompt)
    }
    function to_assistant_turn<T>(self, value: T) -> Turn throws baml.errors.LlmClient {
      self.inner.to_assistant_turn<T>(value)
    }
    function send_chat(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      self.inner.send_chat(request)
    }
    function parse_chat<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      self.inner.parse_chat<T>(from)
    }
  }

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
    }
  }
}


// -----------------------------------------------------------------------------
// 7. Small host-boundary / pure helpers used above
// -----------------------------------------------------------------------------

// genuine host boundary: pull the rendered text out of the opaque PromptAst.
function prompt_text(p: baml.llm.PromptAst) -> string throws baml.errors.LlmClient { $rust_io_function }

// schema-encoding helpers — the FOUR structured-output encodings live in build_chat
// (response_format for OpenAI, forced-tool for Anthropic). These produce the fragments.
function sap_response_format<T>() -> map<string, unknown> throws baml.errors.LlmClient { $rust_io_function }
function sap_forced_tool<T>() -> map<string, unknown>[] throws baml.errors.LlmClient { $rust_io_function }

// pure transcript utilities.
function window_tail(turns: Turn[], keep: int) -> Turn[] { baml.array.suffix(turns, keep) }
function window_head(turns: Turn[], take: int) -> Turn[] { baml.array.prefix(turns, take) }

function collect_text(blocks: Block[]) -> string {
  let out: string = "";
  for (let b in blocks) { match (b) { let t: TextBlock => { out = out + t.text; }, _ => {} } }
  out
}

function first_tool_result(blocks: Block[]) -> ToolResultBlock throws baml.errors.LlmClient {
  for (let b in blocks) { match (b) { let r: ToolResultBlock => { return r; }, _ => {} } }
  throw baml.errors.LlmClient { message: "tool turn had no tool_result block" }
}

function role_string(r: Role) -> string {
  match (r) { System => "system", User => "user", Assistant => "assistant", Tool => "tool" }
}

function summarize_prompt(old: Turn[]) -> baml.llm.PromptAst throws baml.errors.LlmClient { $rust_io_function }
