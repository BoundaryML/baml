// scenario 03 — constrained decoding: ../../../../../llm-provider/ideas/scenarios/03-constrained-decoding/usage.baml
// ============================================================================
// usage.baml — Constrained decoding beyond JSON Schema. Application-author code.
//
// Shows the ergonomics of the two-tier story on the REAL surface:
//   - A `client` selecting a self-hosted backend is one function — config only
//     ("client is sugar for a function returning a Provider", realized literally).
//   - The constrained companion `.decode` is the entry point, mirroring how
//     `.stream` / `.run_tools` decompose a function (hand-written here, since the
//     scenario's `s03_Constrained` capability has no generated companion).
//   - Two divergent providers (vLLM + llama.cpp, plus a hosted OpenAI for the
//     degrade case) prove portability — and expose where it leaks.
//
// Offline tier: negotiation, request-construction divergence, and the typed
// refusals (the leaks). The guaranteed round-trip `decode` itself needs a real
// self-hosted vLLM/llama.cpp/Outlines endpoint — BLOCKED: P8 (no such backend in
// the harness), so only the request/negotiation halves run offline.
// ============================================================================

// ---- the output types ------------------------------------------------------

enum s03_Sentiment {
  Positive
  Negative
  Neutral
}

// A SQL string is NOT JSON — the case a schema can't express at all. The output
// type is just `string`; the *grammar* carries the guarantee.

// ---------------------------------------------------------------------------
// 1. clients — config only. "Where it runs" is a field of the provider class.
// (Realized as plain functions returning `baml.ai.Provider`: the design's
// `client Name() { ... }` sugar IS "a function returning a Provider".)
// ---------------------------------------------------------------------------

// (Bodies start with `let` so the parser doesn't read a bare record literal as an
// LLM declarative body — the same guard the stdlib codec uses.)
function s03_local_llama() -> baml.ai.Provider throws never {          // vLLM serving Llama locally
  let p: baml.ai.Provider = s03_VLLM { model: "meta-llama/Llama-3.1-8B-Instruct", base_url: "http://localhost:8000/v1", api_key: null };
  p
}

function s03_llamacpp_box() -> baml.ai.Provider throws never {         // a llama.cpp server
  let p: baml.ai.Provider = s03_LlamaCpp { base_url: "http://localhost:8080", model: null };
  p
}

function s03_outlines_box() -> baml.ai.Provider throws never {         // in-process, NO HTTP underneath
  let p: baml.ai.Provider = s03_Outlines { model_id: "meta-llama/Llama-3.1-8B-Instruct", device: "cuda" };
  p
}

function s03_hosted() -> baml.ai.Provider throws never {               // schema-JSON only; cannot guarantee shapes
  let p: baml.ai.Provider = s03_OpenAIHosted { model: "gpt-4o", api_key: "unused" };
  p
}

// Two-tier in one client: guarantee on the self-hosted box, degrade to hosted
// schema-JSON when the box is down. Reads like the Fallback chain from the design.
function s03_portable() -> s03_ConstrainedFallback throws never {
  let p = s03_ConstrainedFallback { strategy: [s03_local_llama(), s03_hosted()] };
  p
}

// ---------------------------------------------------------------------------
// 2. functions + their `.decode` companion.
//
// A normal `function` still uses `.call` (unconstrained). The constraint is
// supplied at the constrained companion — the same way a Channel is supplied at
// `.live`. It is NOT a client field: the same client may decode different shapes
// on different calls. We default to a dead client so no offline test hits network
// by accident; the real backend is swapped in at the decode site.
// ---------------------------------------------------------------------------

client<llm> DeadS03 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// CHOICE — guaranteed exactly one enum token, no JSON wrapper.
function s03_Classify(text: string) -> s03_Sentiment {
  client DeadS03
  prompt #"Positive, negative, or neutral?\n{{ text }}"#
}
// app:  s03_classify_decode("I loved it", s03_local_llama(),
//                           s03_Choice { options: ["Positive","Negative","Neutral"] })
//   -> s03_VLLM.decode -> extra_body{guided_choice:[...]} -> "Positive" -> cast_exact<Sentiment>

// REGEX — guaranteed a phone-number-shaped string. T = string.
function s03_PhoneNumber(area: string) -> string {
  client DeadS03
  prompt #"Give me a US phone number in area code {{ area }}."#
}
// app:  decode on s03_local_llama() with s03_Regex { pattern: "\\(\\d{3}\\) \\d{3}-\\d{4}" }
//   -> s03_VLLM -> extra_body{guided_regex:"..."} -> "(415) 555-0142"

