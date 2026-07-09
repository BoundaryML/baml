// scenario 16 — agent security (lethal trifecta): ../../../../../llm-provider/ideas/scenarios/16-agent-security/implement.baml
//   implementation.baml — LIBRARY / std-lib-author code.
//
// The threat: indirect prompt injection / MCP tool poisoning — a third party
// plants instructions in the DATA the agent reads and turns the agent's own
// privileges against it (fetch untrusted content + hold a secret + reach an exfil
// tool = the lethal trifecta). No provider marks content untrusted, so the trust
// boundary is entirely the caller's to build. The design adds:
//
//   1. `s16_Tainted<T>` — a provenance wrapper: a value + a set of taint labels
//      ("web:evil.example", "mcp:notion") that survives transformation (`map` is
//      VIRAL — the transformed value re-emits the incoming taint).
//   2. `s16_Provenance requires Tools` — a capability a Tools provider MAY add to
//      label its tool results by source (default: labels nothing, honestly
//      mirroring the API reality that most providers can't).
//   3. `s16_Quarantined` — a combinator that BREAKS the trifecta by CAPABILITY
//      SUBTRACTION: it `implements HttpProvider` (request/response) and nothing
//      else, so the quarantined model can READ untrusted bytes but structurally
//      cannot ACT (no `Tools` impl => no `.run_tools`). Dual-LLM as a type fact.
//   4. An allowlist-gated dispatch closure (the design's `GatedContext`): outbound
//      URLs off the allowlist are refused, and a side-effecting tool fed TAINTED
//      args escalates to a human. Both refusals are typed `baml.errors.ToolError`s.
//
// REAL-SURFACE MAPPING (what changed from the design's `implement.baml`):
//   - `ExecutionContext.dispatch` does not exist; the real `Tools.run_tools` /
//     `ToolLoop` take a `dispatch: (ToolCall[]) -> ToolResult[] throws never`
//     CLOSURE. So the design's `GatedContext implements ExecutionContext` becomes
//     `s16_gate` (throws typed refusals) + `s16_gated_dispatch` (a `throws never`
//     closure factory that catches those refusals into DENIED `ToolResult`s — a
//     denied call is fed back to the model as data, not a crash — the honest shape
//     for the `throws never` dispatch seam).
//   - `ToolCall.args` and `ToolResult.output` are `string` (JSON), not `unknown`,
//     so the URL/taint checks are pure BAML string+json ops — no `$rust_type`
//     ledger, no host shims (offline tier runs with no network/keys).
//   - Taint is a plain `string[]` set (not a host-owned `$rust_type`), because on
//     the real surface it rides through the `string` seam, not through `unknown`.
//
// DESIGN DIVERGENCES (noted, not built):
//   - The design's VIRAL cross-turn taint LEDGER (results inherit their call's
//     provenance; a downstream call whose args reference a tainted prior result is
//     itself tainted) needs mutable state threaded across `submit` turns, which the
//     `throws never` dispatch closure has no home for. Here the app declares the
//     propagated taint explicitly as `s16_gate`'s `tainted_tools` argument (the set
//     of tool names whose args carry tainted data this turn). `s16_Tainted.map`
//     still models the viral property at the value level, and `s16_Provenance`
//     still models per-result labelling; only the cross-turn plumbing is elided.
//   - The design's `scrub_output` markdown-image scrubber is a pure post-pass on
//     the final answer; kept as `s16_scrub_output` (same allowlist, renderer-side).
//   - The FLUENT `p.quarantined()` factory is a net-new default on the frozen
//     stdlib `Provider` interface; we construct `s16_Quarantined` directly (exactly
//     as scenarios 15/29 construct `Guarded`/`Fallback` directly).
//
// Reuses (read-only, shared namespace): common/fakes.baml's EchoProvider;
// scenario 10's ScriptedTools / echo_dispatch / lookup_tools.


// ── 1. Tainted<T>: the provenance wrapper the providers refuse to give us ──────

/// A value carrying a set of taint labels (its provenance). The set is a plain
/// `string[]` — on the real surface taint rides the `string` tool seam, so no
/// host-owned `$rust_type` is needed. `map` is VIRAL: any transform that consumes
/// a tainted value re-emits the incoming taint, so provenance survives
/// summarization / embedding (the only honest way to keep taint attached).
class s16_Tainted<T> {
    value: T,
    taint: string[],

