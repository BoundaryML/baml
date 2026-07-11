// 39 — harness extensibility, implemented against the BEPv2 `root.ai` seam.
// Scenario-specific capabilities stay local: the shared stdlib only needs
// Provider, Request<T>, Tools, Realtime, Generate, and their ordinary drivers.

class s39_GeoArgs {
  latitude: float,
  longitude: float,
}

class s39_SearchArgs { query: string }

class s39_Patch {
  files_changed: string[],
  summary: string,
}

class s39_Triage {
  severity: string,
  reproducible: bool,
  summary: string,
}

// Tool sources are local data. In-process and MCP tools still lower to the same
// root.ai.Tool roster consumed by the shared agent loop.
class s39_InProcessSource { handler: string }
class s39_McpSource { server: string, wire_name: string, fixture_output: string }
class s39_ProviderSource {}
type s39_ToolSource = s39_InProcessSource | s39_McpSource | s39_ProviderSource

class s39_HarnessTool {
  spec: root.ai.Tool,
  source: s39_ToolSource,
}

function s39_weather_tool() -> s39_HarnessTool throws never {
  s39_HarnessTool {
    spec: root.ai.Tool.typed<s39_GeoArgs>(
      "get_temperature",
      "Get the current temperature at a location.",
    ),
    source: s39_InProcessSource { handler: "weather" },
  }
}

function s39_mcp_issue_tool() -> s39_HarnessTool throws never {
  s39_HarnessTool {
    spec: root.ai.Tool.typed<s39_SearchArgs>(
      "mcp__github__search_issues",
      "Search issues through the configured GitHub MCP server.",
    ),
    source: s39_McpSource {
      server: "github",
      wire_name: "mcp__github__search_issues",
      fixture_output: "issue 42: login returns 500",
    },
  }
}

function s39_builtin_tool(name: string) -> s39_HarnessTool throws never {
  s39_HarnessTool {
    spec: root.ai.Tool.typed<map<string, string>>(name, "Harness-owned built-in tool."),
    source: s39_ProviderSource {},
  }
}

function s39_tool_specs(tools: s39_HarnessTool[]) -> root.ai.Tool[] throws never {
  tools.map((tool: s39_HarnessTool) -> root.ai.Tool { tool.spec })
}

function s39_find_tool(tools: s39_HarnessTool[], name: string) -> s39_HarnessTool? throws never {
  for (let tool in tools) {
    if (tool.spec.name == name) { return tool; }
  }
  null
}

// Hooks observe both sides of dispatch. A blocked call still becomes a
// correlated ToolResult so the model can recover on its next turn.
class s39_HookEvent {
  kind: string,
  tool_name: string,
  payload: string,
}

class s39_HookVerdict {
  block: bool,
  replacement: string?,
  reason: string?,
}

interface s39_Hook {
  function on_event(self, event: s39_HookEvent) -> s39_HookVerdict throws never
}

class s39_AuditHook {
  events: string[],

  implements s39_Hook {
    function on_event(self, event: s39_HookEvent) -> s39_HookVerdict throws never {
      let _ = self.events.push(event.kind + ":" + event.tool_name);
      s39_HookVerdict { block: false, replacement: null, reason: null }
    }
  }
}

class s39_BlockToolHook {
  blocked_name: string,

  implements s39_Hook {
    function on_event(self, event: s39_HookEvent) -> s39_HookVerdict throws never {
      if (event.kind == "pre_tool" && event.tool_name == self.blocked_name) {
        return s39_HookVerdict { block: true, replacement: null, reason: "blocked by policy hook" };
      }
      s39_HookVerdict { block: false, replacement: null, reason: null }
    }
  }
}

function s39_run_hooks(hooks: s39_Hook[], event: s39_HookEvent) -> s39_HookVerdict throws never {
  let replacement: string? = null;
  for (let hook in hooks) {
    let verdict = hook.on_event(event);
    if (verdict.block) { return verdict; }
    if (verdict.replacement != null) { replacement = verdict.replacement; }
  }
  s39_HookVerdict { block: false, replacement: replacement, reason: null }
}