// GRAMMAR — guaranteed a SQL statement from a CFG. Runs on llama.cpp (GBNF).
function s03_ToSql(question: string) -> string {
  client DeadS03
  prompt #"Translate to SQL: {{ question }}"#
}
// the grammar the app hands in (GBNF dialect, because the backend is llama.cpp):
//   s03_Grammar { dialect: "gbnf", source: #"
//     root    ::= "SELECT " columns " FROM " table
//     columns ::= "*" | "id" | "name"
//     table   ::= "users" | "orders"
//   "# }

// ---------------------------------------------------------------------------
// 3. The companion the compiler would generate for `.decode`.
//
// The analog of Foo$stream / Foo$live. Written out here to show the negotiation
// `match` an app author RELIES ON but never writes. Note: for Choice/Regex/Grammar
// there is NO honest fallback to `.call`, so the `_` arm errors — unlike `.stream`.
// (The stdlib's `baml.ai.Constrained` also ships without a generated companion,
// precisely because a guarantee can't be synthesized on the degrade path.)
// ---------------------------------------------------------------------------

function s03_classify_decode(text: string, p: baml.ai.Provider, constraint: s03_Constraint) -> s03_Sentiment
    throws s03_ConstraintError | baml.errors.UnknownError {
  match (p) {
    let c: s03_Constrained => {
      let pa = s03_Classify$render_prompt(text) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["render failed"] } };
      c.decode<s03_Sentiment>(pa, constraint)
    },
    _ => throw s03_Unsatisfiable {
      message: "client cannot guarantee shapes; pick a self-hosted provider",
      constraint: constraint,
    },
  }
}

// ---------------------------------------------------------------------------
// 4. Portability across two divergent backends + the degrade case.
//
// The SAME Constraint values flow to vLLM (guided_choice key) and llama.cpp (GBNF
// lowering). The app code is identical; the provider absorbs the divergence.
// ---------------------------------------------------------------------------

function s03_guaranteed_label(text: string, where: string) -> s03_Sentiment
    throws s03_ConstraintError | baml.errors.UnknownError {
  // pick the backend at runtime — clients are functions, so this is ordinary.
  let p = if (where == "vllm") { s03_local_llama() }
          else if (where == "llamacpp") { s03_llamacpp_box() }
          else { s03_outlines_box() };
  let choice = s03_Choice { options: ["Positive", "Negative", "Neutral"] };
  match (p) {
    let c: s03_Constrained => {
      let pa = s03_Classify$render_prompt(text) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["render failed"] } };
      c.decode<s03_Sentiment>(pa, choice)
    },
    _ => throw s03_Unsatisfiable { message: "not constrained-capable", constraint: choice },
  }
}

// The two-tier degrade. On the self-hosted box the enum is GUARANTEED; if the box
// is unreachable the ConstrainedFallback drops to hosted schema-JSON — which can
// only return a JSON-wrapped value validated after the fact, NOT a guarantee. The
// leak is visible: for a Choice, the fallback to hosted THROWS Unsatisfiable,
// because a bare enum token is not a JsonShape. The app must decide to accept that.
function s03_portable_label(text: string) -> s03_Sentiment
    throws s03_ConstraintError | baml.errors.UnknownError {
  let p = s03_portable();                          // ConstrainedFallback[VLLM, OpenAIHosted]
  let choice = s03_Choice { options: ["Positive", "Negative", "Neutral"] };
  let pa = s03_Classify$render_prompt(text) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["render failed"] } };
  // If vLLM is up: guaranteed. If down: Unsatisfiable (hosted can't honor Choice).
  p.decode<s03_Sentiment>(pa, choice)
    catch (e) {
      // app-level recovery, most-specific-first: the EXPECTED miss is Unsatisfiable
      // (hosted tier can't honor a bare-enum Choice). Re-shape the SAME intent as a
      // JSON schema so the hosted tier CAN serve it (best-effort, SAP-validated, not
      // guaranteed). Any other error we re-raise.
      let _u: s03_Unsatisfiable => {
        let p2 = s03_hosted();
        match (p2) {
          let h: baml.ai.HttpProvider => h.call_messages<s03_Sentiment>(baml.ai.prompt_to_messages(pa)) catch (e2) { _ => throw baml.errors.UnknownError { data: e2, message: ["hosted degrade failed"] } },
          _ => throw baml.errors.UnknownError { data: null, message: ["hosted lost HttpProvider"] },
        }
      },
      let c: s03_ConstraintError => throw c,   // network-mid-decode etc. — not recoverable here
      let u: baml.errors.UnknownError => throw u,
    }
}

