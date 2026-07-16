// ===========================================================================
// 38 — Permissions & sandboxing
// usage.baml  —  APPLICATION-author code
//
// The app builds a code-fixing agent and bounds it three ways:
//   * permission MODE + tool lists as bound CLIENT config (ClaudeCode);
//   * a per-call approval CALLBACK handed in via the ExecutionContext;
//   * a SANDBOX bound into the client (container for CI, virtual for previews).
//
// Two providers prove portability across the mode-vocabulary divergence:
//   * ClaudeCode — rich `permission_mode` words ("plan" / "acceptEdits" / ...).
//   * FlueHarness — NO modes; permission is the tool list + the dispatcher, and
//     the in-process virtual sandbox is the default.
// The dispatcher (PermissionedContext) is identical for both — it rides above
// the ToolCall seam and is indifferent to the provider's mode vocabulary.
// ===========================================================================


// ---- clients are just functions returning a Provider --------------------

// A trusted-CI agent: container sandbox (real isolation), acceptEdits mode so it
// auto-applies file edits, Bash still gated. allowed_tools narrows the surface.
// sandbox / permission_mode / allowed_tools are all BOUND FIELDS, like `model`.
client CiFixer() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: ContainerSandbox { _handle: provision_container() },   // the FENCE
    allowed_tools: ["Read", "Edit", "Grep", "Glob", "Bash"],
    disallowed_tools: ["Bash(rm*)", "Bash(curl*)"],                 // pattern sieve
    permission_mode: "acceptEdits",
  }
}

// A preview/scratch agent: plan mode (read & propose, never act) in the cheap
// in-process virtual sandbox. Nothing it does touches the real machine.
client PreviewPlanner() {
  ClaudeCode {
    model: "claude-haiku-4-6",
    sandbox: VirtualSandbox.new(),                                  // virtual fence
    allowed_tools: ["Read", "Grep", "Glob"],
    disallowed_tools: null,
    permission_mode: "plan",
  }
}

// Same shape on Flue — note there is NO permission_mode here, because Flue has no
// mode vocabulary. The virtual sandbox is the DEFAULT (from() supplies it). The
// only permission lever is the tool list + the dispatcher.
client FluePreview() {
  FlueHarness.from("openrouter/moonshotai/kimi-k2.6")              // virtual sandbox default
}

// Flue with a real container instead of the virtual default — sandbox is just a
// field, swapped without touching anything else.
client FlueContainer() {
  FlueHarness.with_sandbox("openai/gpt-5.5", ContainerSandbox { _handle: provision_container() })
}


// ---- the function the app calls -----------------------------------------
//
// The client is config-only. The live control channel and the approval callback
// are NOT client fields — they arrive at the call (the channel via `.live`, the
// callback inside the ExecutionContext we hand to `.run_tools`).
//
// NOTE: there is no basic `FixBug(task)` value-call here. CiFixer() is a harness
// (ClaudeCode) — a subprocess agent that exposes Tools/Realtime, NOT HttpProvider,
// and (../provider-as-marker.md) it no longer fakes a single-turn `call`. So the
// app drives it through its REAL interactions: `.run_tools` for the batch loop
// (fix_in_ci / fix_with_flue) and `.live` + `.run_tools` for the steerable session
// (fix_interactive). A bare `FixBug(task)` companion would `match` the client for
// HttpProvider, miss, and throw `Unsupported` — so we don't write one.


// ---- the per-call approval CALLBACK (the can_use_tool hook) --------------
//
// Background §3's can_use_tool, written in BAML as a closure. It is conditional:
// destructive Bash -> deny; Edit -> allow (and could rewrite args); everything
// else -> ask. "ask" falls through to the operator (or default-denies if there's
// no control channel). This is the "interesting policy" the background names.

function safety_callback() -> (ToolCall) -> ApprovalDecision {
  call => {
    if (call.name == "Bash" && command_has(call, "rm")) {
      ApprovalDecision { behavior: "deny", updated_input: null, message: "destructive command blocked" }
    } else if (call.name == "Edit") {
      ApprovalDecision { behavior: "allow", updated_input: null, message: null }
    } else {
      ApprovalDecision { behavior: "ask", updated_input: null, message: null }
    }
  }
}


// ---- batch run: gate via ExecutionContext, no human present --------------
//
// `.run_tools` matches CiFixer() against `Tools` (ClaudeCode implements it) and
// threads our PermissionedContext. We build the declarative rules FROM the
// provider's own mode (mode_to_rules) AND layer the programmatic callback on top.
// With no control channel, any "ask" default-denies — correct for unattended CI.