function s39_dispatch_one(
  tools: s39_HarnessTool[],
  hooks: s39_Hook[],
  call: root.ai.ToolCall,
) -> root.ai.ToolResult throws never {
  let pre = s39_run_hooks(hooks, s39_HookEvent {
    kind: "pre_tool", tool_name: call.name, payload: call.args,
  });
  if (pre.block) {
    return root.ai.ToolResult { id: call.id, output: "error: " + (pre.reason ?? "blocked") };
  }
  let args = pre.replacement ?? call.args;
  let routed = match (s39_find_tool(tools, call.name)) {
    null => root.ai.ToolResult { id: call.id, output: "error: tool is not in the surfaced roster" },
    let tool: s39_HarnessTool => match (tool.source) {
      let local: s39_InProcessSource => {
        if (local.handler == "weather") {
          root.ai.invoke_tool<s39_GeoArgs>(
            root.ai.ToolCall { id: call.id, name: call.name, args: args },
            (geo: s39_GeoArgs) -> string { `12.5C @ ${geo.latitude},${geo.longitude}` },
          )
        } else {
          root.ai.ToolResult { id: call.id, output: "error: unknown in-process handler" }
        }
      },
      let mcp: s39_McpSource => root.ai.ToolResult { id: call.id, output: mcp.fixture_output },
      let owned: s39_ProviderSource => root.ai.ToolResult {
        id: call.id, output: "provider-owned tool: " + call.name,
      },
    },
  };
  let post = s39_run_hooks(hooks, s39_HookEvent {
    kind: "post_tool", tool_name: call.name, payload: routed.output,
  });
  root.ai.ToolResult { id: routed.id, output: post.replacement ?? routed.output }
}

function s39_dispatch(
  tools: s39_HarnessTool[],
  hooks: s39_Hook[],
) -> (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
      s39_dispatch_one(tools, hooks, call)
    })
  }
}

// A harness-only capability: named typed procedures do not pretend to be a
// Generate call. The Request<T> still carries task identity and output type.
interface s39_Skills requires root.ai.Provider {
  function run_skill<T>(
    self,
    request: root.ai.Request<T>,
    name: string,
    args: map<string, unknown>,
  ) -> T throws baml.errors.UnknownError
  function commands(self) -> string[] throws baml.errors.UnknownError
}

class s39_HarnessTranscript {
  roster: root.ai.Tool[],
  remaining: root.ai.ToolCall[],
  final_json: string,
}

class s39_ScriptedHarness {
  calls: root.ai.ToolCall[],
  final_json: string,
  skill_json: string,
  skill_commands: string[],
  submitted: string[],

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s39-scripted-harness" }
  }

  implements root.ai.Tools {
    type Transcript = s39_HarnessTranscript

    function begin<T>(self, request: root.ai.Request<T>) -> s39_HarnessTranscript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      s39_HarnessTranscript {
        roster: request.tools,
        remaining: self.calls,
        final_json: self.final_json,
      }
    }

    function step<T>(self, transcript: s39_HarnessTranscript) -> T | root.ai.ToolCalls
        throws baml.errors.ToolError | baml.errors.UnknownError {
      match (transcript.remaining.at(0)) {
        let call: root.ai.ToolCall => {
          if (transcript.roster.filter((tool: root.ai.Tool) -> bool { tool.name == call.name }).length() == 0) {
            throw baml.errors.UnknownError {
              data: call,
              message: ["scripted harness requested a tool outside the surfaced roster"],
            };
          }
          root.ai.ToolCalls { calls: [call] }
        },
        null => baml.sap.parse<T>(transcript.final_json) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["harness final parse failed"] },
        },
      }
    }

    function submit(
      self,
      transcript: s39_HarnessTranscript,
      results: root.ai.ToolResult[],
    ) -> s39_HarnessTranscript throws baml.errors.ToolError | baml.errors.UnknownError {
      for (let result in results) { let _ = self.submitted.push(result.output); }
      s39_HarnessTranscript {
        roster: transcript.roster,
        remaining: transcript.remaining.slice(1, transcript.remaining.length()),
        final_json: transcript.final_json,
      }
    }
  }

  implements s39_Skills {
    function run_skill<T>(
      self,
      request: root.ai.Request<T>,
      name: string,
      args: map<string, unknown>,
    ) -> T throws baml.errors.UnknownError {
      if (!self.skill_commands.includes(name)) {
        throw baml.errors.UnknownError { data: name, message: ["unknown harness skill"] };
      }
      baml.sap.parse<T>(self.skill_json) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["skill output parse failed"] },
      }
    }

    function commands(self) -> string[] throws baml.errors.UnknownError { self.skill_commands }
  }

  implements root.ai.Realtime {
    function open_live<T>(
      self,
      request: root.ai.Request<T>,
      channel: root.ai.Channel,
    ) -> root.ai.Live throws baml.errors.UnknownError {
      s39_Live {
        items: [
          root.ai.TranscriptDelta { text: "session started" },
          root.ai.TranscriptDelta { text: "pairing on " + baml.llm.prompt_to_text(request.prompt) },
          root.ai.LiveClosed {},
        ],
      }
    }
  }
}