// ============================================================================
// TESTS — offline, deterministic. Negotiation + request-construction divergence
// + the typed refusals (the leaks). No network, no API keys.
// ============================================================================

// The real stdlib `baml.ai.Constrained` — the honest B1 runtime-promise anchor.
function s03_require_constrained(p: baml.ai.Provider) -> string throws baml.errors.Unsupported {
    match (p) {
        let c: baml.ai.Constrained => "constrained-capable",
        _ => throw baml.errors.Unsupported { message: "provider cannot hard-constrain decoding" },
    }
}

test "03: a provider without Constrained is a typed refusal, not silent degradation" {
    let r = s03_require_constrained(EchoProvider { reply: "x" }) catch (e) {
        let u: baml.errors.Unsupported => "honest refusal",
        _ => "wrong error",
    };
    assert.equal(r, "honest refusal")
}

// The two-tier split IS "implements the capability or doesn't."
function s03_is_constrained(p: baml.ai.Provider) -> bool throws never {
    match (p) {
        let c: s03_Constrained => true,
        _ => false,
    }
}

test "03: self-hosted implements s03_Constrained; hosted does not (two-tier negotiation)" {
    assert.is_true(s03_is_constrained(s03_local_llama()))
    assert.is_true(s03_is_constrained(s03_llamacpp_box()))
    assert.is_true(s03_is_constrained(s03_outlines_box()))
    assert.equal(s03_is_constrained(s03_hosted()), false)
}

// vLLM absorbs each constraint kind into a distinct extra_body key.
test "03: vLLM lowers Choice to guided_choice in extra_body" {
    let v = s03_VLLM { model: "m", base_url: "http://x/v1", api_key: null };
    let req = v.request_with([baml.ai.ChatMessage.user("hi")], s03_Choice { options: ["Positive", "Negative"] })
        catch (e) { _ => baml.http.Request { method: "", url: "", headers: {}, body: "ERR" } };
    assert.is_true(req.body.includes("guided_choice"))
    assert.is_true(req.body.includes("Positive"))
}

test "03: vLLM lowers Regex to guided_regex" {
    let v = s03_VLLM { model: "m", base_url: "http://x/v1", api_key: null };
    let req = v.request_with([baml.ai.ChatMessage.user("hi")], s03_Regex { pattern: "\\d{3}" })
        catch (e) { _ => baml.http.Request { method: "", url: "", headers: {}, body: "ERR" } };
    assert.is_true(req.body.includes("guided_regex"))
}

test "03: vLLM accepts a gbnf/lark grammar but refuses an unknown dialect (typed Unsatisfiable)" {
    let v = s03_VLLM { model: "m", base_url: "http://x/v1", api_key: null };
    let ok = v.request_with([baml.ai.ChatMessage.user("hi")], s03_Grammar { source: "root ::= \"a\"", dialect: "gbnf" })
        catch (e) { _ => baml.http.Request { method: "", url: "", headers: {}, body: "ERR" } };
    assert.is_true(ok.body.includes("guided_grammar"))
    let bad = v.request_with([baml.ai.ChatMessage.user("hi")], s03_Grammar { source: "x", dialect: "peg" }) catch (e) {
        let u: s03_Unsatisfiable => "unsat",
        _ => "wrong error",
    };
    assert.equal(bad, "unsat")
}

// llama.cpp speaks ONE thing (GBNF): it lowers Choice, refuses Regex.
test "03: llama.cpp lowers Choice into a GBNF grammar field" {
    let l = s03_LlamaCpp { base_url: "http://x", model: null };
    let req = l.request_with([baml.ai.ChatMessage.user("hi")], s03_Choice { options: ["yes", "no"] })
        catch (e) { _ => baml.http.Request { method: "", url: "", headers: {}, body: "ERR" } };
    assert.is_true(req.body.includes("grammar"))
    assert.is_true(req.body.includes("root ::="))
}

