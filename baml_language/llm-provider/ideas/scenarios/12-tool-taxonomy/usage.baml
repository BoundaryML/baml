// ============================================================================
// 12 · Tool taxonomy — APPLICATION-author code.
//
// What the app author writes to use the five tool kinds. The ergonomic claim:
// declaring a tool is choosing a ToolKind variant; the *loop* is one companion
// call (.run_mixed) regardless of who executes what; only the ExecutionContext
// you hand in changes based on which kinds need YOUR code.
// ============================================================================


// ----------------------------------------------------------------------------
// Two providers, to prove portability across the divergences the background
// flags (Anthropic version-suffixed hosted tools + mcp_servers connector vs
// OpenAI Responses bare hosted tools + server-side mcp dispatch).
// ----------------------------------------------------------------------------

client Claude() {
  Anthropic { model: "claude-opus-4-8", api_key: env.ANTHROPIC_API_KEY, max_tokens: 8192 }
}

client GPT() {
  OpenAIResponses { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

// A research function whose answer the model assembles via a HOSTED tool the
// app author NEVER executes. Note `web_search` is just a declaration — there is
// no callback, no parameters: type. The app author writes almost nothing.
class Brief { headline: string, sources: string[] }


// ----------------------------------------------------------------------------
// CASE A · hosted-only. No app-side code runs. The ExecutionContext's dispatch
// and perform are never reached; only `observe` fires, to trace the search.
// ----------------------------------------------------------------------------

// A trivial context: function tools impossible (none declared), computer-use
// impossible (none declared), and observe just logs.
class TraceOnly {
  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[]
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      throw baml.errors.Unsupported { message: "no client tools declared" }
    }
    function perform(self, call: ComputerCall) -> Screenshot
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      throw baml.errors.Unsupported { message: "no computer tool declared" }
    }
    function observe(self, activity: ToolActivity) -> null {
      baml.log.info("tool: " + activity.source + " " + activity.phase);   // you OBSERVE, can't drive
      null
    }
  }
}

function Research(topic: string) -> Brief {
  client: Claude()
  prompt #"Research and summarize: {{ topic }}. Cite sources."#
}
// The .run_mixed companion is generated from the function (decompose pattern):
//   Research.run_mixed(topic, tools, ctx)
// Driving it for the hosted-tool case — the app author supplies declare-only tools:
function RunResearch(topic: string) -> Brief
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let tools: ToolKind[] = [
    HostedTool { kind: "web_search", config: { "max_uses": 3 } },
  ];
  // p is the existential Provider from Claude(); negotiate MixedTools.
  let p = Claude();
  match (p) {
    let m: MixedTools => m.run_mixed<Brief>(render_prompt(topic), tools, TraceOnly {}),
    // delivery-style fallback is NOT honest here: a plain .call can't run a
    // hosted web_search loop. Different interaction shape -> error.
    _ => throw baml.errors.Unsupported { message: "client cannot host tools" },
  }
}


// ----------------------------------------------------------------------------
// CASE B · mixed: one client FunctionTool + one HOSTED code-execution tool in
// the SAME run. The loop interleaves "your code runs lookup_order" with
// "provider runs code_interpreter, you only watch". Same companion, richer ctx.
// ----------------------------------------------------------------------------

class OrderTools {
  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[]
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let out: ToolResult[] = [];
      for (let c in calls) {
        match (c.name) {
          "lookup_order" => {
            let id = c.args["order_id"];
            out = out + [ToolResult { id: c.id, output: db_lookup_order(id) }];   // YOUR code
          },
          _ => { out = out + [ToolResult { id: c.id, output: { "error": "unknown tool" } }]; },
        }
      }
      out
    }
    function perform(self, call: ComputerCall) -> Screenshot
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      throw baml.errors.Unsupported { message: "no computer tool" }
    }
    function observe(self, activity: ToolActivity) -> null {
      baml.log.info("hosted: " + activity.source + ":" + activity.phase); null
    }
  }
}

class OrderTriage { resolution: string }

function TriageOrder(order_id: string) -> OrderTriage {
  client: GPT()
  prompt #"Investigate order {{ order_id }}; run any analysis you need, then resolve it."#
}

function RunTriage(order_id: string) -> OrderTriage
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let lookup: type = LookupArgs;     // first-class type for the FunctionTool schema
  let tools: ToolKind[] = [
    FunctionTool { name: "lookup_order", description: "Fetch an order by id", parameters: LookupArgs },
    HostedTool   { kind: "code_execution", config: { "container": { "type": "auto" } } },
  ];
  let p = GPT();
  match (p) {
    let m: MixedTools => m.run_mixed<OrderTriage>(render_prompt(order_id), tools, OrderTools {}),
    _ => throw baml.errors.Unsupported { message: "client cannot run mixed tools" },
  }
}
class LookupArgs { order_id: string }


