// ============================================================================
// 09 — Function / tool calling basics  ::  implement.baml  (LIBRARY AUTHOR code)
// ============================================================================
//
// We build on the canonical primitives from _conventions.md VERBATIM:
//   interface Provider          — call<T>
//   interface HttpProvider      — build_request / send / parse / default call
//   interface Tools             — begin / step / submit / default run_tools
//   class ToolCall  { id, name, args }          // id OPAQUE to the loop
//   class ToolResult{ id, output }
//   class ToolCalls { calls }                   // the Iterator "Done" sentinel
//   class Tool      { name, description, parameters: type }   // first-class `type`
//
// The teaching point of THIS file: the Tools capability seam (`begin`/`step`/
// `submit`) is identical across providers, but everything below it diverges —
//   * one tool schema -> three wire dialects (JSON Schema vs Gemini OpenAPI subset)
//   * id-vs-name correlation of results to calls
//   * tool-role vs tool_result-in-user vs functionResponse
//   * positional ordering (Anthropic) vs id keying (OpenAI) vs name keying (Gemini)
// ...and ALL of it lives inside the provider body, under the seam.
//
// We also introduce two genuinely net-new host primitives (flagged in
// evaluation.md): `ExecutionContext.dispatch` and a `deps` slot for
// dependency injection (the Pydantic-AI RunContext[Deps] shape).
// ============================================================================


// ----------------------------------------------------------------------------
// NET-NEW host surface #1: ExecutionContext gains `dispatch`.
//
// `Tools.run_tools` (the default loop in _conventions.md) calls
// `ctx.dispatch(calls.calls)`. The loop hands the provider's ToolCall[] to the
// app's registered handlers and gets back ToolResult[] — correlated purely by
// the OPAQUE `ToolCall.id` the loop never interprets. `deps` is the injected,
// out-of-band state the model never sees (DB handle, current user, auth).
// ----------------------------------------------------------------------------
class ExecutionContext {
  function_name: string
  args: map<string, unknown>
  deps: unknown                      // injected dependencies (Pydantic-AI RunContext[Deps])

  // Validate each call's args against the tool's declared `parameters: type`,
  // run the matching handler, and stamp the SAME id back onto the result.
  // The provider correlates by that id (or, for Gemini, re-derives correlation
  // from name+position BELOW the seam — the loop is none the wiser).
  function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient {
    let out: ToolResult[] = [];
    for (let c in calls) {
      // Host bridge: look up the registered handler for `c.name`, SAP-validate
      // `c.args` against its parameter `type`, inject `self.deps`, invoke it.
      // A validation failure becomes a ToolResult carrying an error payload so
      // the model can self-correct, rather than aborting the whole loop.
      let result: ToolResult = self.invoke_one(c) catch (e) {
        _ => { ToolResult { id: c.id, output: { "error": "tool execution failed" } } }
      };
      out = out.append(result);
    }
    out
  }

  // Host boundary: resolve handler by name, validate args against its `type`,
  // inject deps, call it. Stamps `c.id` onto the returned ToolResult.
  function invoke_one(self, c: ToolCall) -> ToolResult throws baml.errors.LlmClient {
    $rust_io_function
  }
}


// ============================================================================
// PROVIDER A — OpenAI (Chat Completions): JSON Schema, call ids, `role:"tool"`
// ============================================================================
//
// Wire facts this provider must own, all below the seam:
//   * tool schema:        JSON Schema under  function.parameters
//   * call id:            YES  (tool_call.id)
//   * result threading:   a dedicated  { role:"tool", tool_call_id, content }  msg
//   * correlation:        by id
// ----------------------------------------------------------------------------
class OpenAI {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  // The plain chat codec (no tools) — gives us `call` for free.
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

