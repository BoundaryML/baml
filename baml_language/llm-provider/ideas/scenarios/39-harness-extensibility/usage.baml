// =============================================================================
// 39 — Harness extensibility: APPLICATION-AUTHOR code.
//
// What an app author writes to teach an agent new tricks: custom tools, an
// external MCP server, specialized sub-agents (owned + A2A peers), a typed
// skill, and a lint-after-edit hook. The library code is in implement.baml.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. Author an in-process tool. The four-tuple (name, desc, schema, handler),
//    exactly the Claude SDK shape. Input args infer from GeoArgs; output is loose.
// -----------------------------------------------------------------------------

class GeoArgs { latitude: float, longitude: float }

// The handler is host code — in a real app this is the closure that fetches
// open-meteo. From BAML we only capture it; $get_temp_handler is host-provided.
function weather_tool() -> HarnessTool {
  Tools$factory.in_process<GeoArgs>(
    "get_temperature",
    "Get the current temperature at a location",
    $get_temp_handler,                       // host closure, (GeoArgs) -> unknown
  )
}


// -----------------------------------------------------------------------------
// 2. A sub-agent: a restricted ClaudeCode wrapped in SubAgent. It gets a small
//    tool set and is opaque to its parent (cannot spawn its own sub-agents).
// -----------------------------------------------------------------------------

client Reviewer() {
  SubAgent {
    description: "Expert reviewer. Use for quality and security reviews.",
    inner: ClaudeCode {
      model: "claude-sonnet-4-6",
      builtin_tools: ["Read", "Grep", "Glob"],        // restricted — no Edit/Bash
      permission_mode: "default",
    },
    max_turns: 8,
  }
}


// -----------------------------------------------------------------------------
// 3. An A2A peer agent we DON'T own, discovered by its Agent Card. It is a
//    Provider too, so it drops into the same `agents` map as the owned reviewer.
// -----------------------------------------------------------------------------

client TranslatorPeer() {
  // discovery hits /.well-known/agent-card.json, then speaks JSON-RPC 2.0.
  A2aProvider.discover("https://translate.example.com", { "Authorization": "Bearer {env.A2A_TOKEN}" })
}


// -----------------------------------------------------------------------------
// 4. A lint-after-edit hook. Observes post_tool on "Edit"; rewrites nothing,
//    just runs a linter as policy. Could `block` instead to enforce.
// -----------------------------------------------------------------------------

class LintHook {
  implements Hook {
    function on_event(self, e: HookEvent) -> HookVerdict throws baml.errors.Io {
      if (e.kind == "post_tool") {
        match (e.tool_name) {
          let n: string => {
            if (n == "Edit" || n == "mcp__fs__write") { run_linter(); }
          },
          _ => {},
        }
      }
      HookVerdict { block: false, replace: null, reason: null }
    }
  }
}
function run_linter() -> null throws baml.errors.Io { $rust_io_function }


// -----------------------------------------------------------------------------
// 5. The main agent. Pulls a remote MCP server's tools, adds the in-process
//    weather tool, registers BOTH an owned sub-agent and an A2A peer, and wires
//    the hook. All of it is plain provider FIELDS — no options blob.
// -----------------------------------------------------------------------------

client DevAgent() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    permission_mode: "acceptEdits",
    builtin_tools: ["Read", "Edit", "Bash", "Grep"],

    // ONE map: in-process tool + every tool from the GitHub MCP server, flat.
    tools: [weather_tool()].concat(
      Tools$factory.from_mcp("github", "https://mcp.github.com/mcp", { "Authorization": "Bearer {env.GITHUB_TOKEN}" })
    ),

    // owned sub-agent AND cross-process peer in the same registry — both Providers.
    agents: {
      "code-reviewer": Reviewer(),
      "translator":    TranslatorPeer(),
    },

    hooks: [LintHook {}],
  }
}


// -----------------------------------------------------------------------------
// 6. Functions over the agent. Show the three call shapes:
//    - plain .run_tools (the tool loop)        — uses Tools capability
//    - the .skill companion (typed slash cmd)  — uses Skills capability
//    - the .live companion (steerable session) — uses Realtime capability
// -----------------------------------------------------------------------------

class Patch { files_changed: string[], summary: string }

// The tool loop. The companion `.run_tools` matches DevAgent's provider against
// `Tools` and drives begin/step/submit, dispatching through a HookedExecutionContext.
function Fix(task: string) -> Patch {
  client: DevAgent()
  prompt #"Fix the bug, then ask the code-reviewer sub-agent to check it: {{ task }}"#
}
// App author calls:  Fix.run_tools("login 500s", devAgentTools(), hookedCtx())
// where the harness builds `ctx` from DevAgent's tools+hooks fields (see below).


// A typed skill ("triage"), Flue-style. This is a CAPABILITY companion: .skill
// matches the provider against `Skills` and errors if absent (a chat model can't).
class Triage { severity: string, reproducible: bool, summary: string }

function TriageIssue(issue_number: int) -> Triage {
  client: DevAgent()
  prompt #"(skill-driven; prompt unused for skills)"#
}
// App author calls:  TriageIssue.skill<Triage>("triage", { "issueNumber": issue_number })


// A steerable live session: the control/event channel is handed in at the call.
function Pair(task: string) -> Transcript {
  client: DevAgent()
  prompt #"Pair with me on: {{ task }}"#
}
// App author calls:  Pair.live("refactor auth", io)   ->   ClaudeCode.run(prompt, io)


// -----------------------------------------------------------------------------
// 7. PORTABILITY CHECK — swap the provider, keep the function. A2A peer as the
//    PRIMARY client (no harness at all). Proves a peer-over-the-wire and an
//    owned subprocess harness both satisfy `Provider.call` behind one function.
// -----------------------------------------------------------------------------

client RemoteSummarizer() {
  A2aProvider.discover("https://summarize.example.com", {})
    .with_retry(2)                          // combinator chain reads the same as on any provider
    .fallback_to(ClaudeCode { model: "claude-haiku-4-5", permission_mode: "default" })
}

function Summarize(doc: string) -> string {
  client: RemoteSummarizer()                // A2A peer first, local harness as fallback
  prompt #"Summarize: {{ doc }}"#
}
// Summarize("...")  -> tries the A2A peer (HttpProvider.call), on failure the harness.
// Same call site whether the answer comes from a peer agent or a subprocess.