// ----------------------------------------------------------------------------
// CASE C · computer-use (hybrid) WITH a human-in-the-loop gate. The model
// proposes clicks; YOUR code performs them on a sandboxed browser; the
// ApprovalGate wrapper pauses for a person before any click lands, because
// "the computer-use loop is your liability."
// ----------------------------------------------------------------------------

class BrowserDriver {
  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[]
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      throw baml.errors.Unsupported { message: "computer-use only" }
    }
    function perform(self, call: ComputerCall) -> Screenshot
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // drive a real/virtual machine, return a fresh screenshot keyed by call_id.
      Screenshot { call_id: call.call_id, image_b64: drive_browser(call.action) }
    }
    function observe(self, activity: ToolActivity) -> null { null }
  }
}

class ConsoleApprover {
  implements Approver {
    function review(self, name: string, args: map<string, unknown>) -> map<string, unknown>
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let ok = prompt_human(name, args);          // blocks for a person
      if (ok) { args } else { throw ApprovalDenied { tool_name: name, reason: "denied by operator" }; }
    }
  }
}

class Booking { confirmed: bool, detail: string }

function BookTable(site: string) -> Booking {
  client: GPT()
  prompt #"Book a table for 2 on {{ site }}."#
}

function RunBooking(site: string) -> Booking {
  let tools: ToolKind[] = [
    ComputerUseTool { display_width: 1280, display_height: 800, environment: "browser" },
  ];
  // Gate the clicks: any "computer_use" action requires operator approval.
  let ctx = ApprovalGate {
    inner: BrowserDriver {},
    approver: ConsoleApprover {},
    gated: ["computer_use"],
  };
  let p = GPT();
  // The gate's denial (ApprovalDenied) and any mid-loop ToolError both ride the
  // ToolError channel; catch most-specific-first per the error model.
  match (p) {
    let m: MixedTools => m.run_mixed<Booking>(render_prompt(site), tools, ctx),
    _ => throw baml.errors.Unsupported { message: "client cannot do computer use" },
  } catch (e) {
    let d: ApprovalDenied        => Booking { confirmed: false, detail: "declined: " + d.reason };
    let _t: baml.errors.ToolError => Booking { confirmed: false, detail: "tool loop failed" };
    let u: baml.UnknownError     => { baml.log.error("booking: " + u.message.join(" | ")); Booking { confirmed: false, detail: "tool loop crashed" } };
  }
}


// ----------------------------------------------------------------------------
// CASE D · remote MCP with OAuth 2.1 audience-binding. The app author declares
// a server and an auth shape; the provider discovers + dispatches; the app only
// observes activity and approves the destructive tool. Note: the app author
// writes a `resource` URI (RFC 8707) so the minted token is bound to THIS server
// — a token for `github` can't be replayed against a malicious server B.
// ----------------------------------------------------------------------------

class IssueSummary { title: string, body: string }

function SummarizeIssue(repo: string) -> IssueSummary {
  client: Claude()
  prompt #"Open the latest issue in {{ repo }} and summarize it."#
}

function RunSummarize(repo: string) -> IssueSummary
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let tools: ToolKind[] = [
    McpServerTool {
      server_label: "github",
      server_url: "https://mcp.example.com/mcp",      // Streamable HTTP endpoint
      auth: OAuth21 {
        resource: "https://mcp.example.com/mcp",      // audience-binds the token to THIS server
        client_id: null,                              // null => Dynamic Client Registration
        client_secret: null,
      },
      require_approval: ApprovalPerTool { tool_names: ["delete_issue"] },
    },
  ];
  let p = Claude();
  match (p) {
    let m: MixedTools => m.run_mixed<IssueSummary>(render_prompt(repo), tools, TraceOnly {}),
    _ => throw baml.errors.Unsupported { message: "client cannot bridge MCP" },
  }
}


// ----------------------------------------------------------------------------
// PORTABILITY NOTE (where it leaks). RunResearch on Claude() and the same shape
// on GPT() compile identically — `run_mixed` is provider-agnostic. But the
// HostedTool.kind strings are NOT portable: "web_search" lowers to
// web_search_20260209 on Anthropic and a bare {"type":"web_search"} on Responses,
// and "code_execution" lowers to code_execution_20260120 vs code_interpreter.
// The provider's build_tools absorbs the rename, but a kind a provider doesn't
// host silently no-ops at the wire (or the provider must throw). The taxonomy is
// unified at the TYPE level; the *catalog* is not.  See evaluation.md (c).
// ----------------------------------------------------------------------------


// host-side helpers used above (genuine boundaries).
function db_lookup_order(id: unknown) -> unknown { $rust_io_function }
function drive_browser(action: map<string, unknown>) -> string { $rust_io_function }   // -> screenshot b64
function prompt_human(name: string, args: map<string, unknown>) -> bool { $rust_io_function }