    /// A clean (untainted) value.
    function clean(v: T) -> s16_Tainted<T> throws never {
        s16_Tainted<T> { value: v, taint: [] }
    }
    /// Tag a value with one provenance label.
    function from(v: T, label: string) -> s16_Tainted<T> throws never {
        s16_Tainted<T> { value: v, taint: [label] }
    }
    function is_tainted(self) -> bool throws never {
        self.taint.length() > 0
    }
    function labels(self) -> string[] throws never {
        self.taint
    }
    /// VIRAL transform: the result carries the SAME taint as the input.
    function map<U>(self, f: (T) -> U throws never) -> s16_Tainted<U> throws never {
        s16_Tainted<U> { value: f(self.value), taint: self.taint }
    }
    /// Union in more taint (combining a tainted value with other tainted data).
    function combine(self, other_labels: string[]) -> s16_Tainted<T> throws never {
        s16_Tainted<T> { value: self.value, taint: s16_union(self.taint, other_labels) }
    }
}

/// Order-preserving, de-duplicating union of two label sets (the taint join).
function s16_union(a: string[], b: string[]) -> string[] throws never {
    let out: string[] = [];
    for (let x in a) {
        if (!out.includes(x)) { out.push(x); }
    }
    for (let x in b) {
        if (!out.includes(x)) { out.push(x); }
    }
    out
}


// ── 2. Provenance: a Tools provider MAY label its results by source ────────────

/// A capability a Tools provider can add to say how a given tool result was
/// produced (web fetch / MCP / local compute). It lives BELOW the ToolCall seam:
/// the loop never sees taint; the dispatcher does. A provider that wraps web
/// fetch / MCP labels its results; one that can't simply doesn't implement it.
///
/// DESIGN DIVERGENCE (OQ5 made concrete — the design's evaluation (c) flagged
/// exactly this): the design NEGOTIATES Provenance at runtime off the existential
/// `Provider` marker (`as_provenance(p) { match (p) { let x: Provenance => … } }`).
/// On the real surface a `match` arm binding a USER-defined interface off `Provider`
/// binds `never` (a dead arm, E0001) UNLESS the interface is registered via the full
/// `//baml:llm_capability` + `//baml:llm_companion(<suffix>)` driver machinery — and
/// that driver must return the LLM value slot `T`, which `label_for`'s `-> string[]`
/// shape does not fit. So Provenance CANNOT be a bare-`match` negotiated capability
/// here; it degrades to a statically-typed interface (`let p: s16_Provenance = …`),
/// accessed on a provider already known to have it. The "does this provider have
/// Provenance?" question the design asks at runtime is therefore not expressible for
/// a non-LLM-value capability — a real limitation, not a workaround.
/// (`requires baml.ai.Provider`, relaxed from the design's `requires Tools`: a
/// provider labels results independently of whether it drives the loop.)
interface s16_Provenance requires baml.ai.Provider {
    function label_for(self, call: baml.ai.ToolCall) -> string[] throws never
}


// ── 3. The two typed policy refusals (security analogue of `Refused`) ──────────
//
// The gate's deliberate refusals are SECURITY DECISIONS, not transport failures,
// so they are concrete `baml.errors.ToolError`s (every classifier false; `Failure`
// axis: a POLICY REFUSAL, not retryable — re-driving replays the poisoned context).
// The app catches them BY CONCRETE TYPE (evaluation (c): a consumer matching only
// the interface would wrongly retry a denied exfil).

/// An outbound URL whose host isn't on the allowlist — refused before dispatch.
class s16_BlockedOutboundUrl {
    url: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}

/// The human-in-the-loop denied a tainted, side-effecting call ("tainted input +
/// dangerous tool", the assembled trifecta).
class s16_HumanDeniedCall {
    tool: string,
    taint: string[],

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}


// ── 4. Gate helpers (pure BAML — no host shims) ────────────────────────────────

/// The outbound URL in a call's JSON args (a `.url` field), or "" if none.
function s16_url_of(call: baml.ai.ToolCall) -> string throws never {
    let j = baml.json.parse(call.args) catch (e) { _ => { return ""; } };
    baml.json.path_or(j, ".url", "")
}

