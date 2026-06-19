// ============================================================================
// usage.baml — Constrained decoding beyond JSON Schema
// Application-author code.
//
// Shows the ergonomics of the two-tier story:
//   - A `client` selecting a self-hosted backend is one line — config only.
//   - The constrained companion `.decode` is the new entry point, mirroring how
//     `.stream` / `.run_tools` decompose a function.
//   - At least two divergent providers (vLLM and llama.cpp, plus a hosted OpenAI
//     for the degrade case) prove portability — and expose where it leaks.
// ============================================================================

// ---- the output types ------------------------------------------------------

enum Sentiment {
  Positive
  Negative
  Neutral
}

// A SQL string is NOT JSON — this is the case a schema can't express at all.
// The output type is just `string`; the *grammar* carries the guarantee.

// ---------------------------------------------------------------------------
// 1. clients — config only. "Where it runs" is a field of the provider class.
// ---------------------------------------------------------------------------

client LocalLlama() {                          // vLLM serving Llama locally
  VLLM { model: "meta-llama/Llama-3.1-8B-Instruct", base_url: "http://localhost:8000/v1" }
}

client LlamaCppBox() {                         // a llama.cpp server
  LlamaCpp { base_url: "http://localhost:8080" }
}

client OutlinesBox() {                         // in-process, NO HTTP underneath
  Outlines { model_id: "meta-llama/Llama-3.1-8B-Instruct", device: "cuda" }
}

client Hosted() {                              // schema-JSON only; cannot guarantee shapes
  OpenAIHosted { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

// Two-tier in one client: guarantee on the self-hosted box, degrade to hosted
// schema-JSON when the box is down. Reads like the Fallback chain from the proposal.
client Portable() {
  LocalLlama().or_else_hosted(Hosted())        // -> ConstrainedFallback
}

// ---------------------------------------------------------------------------
// 2. functions + their `.decode` companion.
//
// A normal `function` still uses `.call` (unconstrained). The constraint is
// supplied at the constrained companion, the same way a Channel is supplied at
// `.live`. The constraint is NOT a client field — it's a per-call parameter,
// because the same client may decode different shapes on different calls.
// ---------------------------------------------------------------------------

// CHOICE — guaranteed exactly one enum token, no JSON wrapper.
function Classify(text: string) -> Sentiment {
  client: LocalLlama()
  prompt #"Positive, negative, or neutral?\n{{ text }}"#
}
// app calls:  Classify.decode("I loved it", Choice { options: ["Positive","Negative","Neutral"] })
//   -> VLLM.decode  -> extra_body{guided_choice:[...]}  -> "Positive"  -> cast_exact<Sentiment>

// REGEX — guaranteed a phone-number-shaped string. T = string.
function PhoneNumber(area: string) -> string {
  client: LocalLlama()
  prompt #"Give me a US phone number in area code {{ area }}."#
}
// app calls:  PhoneNumber.decode("415", Regex { pattern: r"\(\d{3}\) \d{3}-\d{4}" })
//   -> VLLM.decode -> extra_body{guided_regex:"..."} -> "(415) 555-0142"

// GRAMMAR — guaranteed a SQL statement from a CFG. Runs on llama.cpp (GBNF).
function ToSql(question: string) -> string {
  client: LlamaCppBox()
  prompt #"Translate to SQL: {{ question }}"#
}
// the grammar the app hands in (GBNF dialect, because the client is llama.cpp):
//   ToSql.decode("all users", Grammar {
//     dialect: "gbnf",
//     source: #"
//       root    ::= "SELECT " columns " FROM " table
//       columns ::= "*" | "id" | "name"
//       table   ::= "users" | "orders"
//     "#,
//   })

// ---------------------------------------------------------------------------
// 3. The companion the compiler generates for `.decode`.
//
// This is the analog of Foo$stream / Foo$live in §5 of the proposal. The library
// would generate it; written out here to show the negotiation `match` an app
// author RELIES ON but never writes. Note: for Choice/Regex/Grammar there is no
// honest fallback to `.call`, so the `_` arm errors — unlike `.stream`.
// ---------------------------------------------------------------------------

function Classify$decode(text: string, constraint: Constraint) -> Sentiment {
  let p = LocalLlama();
  match (p) {
    let c: Constrained => c.decode<Sentiment>(render_prompt(text), constraint),
    _                  => throw Unsatisfiable {
      message: "client cannot guarantee shapes; pick a self-hosted provider",
      constraint: constraint,
    },
  }
}

// ---------------------------------------------------------------------------
// 4. Portability proven across two divergent backends + the degrade case.
//
// Same Constraint values flow to vLLM (guided_choice key) and llama.cpp (GBNF
// lowering). The app code is identical; the provider absorbs the divergence.
// ---------------------------------------------------------------------------

function GuaranteedLabel(text: string, where: string) -> Sentiment {
  // pick the backend at runtime — clients are functions, so this is ordinary.
  let p = if (where == "vllm") { LocalLlama() }
          else if (where == "llamacpp") { LlamaCppBox() }
          else { OutlinesBox() };
  let choice = Choice { options: ["Positive", "Negative", "Neutral"] };
  match (p) {
    let c: Constrained => c.decode<Sentiment>(render_prompt(text), choice),
    _                  => throw Unsatisfiable { message: "not constrained-capable", constraint: choice },
  }
}

// The two-tier degrade. On the self-hosted box the enum is GUARANTEED; if the box
// is unreachable the ConstrainedFallback drops to hosted schema-JSON — which can
// only return a JSON-wrapped value validated after the fact, NOT a guarantee. The
// leak is visible: for a Choice, the fallback to Hosted will THROW Unsatisfiable,
// because a bare enum token is not a JsonShape. The app must decide to accept that.
function PortableLabel(text: string) -> Sentiment {
  let p = Portable();                          // ConstrainedFallback[VLLM, OpenAIHosted]
  let choice = Choice { options: ["Positive", "Negative", "Neutral"] };
  // If vLLM is up: guaranteed. If down: Unsatisfiable (hosted can't honor Choice).
  p.decode<Sentiment>(render_prompt(text), choice)
    catch (e) {
      // app-level recovery: re-shape the SAME intent as a JSON schema so the
      // hosted tier CAN serve it (best-effort, validated by SAP, not guaranteed).
      _ => {
        let p2 = Hosted();
        p2.call<Sentiment>(render_prompt(text))   // degrade explicitly, app's choice
      }
    }
}