  // The tool loop. Transcript is OUR private, opaque shape: it carries the full
  // OpenAI `messages` array (assistant turns preserved verbatim — dropping the
  // assistant turn that requested a tool breaks the protocol) plus the encoded
  // tool list and the desired output `type`.
  implements Tools {
    type Transcript = OpenAiTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> OpenAiTranscript throws baml.errors.LlmClient {
      // DIALECT #1: encode each Tool's `parameters: type` to JSON Schema, wrapped
      // in OpenAI's { type:"function", function:{ name, description, parameters } }.
      let wire_tools: string = OpenAI.encode_tools_openai(tools);
      OpenAiTranscript {
        model: self.model,
        api_key: self.api_key,
        messages: baml.llm.prompt_to_openai_messages(prompt),   // user/system turns
        wire_tools: wire_tools,
        output_schema: baml.llm.schema_json_of(prompt),         // where SAP expects the final T
      }
    }

    function step<T>(self, t: OpenAiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      // POST /chat/completions with t.messages + t.wire_tools.
      let resp: baml.http.Response = OpenAI.post_chat(t) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "openai chat failed" }; }
      };
      let body: string = resp.text();

      // finish_reason == "tool_calls"  => model wants to call tools.
      if (baml.llm.openai_finish_reason(body) == "tool_calls") {
        // Each entry has a unique `id` (call.id), a function name and a JSON-string
        // of arguments. We surface { id, name, args } — id is opaque to the loop.
        let raw: OpenAiToolCall[] = baml.llm.openai_extract_tool_calls(body);
        let calls: ToolCall[] = [];
        for (let r in raw) {
          calls = calls.append(ToolCall {
            id: r.id,                                           // <- echoed back later
            name: r.name,
            args: baml.sap.parse<map<string, unknown>>(r.arguments_json),
          });
        }
        // CRITICAL: stash the assistant turn that carried these tool_calls onto the
        // transcript so submit() can re-send it. (Mutating via a returned copy.)
        OpenAI.remember_assistant_turn(t, body);
        return ToolCalls { calls: calls };
      }

      // Otherwise it's a final answer: SAP-parse the content into T.
      baml.sap.parse<T>(baml.llm.openai_content(body))
    }

    function submit(self, t: OpenAiTranscript, results: ToolResult[]) -> OpenAiTranscript throws baml.errors.LlmClient {
      // THREADING #1: OpenAI wants ONE dedicated message per result:
      //   { role:"tool", tool_call_id: <id>, content: <output> }
      // correlated BY ID. Order does not matter to OpenAI, but we keep it stable
      // for cache friendliness.
      let appended: string[] = [];
      for (let r in results) {
        appended = appended.append(
          baml.llm.openai_tool_message(r.id, baml.sap.to_json(r.output))   // {role:"tool",tool_call_id,content}
        );
      }
      OpenAI.append_messages(t, appended)
    }
  }

  // --- statics: the dialect encoders & host bridges, all BELOW the seam ---
  function encode_tools_openai(tools: Tool[]) -> string throws baml.errors.LlmClient {
    // For each tool: reflect its `parameters: type` to JSON Schema, then wrap.
    let entries: string[] = [];
    for (let tool in tools) {
      let schema: string = baml.reflect.type_to_json_schema(tool.parameters);  // standard JSON Schema
      entries = entries.append(
        baml.llm.openai_tool_entry(tool.name, tool.description, schema)
      );
    }
    baml.llm.json_array(entries)
  }
  function post_chat(t: OpenAiTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_assistant_turn(t: OpenAiTranscript, body: string) -> null { $rust_io_function }
  function append_messages(t: OpenAiTranscript, msgs: string[]) -> OpenAiTranscript { $rust_io_function }
}

class OpenAiTranscript {
  model: string
  api_key: string
  messages: string                  // serialized OpenAI messages[] (assistant turns preserved)
  wire_tools: string                // serialized JSON-Schema tool list
  output_schema: string
}
class OpenAiToolCall { id: string, name: string, arguments_json: string }


