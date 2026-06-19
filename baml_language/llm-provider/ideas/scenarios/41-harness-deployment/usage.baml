// =============================================================================
// 41 — Embedding & deployment of a harness        (usage.baml — APP AUTHOR)
// =============================================================================
//
// This is what an application author writes ON TOP of implement.baml.
// It shows:
//   - a `client` block as the DECLARATIVE alternative to registerProvider
//   - registerProvider as the RUNTIME alternative (read at open() time)
//   - portability across two divergent transports (ClaudeCode JSONL vs Pi RPC)
//   - the .live companion bridging a host Channel into the harness
//   - a DeployedAgent with declarative webhook/cron triggers + a durable <id>

// -----------------------------------------------------------------------------
// 1. THE DECLARATIVE PATH — a `client` block.
// -----------------------------------------------------------------------------
// `client CodeAgent() { ... }` desugars to `function CodeAgent() -> Provider`.
// Config lives in the provider's fields — sandbox/permission/tools, NOT a blob.
// This is the static, source-controlled equivalent of registerProvider.

client CodeAgent() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: "container",
    permission_mode: "acceptEdits",
    allowed_tools: ["Read", "Edit", "Bash", "Grep"],
  }
}

// Swap the WHOLE transport (JSONL -> JSON-RPC) by changing one provider name.
// The app's functions below don't change: same .call, same .live, same triggers.
client RpcAgent() {
  Pi { model: "claude-sonnet-4-6", no_session: false }
}

// Supervise the subprocess (respawn-on-crash) by a combinator chain — reads like
// with_retry. Lifecycle is orchestration, so it WRAPS the provider.
client SupervisedAgent() {
  ClaudeCode { sandbox: "container", permission_mode: "default" }
    .supervised(3)
}

// -----------------------------------------------------------------------------
// 2. THE RUNTIME PATH — registerProvider at boot.
// -----------------------------------------------------------------------------
// The "app.ts" equivalent: set per-provider endpoint/credentials ONCE, applying
// to every harness/session that resolves "anthropic" at open() time. This is the
// imperative counterpart to the `client` block above — the harness body reads it
// late, so this can override a CodeAgent() constructed earlier.

function boot() -> null {
  register_provider(registry(), "anthropic", ProviderSettings {
    base_url: env.ANTHROPIC_BASE_URL,             // a gateway, not api.anthropic.com
    api_key: env.GATEWAY_KEY,
    headers: { "x-tenant": "acme" },
  });
}

// -----------------------------------------------------------------------------
// 3. PORTABLE FUNCTIONS — written once, run on either transport.
// -----------------------------------------------------------------------------
// .call flattens the whole agent run to one typed answer. The app author writes
// the SAME function regardless of JSONL vs RPC vs in-process generator.

function Fix(task: string) -> Patch {
  client: CodeAgent()                              // ClaudeCode / JSONL
  prompt #"Fix the bug, return a patch: {{ task }}"#
}

function FixViaRpc(task: string) -> Patch {
  client: RpcAgent()                               // Pi / JSON-RPC — identical body
  prompt #"Fix the bug, return a patch: {{ task }}"#
}

// -----------------------------------------------------------------------------
// 4. THE .live COMPANION — host Channel bridged into the harness duplex.
// -----------------------------------------------------------------------------
// The live control/event channel is NOT a client field; it's the io parameter of
// Realtime.run, handed in at the call. Fix.live(task, io) -> ClaudeCode.run.
//
//   Fix.live("flaky test in auth.rs", io)
//     -> match (CodeAgent()) { let r: Realtime => r.run(prompt, io), _ => throw }
//
// On ClaudeCode the io is fully duplex (interrupt/steer forwarded as JSONL).
// On AgentsRunner the io is one-way (events stream out; on() never fires).
// Same companion; the divergence is honestly visible to whoever wires io.

// -----------------------------------------------------------------------------
// 5. THE DEPLOYABLE UNIT — triggers + durable instance identity.
// -----------------------------------------------------------------------------
// An agent is a deployable value: a Provider + declarative triggers + a durable
// <id>. The substrate routes POST /agents/<name>/<id> to handle(<id>, prompt),
// and the SAME <id> continues the SAME instance under a single-writer lease.

function translator_agent() -> DeployedAgent {
  DeployedAgent {
    name: "translator",                            // filename-is-id (Flue)
    inner: ClaudeCode { sandbox: "virtual", permission_mode: "default" },
    triggers: [
      Trigger { kind: "webhook", schedule: null },
      Trigger { kind: "cron",    schedule: "0 9 * * *" },   // daily 09:00
    ],
  }
}

// What the substrate calls when POST /agents/translator/<customer-id> arrives.
// <customer-id> IS the durable instance: same id == same conversation space.
function on_webhook(customer_id: string, text: string, language: string) -> string {
  let agent = translator_agent();
  let prompt = render_translate(language, text);
  agent.handle(customer_id, prompt)                // single-writer lease inside
}

// Pi --no-session as the inner harness: NOT durable. handle() `match`es and
// degrades — every trigger is a fresh process; idempotency is on the host.
function ephemeral_agent() -> DeployedAgent {
  DeployedAgent {
    name: "summarizer",
    inner: Pi { model: "claude-sonnet-4-6", no_session: true },
    triggers: [Trigger { kind: "webhook", schedule: null }],
  }
}

// -----------------------------------------------------------------------------
// 6. CONSUMING a deployment from ANOTHER app (the author/consume split).
// -----------------------------------------------------------------------------
// Authoring (everything above) uses the runtime: provider classes, client
// blocks, triggers. CONSUMING a deployed agent is a different surface — a plain
// HTTP client addressing a running deployment by URL+token. That consumer is
// just an ordinary HttpProvider over baml.http; it imports none of the harness
// machinery. We model it as a normal provider so a consumer's own functions get
// .call/.stream for free.

class FlueSdkClient {
  base_url: string
  token: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      baml.http.Request {
        method: "POST",
        url: self.base_url + "/agents/translator/invoke",
        headers: { "authorization": "Bearer " + self.token },
        body: frame_user(prompt),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "deploy unreachable" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }
}

client RemoteTranslator() {
  FlueSdkClient { base_url: "https://example.com/api", token: env.FLUE_TOKEN }
}

// Consumer code: identical ergonomics to any model call — it has NO idea a whole
// harness, sandbox, and durable object sit behind this.
function Translate(text: string, language: string) -> string {
  client: RemoteTranslator()
  prompt #"Translate to {{ language }}: {{ text }}"#
}

function render_translate(language: string, text: string) -> baml.llm.PromptAst { $rust_io_function }