/// Pull the host out of a URL: strip scheme, take the segment before the first `/`.
function s16_host_of(url: string) -> string throws never {
    let after = if (url.includes("://")) { url.split("://").at(1) ?? "" } else { url };
    after.split("/").at(0) ?? ""
}

function s16_host_allowed(allow: string[], url: string) -> bool throws never {
    allow.includes(s16_host_of(url))
}

/// Best-effort "this tool commits an external side effect / can exfiltrate".
function s16_is_side_effecting(name: string) -> bool throws never {
    name == "http_post" || name == "send_email" || name == "write_file"
}


// ── 5. The gate: the one chokepoint every tool call passes ─────────────────────

/// Provenance-aware, allowlist-enforcing dispatch. THROWS the typed refusals so a
/// direct caller can catch them by concrete type; `s16_gated_dispatch` wraps this
/// for the `throws never` ToolLoop seam. `tainted_tools` is the set of tool names
/// whose args carry tainted data this turn (the app's explicit stand-in for the
/// design's cross-turn viral ledger — see the header divergence note).
function s16_gate(
    calls: baml.ai.ToolCall[],
    allow_hosts: string[],
    tainted_tools: string[],
) -> baml.ai.ToolResult[]
    throws s16_BlockedOutboundUrl | s16_HumanDeniedCall {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        // (1) output-URL allowlist — refuse exfil hosts outright, before dispatch.
        let url = s16_url_of(c);
        if (url != "" && !s16_host_allowed(allow_hosts, url)) {
            throw s16_BlockedOutboundUrl { url: url };
        }
        // (2) provenance-aware approval gate: interrupt only when the trifecta is
        //     actually assembled here — tainted input AND a dangerous tool.
        if (s16_is_side_effecting(c.name) && tainted_tools.includes(c.name)) {
            throw s16_HumanDeniedCall { tool: c.name, taint: tainted_tools };
        }
        // (3) run the tool (offline: echo).
        out.push(baml.ai.ToolResult { id: c.id, output: "ran " + c.name });
    }
    out
}

/// A drop-in ToolLoop dispatcher (the `throws never` seam): runs `s16_gate` and
/// catches its typed refusals into DENIED `ToolResult`s, so a blocked exfil is fed
/// back to the model as DATA and the loop continues rather than crashing.
function s16_gated_dispatch(
    allow_hosts: string[],
    tainted_tools: string[],
) -> (baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        s16_gate(calls, allow_hosts, tainted_tools) catch (e) {
            let b: s16_BlockedOutboundUrl => {
                let r: baml.ai.ToolResult[] = [];
                for (let c in calls) {
                    r.push(baml.ai.ToolResult { id: c.id, output: "DENIED (allowlist): " + c.name });
                }
                return r;
            },
            let d: s16_HumanDeniedCall => {
                let r: baml.ai.ToolResult[] = [];
                for (let c in calls) {
                    r.push(baml.ai.ToolResult { id: c.id, output: "DENIED (tainted): " + c.name });
                }
                return r;
            },
        }
    }
}


// ── 6. Quarantined: break the trifecta by CAPABILITY SUBTRACTION ───────────────