// ============================================================================
// PROVIDER B — Anthropic (Messages): JSON Schema, tool_use ids, results in USER
// ============================================================================
//
// Wire facts, all below the seam — different from OpenAI in THREE ways:
//   * tool schema:        JSON Schema under  input_schema  (key differs)
//   * call id:            YES  (tool_use.id)
//   * result threading:   tool_result blocks nested inside a  role:"user"  message
//   * correlation:        by id (tool_use_id), but POSITIONAL ORDER inside the
//                         user message must match the tool_use block order
//   * max_tokens:         REQUIRED bespoke field (a provider-specific param —
//                         dissolves into a class field, no options blob)
// ----------------------------------------------------------------------------
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int                   // bespoke, required by the Messages API

  implements ChatModelOptions { model as model, api_key as api_key }

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

  implements Tools {
    type Transcript = AnthropicTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // DIALECT #2: same JSON Schema as OpenAI, but under `input_schema` and with
      // no `{type:"function"}` wrapper.
      AnthropicTranscript {
        model: self.model,
        api_key: self.api_key,
        max_tokens: self.max_tokens,
        messages: baml.llm.prompt_to_anthropic_messages(prompt),
        wire_tools: Anthropic.encode_tools_anthropic(tools),
        // remembers, per submit(), the ORDER of tool_use ids from the last assistant
        // turn so results can be re-ordered positionally.
        pending_ids: [],
      }
    }

    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      let resp: baml.http.Response = Anthropic.post_messages(t) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "anthropic messages failed" }; }
      };
      let body: string = resp.text();

      // stop_reason == "tool_use"  => the assistant content is a list of blocks;
      // each tool_use block carries id / name / input.
      if (baml.llm.anthropic_stop_reason(body) == "tool_use") {
        let raw: AnthropicToolUse[] = baml.llm.anthropic_extract_tool_use(body);
        let calls: ToolCall[] = [];
        let ids: string[] = [];
        for (let r in raw) {
          calls = calls.append(ToolCall { id: r.id, name: r.name, args: r.input });
          ids = ids.append(r.id);                               // remember positional order
        }
        // Preserve the assistant turn verbatim (it carries the tool_use blocks) and
        // record the id order for positional re-assembly in submit().
        Anthropic.remember_assistant_turn(t, body, ids);
        return ToolCalls { calls: calls };
      }

      baml.sap.parse<T>(baml.llm.anthropic_text(body))
    }

    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // THREADING #2: results go back as `tool_result` blocks nested in a SINGLE
      // role:"user" message, and the blocks must appear in the SAME ORDER as the
      // tool_use blocks that requested them. We re-sort `results` by `t.pending_ids`
      // before serializing — the loop handed us results in dispatch order, which is
      // NOT guaranteed to match the assistant turn's order.
      let ordered: ToolResult[] = Anthropic.reorder_by_ids(results, t.pending_ids);
      let blocks: string[] = [];
      for (let r in ordered) {
        blocks = blocks.append(
          baml.llm.anthropic_tool_result_block(r.id, baml.sap.to_json(r.output))  // {type:"tool_result",tool_use_id,content}
        );
      }
      // wrap all blocks in one user message and append
      Anthropic.append_user_message(t, blocks)
    }
  }

  function encode_tools_anthropic(tools: Tool[]) -> string throws baml.errors.LlmClient {
    let entries: string[] = [];
    for (let tool in tools) {
      let schema: string = baml.reflect.type_to_json_schema(tool.parameters);     // standard JSON Schema
      entries = entries.append(
        baml.llm.anthropic_tool_entry(tool.name, tool.description, schema)         // input_schema, no wrapper
      );
    }
    baml.llm.json_array(entries)
  }
  // Re-order results to match the recorded tool_use order (positional discipline).
  function reorder_by_ids(results: ToolResult[], order: string[]) -> ToolResult[] {
    let out: ToolResult[] = [];
    for (let id in order) {
      for (let r in results) {
        if (r.id == id) { out = out.append(r); }
      }
    }
    out
  }
  function post_messages(t: AnthropicTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_assistant_turn(t: AnthropicTranscript, body: string, ids: string[]) -> null { $rust_io_function }
  function append_user_message(t: AnthropicTranscript, blocks: string[]) -> AnthropicTranscript { $rust_io_function }
}