class s39_Live {
  items: root.ai.LiveEvent[],

  implements root.ai.Live {
    function events(self) -> root.ai.LiveEventStream throws never { self.items }
    function cancel_response(self) -> null throws baml.errors.UnknownError { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError { null }
    function close(self) -> null throws never { null }
    function cleanup(self) -> null throws never { null }
  }
}

// The sub-agent wrapper forwards only Generate. It intentionally withholds
// Tools and Skills, which enforces the no-nested-sub-agent rule structurally.
class s39_SubAgent {
  inner: root.ai.Provider,
  description: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s39-sub-agent" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      match (self.inner) {
        let generate: root.ai.Generate => generate.generate<T>(request.for_provider(self.inner)),
        _ => throw baml.errors.Unsupported { message: "sub-agent inner cannot generate" },
      }
    }
  }
}

// A2A is request/response, so it honestly implements Generate. Offline examples
// use fixture_text; the integration block exercises JSON-RPC over HTTP.
class s39_A2aPart { text: string }
class s39_A2aMessage { parts: s39_A2aPart[] }
class s39_A2aResult { message: s39_A2aMessage }
class s39_A2aEnvelope { result: s39_A2aResult }

class s39_A2aProvider {
  endpoint: string?,
  headers: map<string, string>,
  fixture_text: string?,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s39-a2a-peer" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let peer_text = match (self.fixture_text) {
        let fixture: string => fixture,
        null => {
          let endpoint = self.endpoint ?? "";
          let prompt_json = baml.json.to_string(baml.llm.prompt_to_text(request.prompt)) catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["a2a prompt encode failed"] },
          };
          let response = baml.http.send(baml.http.Request {
            method: "POST",
            url: endpoint,
            headers: self.headers,
            body: `{"jsonrpc":"2.0","id":"baml","method":"message/send","params":{"text":${prompt_json}}}`,
          }) catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["a2a send failed"] },
          };
          let body = response.text() catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["a2a response read failed"] },
          };
          let envelope = baml.json.from_string<s39_A2aEnvelope>(body) catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["a2a envelope parse failed"] },
          };
          envelope.result.message.parts.at(0)?.text ?? ""
        },
      };
      let value: T = baml.sap.parse<T>(peer_text) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["a2a result did not match task output"] },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: "s39-a2a-peer", model: null, request_id: null,
          finish_reason: "stop", usage: null, attributes: {}, raw: null,
        },
      }
    }
  }
}

function s39_prompt_template(
  text: string,
  structured: bool,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    if (structured) {
      baml.llm.assemble_prompt_ast(["", "\n", ""], [text, ctx.output_format])
    } else {
      baml.llm.assemble_prompt_ast(["", ""], [text])
    }
  }
}