/// Wraps a provider and forwards ONLY the request/response capability
/// (`implements HttpProvider`) — deliberately NO `Tools`, `Streaming`, `Realtime`.
/// So `s16_Quarantined(p)` IS, at the type level, a no-tools provider: a
/// `.run_tools` companion match against `Tools` misses it and throws `Unsupported`.
/// This is the dual-LLM/quarantine pattern expressed as a type — the quarantined
/// model reads untrusted bytes but structurally cannot act. That capability loss
/// (Tools/Streaming/Realtime) is the point; it is ENFORCED, not advisory.
class s16_Quarantined {
    inner: baml.ai.Provider,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["quarantined forwards to inner; no single request"] }
        }
        function send(self, request: baml.http.Request) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["quarantined does not send directly"] }
        }
        function parse<T>(self, from: unknown) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["quarantined does not parse directly"] }
        }
        function parse_meta(self, from: unknown) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["quarantined has no single meta"] }
        }

        // Drive the inner with an IDENTITY projection (the Traced/Guarded pattern),
        // then re-apply the caller's projection so the usage/sidecar is not dropped.
        // NOTE (design D5 residue): `call`'s return is bare `T`, NOT `Tainted<T>` —
        // a combinator cannot re-type `HttpProvider.call`'s `-> T` per-wrapper. The
        // quarantined model's output is untrusted-by-construction, so wrapping it
        // `Tainted<T>` is the CALLER's obligation (see usage.baml). This is the
        // leakiest seam in the scenario and it is unavoidable on this surface.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let r0: baml.ai.CallResult<T, baml.ai.ResponseMeta> = match (self.inner) {
                let h: baml.ai.HttpProvider => h.call_messages_with<T, baml.ai.ResponseMeta, never>(
                    messages,
                    (mm: baml.ai.ResponseMeta) -> baml.ai.ResponseMeta { mm },
                ),
                _ => throw baml.errors.Unsupported { message: "quarantined inner cannot produce a value" },
            };
            baml.ai.CallResult<T, V> { value: r0.value, meta: project(r0.meta) }
        }
    }
    // intentionally NO `implements Tools` / `Streaming` / `Realtime`.
}


// ── 7. An offline privileged agent: Tools + Provenance ─────────────────────────
//
// The design's OpenAITools/GeminiTools are native wire providers (id- vs
// name-correlation lives below the seam in scenario 09). Offline, one scripted
// agent stands in: it drives the loop AND labels web-sourced results (Provenance),
// so the gate and the provenance capability can both be exercised with no network.

class s16_AgentState {
    posted: bool,
    final_json: string,
}

/// Scripts exactly one side-effecting `http_post` (to `post_url`) then answers.
/// Implements Provenance: web-fetch/web-search results are tainted by source; a
/// local `http_post` result is clean (the taint on THAT call comes from its ARGS).
class s16_ExfilAgent {
    post_url: string,
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = s16_AgentState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s16_AgentState throws never {
            s16_AgentState { posted: false, final_json: self.final_json }
        }
        function step<T>(self, t: s16_AgentState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (!t.posted) {
                return baml.ai.ToolCalls {
                    calls: [baml.ai.ToolCall {
                        id: "call_1",
                        name: "http_post",
                        args: "{\"url\":\"" + self.post_url + "\",\"body\":\"secrets\"}",
                    }],
                };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["exfil agent final parse failed"] },
            }
        }
        function submit(self, t: s16_AgentState, results: baml.ai.ToolResult[]) -> s16_AgentState throws never {
            s16_AgentState { posted: true, final_json: t.final_json }
        }
    }

    implements s16_Provenance {
        function label_for(self, call: baml.ai.ToolCall) -> string[] throws never {
            if (call.name == "web_fetch" || call.name == "web_search") {
                ["web:" + s16_host_of(s16_url_of(call))]
            } else {
                []
            }
        }
    }
}


// ── 8. Output scrubbing (renderer-side leg of the trifecta) ────────────────────

/// Post-pass over a model's final answer: drop any markdown image whose host isn't
/// on the allowlist (the model API never sees the renderer, so the exfil can only
/// be closed here). Simplified pure-BAML scrubber: strips `![...](url)` spans whose
/// host is off-allowlist. Same allowlist as the dispatcher.
function s16_scrub_output(answer: string, allow_hosts: string[]) -> string throws never {
    // Split on the markdown-image opener; rebuild, dropping off-allowlist targets.
    let pieces = answer.split("![");
    let out = pieces.at(0) ?? "";
    let i = 0;
    for (let piece in pieces) {
        if (i > 0) {
            // piece looks like: alt](https://host/path)rest...
            let close = piece.split(")");
            let head = close.at(0) ?? "";                 // alt](https://host/path
            let rest = close.slice(1, close.length()).join(")");
            let url = if (head.includes("](")) { head.split("](").at(1) ?? "" } else { "" };
            if (url != "" && s16_host_allowed(allow_hosts, url)) {
                out = out + "![" + head + ")" + rest;
            } else {
                out = out + "[image removed]" + rest;
            }
        }
        i = i + 1;
    }
    out
}