class AnthropicTranscript {
  model: string
  api_key: string?
  max_tokens: int
  messages: string
  wire_tools: string
  pending_ids: string[]             // tool_use id order from the last assistant turn
}
class AnthropicToolUse { id: string, name: string, input: map<string, unknown> }


// ============================================================================
// PROVIDER C — Gemini (generateContent): OpenAPI subset, NO ids, functionResponse
// ============================================================================
//
// The hard one. Wire facts, all below the seam:
//   * tool schema:        OpenAPI 3.0 SUBSET under functionDeclarations — NOT full
//                         JSON Schema. Drops $ref, restricts `format`, handles
//                         anyOf differently. A schema that round-trips to OpenAI
//                         must be TRANSFORMED here (DIALECT #3).
//   * call id:            NONE. Gemini matches a functionResponse to its
//                         functionCall by function NAME + POSITION.
//   * result threading:   functionResponse parts inside a user/function turn.
//   * correlation:        by name. Two concurrent calls to the SAME function are
//                         ambiguous by name alone — so we SYNTHESIZE a local id at
//                         the seam and keep a private id->(name,index) map, so the
//                         loop's ToolCall.id contract still holds even though the
//                         wire has no ids.
// ----------------------------------------------------------------------------
class Gemini {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

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

  implements Tools {
    type Transcript = GeminiTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> GeminiTranscript throws baml.errors.LlmClient {
      // DIALECT #3: transform each tool's JSON-Schema-shaped `parameters: type`
      // into the Gemini OpenAPI-3 subset (strip $ref, coerce formats, flatten
      // anyOf). This is the lossy translation the background calls out.
      GeminiTranscript {
        model: self.model,
        api_key: self.api_key,
        contents: baml.llm.prompt_to_gemini_contents(prompt),
        wire_tools: Gemini.encode_tools_gemini(tools),
        // id synthesis: maps OUR opaque ToolCall.id -> (name, positional index).
        id_map: [],
      }
    }

    function step<T>(self, t: GeminiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      let resp: baml.http.Response = Gemini.post_generate(t) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "gemini generateContent failed" }; }
      };
      let body: string = resp.text();

      // A turn with functionCall parts => tool calls. There are NO wire ids, so we
      // synthesize a stable local id per (name, position) and store the mapping.
      let raw: GeminiFunctionCall[] = baml.llm.gemini_extract_function_calls(body);
      if (raw.length() > 0) {
        let calls: ToolCall[] = [];
        let mapping: GeminiIdEntry[] = [];
        let index: int = 0;
        for (let r in raw) {
          // synthesize: name + index uniquely identifies this call within the turn,
          // disambiguating two concurrent calls to the same function (the exact case
          // name-correlation alone can't handle).
          let synthetic: string = Gemini.synth_id(r.name, index);
          calls = calls.append(ToolCall { id: synthetic, name: r.name, args: r.args });
          mapping = mapping.append(GeminiIdEntry { id: synthetic, name: r.name, index: index });
          index = index + 1;
        }
        Gemini.remember_model_turn(t, body, mapping);
        return ToolCalls { calls: calls };
      }

      baml.sap.parse<T>(baml.llm.gemini_text(body))
    }

    function submit(self, t: GeminiTranscript, results: ToolResult[]) -> GeminiTranscript throws baml.errors.LlmClient {
      // THREADING #3: results go back as functionResponse parts inside a user turn.
      // The wire has no ids, so we look up each result's synthetic id in t.id_map to
      // recover the function NAME (and position) Gemini correlates by. We re-order by
      // the recorded index so positional matching stays correct.
      let parts: string[] = [];
      for (let entry in Gemini.sorted_by_index(t.id_map)) {
        for (let r in results) {
          if (r.id == entry.id) {
            parts = parts.append(
              baml.llm.gemini_function_response(entry.name, baml.sap.to_json(r.output))  // keyed by NAME
            );
          }
        }
      }
      Gemini.append_function_turn(t, parts)
    }
  }

  function encode_tools_gemini(tools: Tool[]) -> string throws baml.errors.LlmClient {
    let decls: string[] = [];
    for (let tool in tools) {
      // The lossy transform: JSON Schema -> Gemini OpenAPI-3 subset.
      let openapi: string = baml.reflect.type_to_gemini_openapi(tool.parameters);
      decls = decls.append(
        baml.llm.gemini_function_declaration(tool.name, tool.description, openapi)
      );
    }
    baml.llm.gemini_tools_wrapper(decls)            // { functionDeclarations: [...] }
  }
  function synth_id(name: string, index: int) -> string { $rust_io_function }
  function sorted_by_index(map: GeminiIdEntry[]) -> GeminiIdEntry[] {
    // stable sort by .index so functionResponse ordering matches functionCall ordering
    $rust_io_function
  }
  function post_generate(t: GeminiTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_model_turn(t: GeminiTranscript, body: string, map: GeminiIdEntry[]) -> null { $rust_io_function }
  function append_function_turn(t: GeminiTranscript, parts: string[]) -> GeminiTranscript { $rust_io_function }
}