function fix_in_ci(task: string) -> Patch
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let actor: Provider = CiFixer();

  // pull the declarative posture out of the Claude-specific mode word. This is
  // the ONE place the mode vocabulary is interpreted; everything downstream is
  // provider-agnostic ApprovalRule data.
  let rules: ApprovalRule[] = match (actor) {
    let cc: ClaudeCode => cc.mode_to_rules(),
    _ => [],                                  // a provider with no modes contributes none
  };

  let ctx = PermissionedContext {
    policy: ToolPolicy {
      tools: code_tools(),
      rules: rules.concat([
        // declarative require_approval as DATA: always gate network egress.
        ApprovalRule { tool: "WebFetch", require: "always", deny_pattern: null },
        // pattern sieve as DATA: deny the literal `rm -rf` invocation outright.
        ApprovalRule { tool: "Bash", require: "always", deny_pattern: "Bash(rm -rf*)" },
      ]),
    },
    can_use_tool: safety_callback(),          // the programmatic hook
    control: null,                            // unattended: "ask" == deny
  };

  match (actor) {
    let t: Tools => t.run_tools<Patch>(render_task(task), code_tools(), ctx),
    _ => throw baml.errors.Unsupported { message: "client is not a Tools provider" },
  }
}


// ---- interactive run: the SAME gate, but "ask" -> a Channel EVENT ---------
//
// Here a human is watching a live steerable session. We hand the control channel
// to BOTH `.live` (so the provider drives the session over it) and to the
// PermissionedContext (so an "ask" decision EMITS an approval event the operator
// answers over the same channel). This is the third home for the approval
// decision — field? no; hook? yes for batch; CHANNEL EVENT for the live session.

function fix_interactive(task: string, io: Channel) -> Patch
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let actor: Provider = CiFixer();
  let rules: ApprovalRule[] = match (actor) {
    let cc: ClaudeCode => cc.mode_to_rules(),
    _ => [],
  };
  let ctx = PermissionedContext {
    policy: ToolPolicy { tools: code_tools(), rules: rules },
    can_use_tool: safety_callback(),
    control: io,                              // present -> "ask" escalates over the channel
  };

  // the provider drives the steerable session over `io`; when its loop reaches a
  // tool that the ctx marks "ask", PermissionedContext.escalate emits an approval
  // request event onto `io` and blocks for the operator's reply.
  match (actor) {
    let t: Tools => t.run_tools<Patch>(render_task(task), code_tools(), ctx),
    _ => throw baml.errors.Unsupported { message: "client is not a Tools provider" },
  }
}


// ---- Flue: identical dispatcher, NO mode vocabulary ----------------------
//
// The ONLY thing that changed from fix_in_ci is the client. There is no
// mode_to_rules to call (Flue has no modes), so the app supplies its own rules
// from the tool list. PermissionedContext, ApprovalRule, the callback, and the
// sandbox abstraction are all provider-agnostic.

function fix_with_flue(task: string) -> Patch
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let actor: Provider = FlueContainer();
  let ctx = PermissionedContext {
    policy: ToolPolicy {
      tools: code_tools(),
      rules: [
        ApprovalRule { tool: "Bash", require: "always", deny_pattern: "Bash(rm*)" },
        ApprovalRule { tool: "WebFetch", require: "always", deny_pattern: null },
      ],
    },
    can_use_tool: safety_callback(),
    control: null,
  };
  match (actor) {
    let t: Tools => t.run_tools<Patch>(render_task(task), code_tools(), ctx),
    _ => throw baml.errors.Unsupported { message: "flue client is not a Tools provider" },
  }
}


// ---- supporting app types + host glue -----------------------------------

class Patch { files_changed: string[], diff: string }

class BashArgs  { command: string }
class EditArgs  { path: string, old: string, new: string }
class FetchArgs { url: string }

function code_tools() -> Tool[] {
  [
    Tool { name: "Read",     description: "read a file",        parameters: reflect.type_of<FetchArgs>() },
    Tool { name: "Edit",     description: "edit a file",        parameters: reflect.type_of<EditArgs>() },
    Tool { name: "Bash",     description: "run a shell command",parameters: reflect.type_of<BashArgs>() },
    Tool { name: "WebFetch", description: "fetch a URL",        parameters: reflect.type_of<FetchArgs>() },
  ]
}

function command_has(call: ToolCall, needle: string) -> bool { $rust_io_function }   // reads call.args.command
function render_task(task: string) -> baml.llm.PromptAst { $rust_io_function }
function provision_container() -> $rust_type { $rust_io_function }                    // connector handle