test "03: llama.cpp refuses a bare Regex (no honest regex->gbnf)" {
    let l = s03_LlamaCpp { base_url: "http://x", model: null };
    let r = l.request_with([baml.ai.ChatMessage.user("hi")], s03_Regex { pattern: "\\d+" }) catch (e) {
        let u: s03_Unsatisfiable => "refused",
        _ => "wrong error",
    };
    assert.equal(r, "refused")
}

test "03: llama.cpp refuses a non-gbnf grammar dialect" {
    let l = s03_LlamaCpp { base_url: "http://x", model: null };
    let r = l.request_with([baml.ai.ChatMessage.user("hi")], s03_Grammar { source: "x", dialect: "lark" }) catch (e) {
        let u: s03_Unsatisfiable => "refused",
        _ => "wrong error",
    };
    assert.equal(r, "refused")
}

test "03: gbnf_from_choice builds an alternation of quoted literals" {
    assert.equal(s03_gbnf_from_choice(["a", "b", "c"]), "root ::= \"a\" | \"b\" | \"c\"")
}

// Outlines: the dispatch + CFG dialect gate are pure (offline); generation is BLOCKED.
test "03: Outlines refuses a non-lark/ebnf CFG dialect at the gate (offline)" {
    let o = s03_Outlines { model_id: "m", device: "cpu" };
    let pa = baml.ai.messages_to_prompt([baml.ai.ChatMessage.user("x")]);
    let r = o.decode<s03_Sentiment>(pa, s03_Grammar { source: "x", dialect: "gbnf" }) catch (e) {
        let u: s03_Unsatisfiable => "refused",
        _ => "other",
    };
    assert.equal(r, "refused")
}

// Outlines dispatch reaches the (BLOCKED: P8) in-process generator for a valid kind.
test "03: Outlines dispatch reaches the in-process generator (BLOCKED: P8 marker)" {
    let o = s03_Outlines { model_id: "m", device: "cpu" };
    let pa = baml.ai.messages_to_prompt([baml.ai.ChatMessage.user("x")]);
    let r = o.decode<string>(pa, s03_Choice { options: ["Positive"] }) catch (e) {
        let u: baml.errors.UnknownError => u.message.join(" "),
        _ => "other",
    };
    assert.is_true(r.includes("BLOCKED: P8"))
}

// The negotiation companion refuses a client that can't guarantee shapes.
test "03: classify_decode refuses a hosted (non-constrained) client with Unsatisfiable" {
    let r = s03_classify_decode("I loved it", s03_hosted(), s03_Choice { options: ["Positive"] }) catch (e) {
        let u: s03_Unsatisfiable => "refused",
        _ => "wrong error",
    };
    assert.equal(r, "refused")
}

// The combinator's empty-claim leak: hosted-only member cannot honor a Choice.
test "03: ConstrainedFallback with only a hosted member throws Unsatisfiable for Choice (the leak)" {
    let fb = s03_ConstrainedFallback { strategy: [s03_hosted()] };
    let pa = baml.ai.messages_to_prompt([baml.ai.ChatMessage.user("x")]);
    let r = fb.decode<string>(pa, s03_Choice { options: ["Positive"] }) catch (e) {
        let u: s03_Unsatisfiable => u.message,
        _ => "wrong error",
    };
    assert.is_true(r.includes("no member supports"))
}

// The combinator DOES select a constrained member (vLLM tried, rejects the dialect
// before any network, chain continues, no honest degrade for Grammar -> Unsatisfiable).
test "03: ConstrainedFallback prefers the constrained member (offline, via dialect miss)" {
    let fb = s03_ConstrainedFallback { strategy: [s03_hosted(), s03_local_llama()] };
    let pa = baml.ai.messages_to_prompt([baml.ai.ChatMessage.user("x")]);
    let r = fb.decode<string>(pa, s03_Grammar { source: "x", dialect: "peg" }) catch (e) {
        let u: s03_Unsatisfiable => u.message,
        _ => "wrong error",
    };
    assert.is_true(r.includes("no member supports"))
}

// or_else_hosted (sugar on a concrete self-hosted provider) builds a fallback.
test "03: or_else_hosted builds a ConstrainedFallback that is itself constrained-capable" {
    let vllm = s03_VLLM { model: "m", base_url: "http://x/v1", api_key: null };
    let fb = vllm.or_else_hosted(s03_hosted());
    assert.equal(fb.strategy.length(), 2)
    assert.is_true(s03_is_constrained(fb))
    let fb2 = fb.add(s03_llamacpp_box());
    assert.equal(fb2.strategy.length(), 3)
}