class GeminiTranscript {
  model: string
  api_key: string
  contents: string                  // serialized Gemini contents[]
  wire_tools: string                // serialized functionDeclarations (OpenAPI subset)
  id_map: GeminiIdEntry[]           // synthetic id -> (name, position); wire has no ids
}
class GeminiFunctionCall { name: string, args: map<string, unknown> }
class GeminiIdEntry { id: string, name: string, index: int }


// ============================================================================
// COMBINATOR — ToolFallback: forward the Tools capability by runtime delegation
// ============================================================================
//
// Mirrors Fallback in §6 of the proposal, but forwards `Tools` (begin/step/
// submit) instead of (or in addition to) call/stream. A combinator is a plain,
// non-generic class that `match`es its members and routes to a capable one.
//
// CAVEAT it must own: a Transcript belongs to the member that produced it. We
// cannot start `begin` on member A and `submit` to member B. So the Transcript
// we expose remembers WHICH member owns it, and step/submit re-dispatch to that
// same member. This is the "Transcript is provider-owned + opaque" rule biting
// at the combinator layer.
// ----------------------------------------------------------------------------
class ToolFallback {
  strategy: Provider[]

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) { _ => { continue; } };
        return r;
      }
      throw baml.errors.LlmClient { message: "all providers failed" }
    }
  }

  implements Tools {
    type Transcript = BoundTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> BoundTranscript throws baml.errors.LlmClient {
      // Pick the first member that actually implements Tools; bind the transcript
      // to it so step/submit re-target the same provider.
      for (let p in self.strategy) {
        match (p) {
          let tp: Tools => {
            let inner = tp.begin<T>(prompt, tools);
            return BoundTranscript { owner: tp, inner: inner };
          },
          _ => { continue; }
        }
      }
      throw baml.errors.LlmClient { message: "no tool-capable provider available" }
    }

    function step<T>(self, t: BoundTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      // re-dispatch to the bound owner; its opaque Transcript lives inside t.inner.
      t.owner.step<T>(t.inner)
    }

    function submit(self, t: BoundTranscript, results: ToolResult[]) -> BoundTranscript throws baml.errors.LlmClient {
      let next = t.owner.submit(t.inner, results);
      BoundTranscript { owner: t.owner, inner: next }
    }
  }
}

class BoundTranscript {
  owner: Tools                       // the member that owns the live transcript
  inner: unknown                     // that member's opaque Transcript (its associated type)
}
