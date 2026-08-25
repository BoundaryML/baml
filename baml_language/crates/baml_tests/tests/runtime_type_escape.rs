//! BEP-066 ruling (A): an inline `unreflect(value)` type argument is legal
//! only while the runtime type stays out of the expression's published type.
//!
//! The parameter an inline `unreflect(...)` introduces is rigid for exactly
//! one call. A type that IS that parameter (`parse<T>(..) -> T`) is fine —
//! the value carries its own runtime tag and nothing static claims more than
//! `unknown`. A type that EMBEDS it (`Wrapper<T>`, `T[]`, a constructed
//! `Agent<T>`) is not: the published type would keep naming a parameter that
//! no longer exists, and every later dispatch re-derives its arguments from
//! that type. Those spellings are refused with E0168, which suggests the
//! lexical `type Out = unreflect(v)` binding that does outlive a call.
//!
//! A call publishes three things, and the rule reads the same in all three:
//! the RESULT it returns, the ERROR its `throws` clause hands the caller —
//! written or inferred — and, when a `?.` chain short-circuits it, the
//! `| null` the chain wraps around the result. All three read the published
//! TYPE, never the spelling: a chained call whose result never mentions the
//! parameter (`-> bool`, `-> Wrapper<unknown>`) has nothing for the `| null`
//! to wrap and stays legal. The last section sweeps every other construct that
//! transports a call's result and pins where it lands, because the bar is that
//! no spelling reaches a runtime failure.

use baml_compiler_diagnostics::Severity;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::BexExternalValue;

fn compile_errors(source: &str) -> Vec<(String, String)> {
    check_user_files(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.message))
        .collect()
}

fn assert_escapes(source: &str, expected: usize) {
    let errors = compile_errors(source);
    assert_eq!(
        errors,
        vec![
            (
                "E0168".to_string(),
                "this runtime type must be given a name before it can be used here".to_string(),
            );
            expected
        ],
        "expected {expected} E0168 finding(s) and nothing else for:\n{source}",
    );
}

fn assert_single_escape(source: &str) {
    assert_escapes(source, 1);
}

fn assert_accepted(source: &str) {
    let errors = compile_errors(source);
    assert!(errors.is_empty(), "expected no errors, got: {errors:#?}");
}

/// The exact codes a source reports — for the audited shapes whose refusal is
/// some other rule's to make, so a later change that hands them to E0168 (or
/// to nothing at all) has to come through here.
fn assert_error_codes(source: &str, expected: &[&str]) {
    let codes: Vec<String> = compile_errors(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    assert_eq!(codes, expected, "for:\n{source}");
}

/// Render one file's diagnostics the way `baml check` shows them, so the
/// snapshot below pins the headline, the note at the `unreflect(...)` slot and
/// the suggested rewrite together — the three parts users read.
fn render_errors(source: &str) -> String {
    use std::collections::HashMap;

    use baml_compiler_diagnostics::render::{DiagnosticFormat, RenderConfig, render_diagnostics};

    let db = setup_test_db(source);
    let diagnostics: Vec<_> = check_user_files(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    let mut sources = HashMap::new();
    let mut paths = HashMap::new();
    for file in baml_compiler2_hir::compiler2_all_files(&db) {
        sources.insert(file.file_id(&db), file.text(&db).clone());
        paths.insert(file.file_id(&db), file.path(&db));
    }
    render_diagnostics(
        &diagnostics,
        &sources,
        &paths,
        &RenderConfig {
            format: DiagnosticFormat::Human,
            color: false,
            show_error_codes: true,
        },
    )
}

/// A scripted `ai.Client` that answers with a fixed payload, so the Agent loop
/// runs end to end without a network or an API key. Same shape as the #4501
/// scenario in `runtime_type_bindings.rs`.
const PROBE_CLIENT: &str = r##"
client DefaultClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class ProbeClient {
    reply: string,

    implements ai.Client {
        function id(self) -> string {
            "probe"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                content: [ai.content.Text { text: self.reply }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}
"##;

// ── Refused: the result type would keep naming the call-scoped parameter ────

/// The B-1582 item-1 spelling, verbatim from the ticket. `ai.Agent<T>.new`
/// returns `Agent<T>`, so the constructed value's published type is
/// `Agent<unknown>` while the instance carries the real runtime class — the
/// disagreement that later made `.run` fail to dispatch at all. The spec the
/// runner is handed is the ticket's second inline slot, and `@spec<T>` embeds
/// the parameter the same way, so both slots are named.
#[test]
fn agent_constructed_with_an_inline_runtime_type_is_refused() {
    assert_escapes(
        &format!(
            r##"
        {PROBE_CLIENT}

        function DynamicOutput<T>() -> T {{
            client: DefaultClient
            prompt: `${{ctx.output_format}}`
        }}

        function main(t: reflect.Type, c: ai.Client) -> unknown throws unknown {{
            ai.Agent<unreflect(t)>.new(client = c).run(DynamicOutput@spec<unreflect(t)>())
        }}
        "##
        ),
        2,
    );
}

/// STOP.md's minimal panic repro: a class literal whose type parameter has no
/// other inference source used to leave an error-recovery type in the
/// turbofish, reach MIR and hit `runtime_ty.rs`'s `Error is not a valid
/// RuntimeTy`. It is now a diagnostic, and compilation stops long before
/// lowering.
#[test]
fn class_literal_with_an_inline_runtime_type_is_refused_before_lowering() {
    assert_single_escape(
        r#"
class Holder<T> { label string }

function main(t: reflect.Type) -> unknown {
    Holder<unreflect(t)> { label: "h" }
}
"#,
    );
}

/// The sibling shape from STOP.md's table: the same class parameter written on
/// a static constructor call instead of a literal. It never panicked — it
/// silently erased to `unknown` — and it is the same lie.
#[test]
fn class_constructor_with_an_inline_runtime_type_is_refused() {
    assert_single_escape(
        r#"
class Holder<T> {
    label string

    function new(label: string) -> Holder<T> throws never {
        Holder<T> { label: label }
    }
}

function main(t: reflect.Type) -> unknown throws unknown {
    Holder<unreflect(t)>.new("h")
}
"#,
    );
}

/// A class parameter is not the only way to embed the parameter: a free
/// function returning `Wrapper<T>` publishes the same lying `Wrapper<unknown>`.
#[test]
fn a_result_that_wraps_the_parameter_is_refused() {
    assert_single_escape(
        r#"
class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function main(t: reflect.Type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
"#,
    );
}

/// The same report from the CALL side, whose span and rewrite travel a
/// different road than the class literal's: the slot span comes from
/// `DiagnosticLocation::UnreflectArg` through `AstSourceMap`, and the rewrite
/// is assembled in `TirDiagnostic::render_with_body_type_refs` from the file text.
/// The code+message assertions above would not notice either degrading.
#[test]
fn the_report_at_a_call_site_names_the_slot_and_spells_the_fix() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function main(t: reflect.Type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
"#
        ),
        @"
    E0168

      × this runtime type must be given a name before it can be used here
       ╭─[test.baml:6:65]
     6 │ function main(t: reflect.Type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
       ·                                                                 ──────┬─────
       ·                                                                       ╰── a type created at runtime only lasts for one call when written inline with `unreflect(...)`, but the value this expression creates would still need it afterwards
       ╰────
      ╰─▶   ☞ name the type first, then use the name:
            │     type Out = unreflect(t);
            │     wrap<Out>(1)
             ╭─[test.baml:6:65]
           6 │ function main(t: reflect.Type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
             ·                                                                 ────────────
             ╰────
    "
    );
}

/// …and so does a result under a builtin constructor.
#[test]
fn a_result_under_a_builtin_constructor_is_refused() {
    assert_single_escape(
        r#"
function listed<T>(v: T) -> T[]? throws never { [v] }

function main(t: reflect.Type) -> unknown throws unknown { listed<unreflect(t)>(1) }
"#,
    );
}

/// The whole report, as `baml check` prints it: headline, the note anchored on
/// the `unreflect(...)` slot, and a rewrite quoted back in the author's own
/// source.
#[test]
fn the_report_names_the_slot_and_spells_the_fix() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Holder<T> { label string }

function main(t: reflect.Type) -> unknown {
    Holder<unreflect(t)> { label: "h" }
}
"#
        ),
        @r#"
    E0168

      × this runtime type must be given a name before it can be used here
       ╭─[test.baml:5:12]
     5 │     Holder<unreflect(t)> { label: "h" }
       ·            ──────┬─────
       ·                  ╰── a type created at runtime only lasts for one call when written inline with `unreflect(...)`, but the value this expression creates would still need it afterwards
       ╰────
      ╰─▶   ☞ name the type first, then use the name:
            │     type Out = unreflect(t);
            │     Holder<Out> { label: "h" }
             ╭─[test.baml:5:12]
           5 │     Holder<unreflect(t)> { label: "h" }
             ·            ────────────
             ╰────
    "#
    );
}

// ── Refused: the error the call can throw would keep naming the parameter ──

/// A `throws` clause publishes a type exactly like a result does — the
/// caller's handler binds the error after the call has returned — so a clause
/// that still names the call-scoped parameter is the same lie, and gets the
/// same report with the note worded for the error channel.
#[test]
fn a_declared_throws_that_wraps_the_parameter_is_refused() {
    assert_single_escape(
        r#"
class Boom<T> { payload T }

function risky<T>(v: T) -> int throws Boom<T> { throw Boom<T> { payload: v } }

function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1) }
"#,
    );
}

/// The wrinkle that deferred this half of the rule: a `throws` clause is
/// INFERRED from the body when the author writes none, so the report fires on
/// a callee with no `throws` in sight. Nothing about the refusal changes — the
/// error is just as published either way — and the note is worded for exactly
/// this reading: "the error this call can throw", never "the clause you
/// wrote".
#[test]
fn an_inferred_throws_that_wraps_the_parameter_is_refused() {
    assert_single_escape(
        r#"
class Boom<T> { payload T }

function risky<T>(v: T, fail: bool) -> int {
    if (fail) { throw Boom<T> { payload: v } }
    0
}

function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1, true) }
"#,
    );
}

/// The inferred case, rendered: the report has to read sensibly against a
/// signature whose author never wrote a `throws` clause at all.
#[test]
fn the_report_for_an_inferred_throws_names_the_error_channel() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Boom<T> { payload T }

function risky<T>(v: T, fail: bool) -> int {
    if (fail) { throw Boom<T> { payload: v } }
    0
}

function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1, true) }
"#
        ),
        @"
    E0168

      × this runtime type must be given a name before it can be used here
       ╭─[test.baml:9:51]
     9 │ function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1, true) }
       ·                                                   ──────┬─────
       ·                                                         ╰── a type created at runtime only lasts for one call when written inline with `unreflect(...)`, but the error this call can throw would still need it afterwards
       ╰────
      ╰─▶   ☞ name the type first, then use the name:
            │     type Out = unreflect(t);
            │     risky<Out>(1, true)
             ╭─[test.baml:9:51]
           9 │ function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1, true) }
             ·                                                   ────────────
             ╰────
    "
    );
}

/// Catching the error does not make the type legal: the clause is published by
/// the callee, and the handler is one more place that reads it.
#[test]
fn a_throws_that_wraps_the_parameter_is_refused_even_when_caught() {
    assert_single_escape(
        r#"
class Boom<T> { payload T }

function risky<T>(v: T) -> int throws Boom<T> { throw Boom<T> { payload: v } }

function main(t: reflect.Type) -> unknown {
    risky<unreflect(t)>(1) catch (e) { _ => 0 }
}
"#,
    );
}

// ── Refused: a `?.` chain republishes the result as nullable ───────────────

/// The asymmetry #4518 left open: a declared `-> T?` was refused while the
/// `?.` spelling of the same published `unknown?` was accepted. Both are the
/// parameter under a constructor now — the chain's `| null` is a wrapper like
/// any other.
#[test]
fn an_optional_chained_call_is_refused() {
    assert_single_escape(
        r#"
class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

function main(t: reflect.Type, s: Source?) -> unknown { s?.pick<unreflect(t)>(1) }
"#,
    );
}

/// The `?.` link does not have to sit on the call itself: any short-circuiting
/// link earlier in the chain publishes the tail's result as nullable, and the
/// tail is the call.
#[test]
fn an_earlier_optional_link_refuses_the_call_at_the_end_of_the_chain() {
    assert_single_escape(
        r#"
class Source {
    function again(self) -> Source throws never { self }

    function pick<T>(self, v: T) -> T throws never { v }
}

function main(t: reflect.Type, s: Source?) -> unknown {
    s?.again().pick<unreflect(t)>(1)
}
"#,
    );
}

/// `?.[ ]` short-circuits the same way `?.` does, and the tail is still the
/// call.
#[test]
fn an_optional_index_link_refuses_the_call_at_the_end_of_the_chain() {
    assert_single_escape(
        r#"
class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

function main(t: reflect.Type, xs: Source[]?) -> unknown {
    xs?.[0].pick<unreflect(t)>(1)
}
"#,
    );
}

/// A slot can escape through more than one published type at once — here the
/// result wraps the parameter AND the chain republishes it as nullable. One
/// slot, one report: the user has one thing to fix.
#[test]
fn a_slot_that_escapes_twice_is_reported_once() {
    assert_single_escape(
        r#"
class Wrapper<T> { inner T }

class Source {
    function wrapit<T>(self, v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }
}

function main(t: reflect.Type, s: Source?) -> unknown { s?.wrapit<unreflect(t)>(1) }
"#,
    );
}

/// The whole `?.` report, as `baml check` prints it. The rewrite quotes the
/// chain back with the slot named, so the suggestion is the line the user
/// already wrote.
#[test]
fn the_report_for_an_optional_chain_spells_the_fix_in_the_chains_own_words() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

function main(t: reflect.Type, s: Source?) -> unknown { s?.pick<unreflect(t)>(1) }
"#
        ),
        @"
    E0168

      × this runtime type must be given a name before it can be used here
       ╭─[test.baml:6:65]
     6 │ function main(t: reflect.Type, s: Source?) -> unknown { s?.pick<unreflect(t)>(1) }
       ·                                                                 ──────┬─────
       ·                                                                       ╰── a type created at runtime only lasts for one call when written inline with `unreflect(...)`, but the value this expression creates would still need it afterwards
       ╰────
      ╰─▶   ☞ name the type first, then use the name:
            │     type Out = unreflect(t);
            │     s?.pick<Out>(1)
             ╭─[test.baml:6:65]
           6 │ function main(t: reflect.Type, s: Source?) -> unknown { s?.pick<unreflect(t)>(1) }
             ·                                                                 ────────────
             ╰────
    "
    );
}

// ── Accepted: occurrence-typed values, declared erasure, the lexical form ───

/// `parse<T>(..) -> T` is the shape the whole dynamic path is built on: the
/// result is a VALUE typed by the parameter's occurrence (`unknown`), and its
/// runtime class travels with the value. Widely used in tests and demos; it
/// must keep working.
#[test]
fn a_result_that_is_the_parameter_stays_legal() {
    assert_accepted(&format!(
        r##"
        {PROBE_CLIENT}

        function Extract<T>(document: string) -> T {{
            client: DefaultClient
            prompt: `${{document}} ${{ctx.output_format}}`
        }}

        function main(t: reflect.Type, document: string) -> unknown throws unknown {{
            Extract$parse<unreflect(t)>(document)
        }}
        "##
    ));
}

/// A companion whose result never mentions the parameter consumes the runtime
/// type entirely inside the call.
#[test]
fn a_result_that_never_mentions_the_parameter_stays_legal() {
    assert_accepted(&format!(
        r##"
        {PROBE_CLIENT}

        function Extract<T>(document: string) -> T {{
            client: DefaultClient
            prompt: `${{document}} ${{ctx.output_format}}`
        }}

        function main(t: reflect.Type, document: string) -> string throws unknown {{
            Extract$render_prompt<unreflect(t)>(document).text()
        }}
        "##
    ));
}

/// Declared erasure is the author's contract: `-> Wrapper<unknown>` promises
/// nothing about `T`, so nothing escapes even though the result is a wrapper.
#[test]
fn a_declared_erased_result_stays_legal() {
    assert_accepted(
        r#"
class Wrapper<T> { inner T }

function erase<T>(v: T) -> Wrapper<unknown> throws never {
    Wrapper<unknown> { inner: v }
}

function main(t: reflect.Type) -> unknown throws unknown { erase<unreflect(t)>(1) }
"#,
    );
}

/// The suggested spelling, on the very shapes the inline one is refused for.
#[test]
fn the_lexical_binding_keeps_working_everywhere() {
    assert_accepted(
        r#"
class Holder<T> { label string }

class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function main(t: reflect.Type) -> unknown throws unknown {
    type Out = unreflect(t)
    let literal = Holder<Out> { label: "h" }
    let wrapped = wrap<Out>(1)
    wrapped.inner
}
"#,
    );
}

/// The error channel keeps the same carve-out the result has: an error typed
/// by the bare parameter is an occurrence-typed VALUE, its runtime tag rides
/// on the thrown value, and the handler sees `unknown` — nothing static claims
/// more.
#[test]
fn a_throws_that_is_the_parameter_stays_legal() {
    assert_accepted(
        r#"
function fail<T>(v: T) -> int throws T { throw v }

function main(t: reflect.Type) -> unknown { fail<unreflect(t)>(1) }
"#,
    );
}

/// A clause that never mentions the parameter is the ordinary case, and the
/// overwhelmingly common one: `throws never`, a fixed error class, a string.
#[test]
fn a_throws_that_never_mentions_the_parameter_stays_legal() {
    assert_accepted(
        r#"
class Boom { note string }

function risky<T>(v: T, fail: bool) -> T throws Boom {
    if (fail) { throw Boom { note: "no" } }
    v
}

function main(t: reflect.Type) -> unknown { risky<unreflect(t)>(1, false) }
"#,
    );
}

/// A `?.` link that short-circuits nothing publishes no wrapper: the receiver
/// is not nullable, the chain adds no `| null`, and the call's result is the
/// bare parameter it always was. The rule reads the published type, not the
/// punctuation.
#[test]
fn an_inert_optional_link_publishes_no_wrapper_and_stays_legal() {
    assert_accepted(
        r#"
class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

function main(t: reflect.Type, s: Source) -> unknown { s?.pick<unreflect(t)>(1) }
"#,
    );
}

/// The chain rule reads the published type too, not the punctuation. A tail
/// whose result never mentions the parameter publishes nothing about the
/// runtime type, so there is nothing for the chain's `| null` to wrap — and no
/// honest note to write, since the value this expression creates does not need
/// the type afterwards.
#[test]
fn a_chain_tail_whose_result_never_mentions_the_parameter_stays_legal() {
    assert_accepted(
        r#"
class Source {
    function put<T>(self, v: T) -> bool throws never { true }
}

function main(t: reflect.Type, s: Source?) -> unknown { s?.put<unreflect(t)>(1) }
"#,
    );
}

/// The `?.` spelling of #4518's declared-erasure row: `-> Wrapper<unknown>` is
/// the author's own contract that nothing about `T` is published, and a chain
/// around it does not change that. Accepted straight, accepted chained.
#[test]
fn a_chain_tail_with_a_declared_erased_result_stays_legal() {
    assert_accepted(
        r#"
class Wrapper<T> { inner T }

class Source {
    function erase<T>(self, v: T) -> Wrapper<unknown> throws never {
        Wrapper<unknown> { inner: v }
    }
}

function main(t: reflect.Type, s: Source?) -> unknown { s?.erase<unreflect(t)>(1) }
"#,
    );
}

/// Only the chain's TAIL is republished as nullable. A call in argument
/// position inside a chain hands its value straight to the parameter it fills,
/// so it keeps the verdict its own signature earned.
#[test]
fn a_call_in_argument_position_inside_a_chain_stays_legal() {
    assert_accepted(
        r#"
class Source {
    function keep(self, v: unknown) -> string throws never { "kept" }
}

function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type, s: Source?) -> unknown {
    s?.keep(ident<unreflect(t)>(1))
}
"#,
    );
}

// ── Audit: every other construct that transports a call's result ───────────
//
// The parameter an inline `unreflect(...)` introduces can only reach a
// published type through the callee's own signature (result and `throws`,
// both checked above), through a class literal's `C<…T…>` (checked at
// lowering), or through a wrapper the compiler itself adds — the `?.` chain's
// `| null`. Everything else downstream consumes the SUBSTITUTED type, which is
// `unknown` (or the parameter's first bound), and can no longer name the
// parameter at all. These pin that reading for the transports that looked like
// candidates.

/// `spawn` wraps the block's value in a `Future<V, E>` — but `V` is the
/// already-erased `unknown`, so the future publishes nothing about the
/// parameter, and awaiting hands the value back with its runtime tag intact.
#[test]
fn spawning_and_awaiting_the_result_stays_legal() {
    assert_accepted(
        r#"
function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type) -> unknown throws unknown {
    let running = spawn { ident<unreflect(t)>(1) }
    await running
}
"#,
    );
}

/// …and a result that escapes is refused inside a `spawn` exactly as it is
/// outside one: the report is made where the call is written.
#[test]
fn spawning_an_escaping_call_is_still_refused() {
    assert_single_escape(
        r#"
class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function main(t: reflect.Type) -> unknown throws unknown {
    let running = spawn { wrap<unreflect(t)>(1) }
    await running
}
"#,
    );
}

/// A `catch` joins the call's published type with its handlers'. The call
/// published `unknown`, so the join does too.
#[test]
fn catching_the_result_stays_legal() {
    assert_accepted(
        r#"
function ident<T>(v: T) -> T throws string { v }

function main(t: reflect.Type) -> unknown throws unknown {
    ident<unreflect(t)>(1) catch (e) { _ => 0 }
}
"#,
    );
}

/// A `match` arm join is the same story: the arms contribute their published
/// types, and this call's is the erased one.
#[test]
fn joining_the_result_in_match_arms_stays_legal() {
    assert_accepted(
        r#"
function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type, flag: bool) -> unknown throws unknown {
    match flag {
        true => ident<unreflect(t)>(1),
        false => 0,
    }
}
"#,
    );
}

/// A streaming call publishes `T` as a stream of partials, so the result rule
/// already refuses it — on top of the older guard that says a streaming call
/// carries no runtime type arguments at all. Both refusals are the same
/// answer; the shape is pinned so it cannot quietly become neither.
#[test]
fn a_streaming_call_with_an_inline_runtime_type_is_refused_twice() {
    assert_error_codes(
        &format!(
            r##"
        {PROBE_CLIENT}

        function Extract<T>(document: string) -> T {{
            client: DefaultClient
            prompt: `${{document}} ${{ctx.output_format}}`
        }}

        function main(t: reflect.Type, document: string) -> unknown throws unknown {{
            Extract$stream<unreflect(t)>(document)
        }}
        "##
        ),
        &["E0010", "E0168"],
    );
}

/// There is no postfix `!` to transport anything through: BAML has no non-null
/// assertion operator, and the parser says so by name. Pinned here because the
/// audit's answer for that shape is "the spelling does not exist", and this is
/// what makes that answer checkable.
#[test]
fn a_postfix_bang_is_not_a_spelling_this_rule_has_to_cover() {
    assert_error_codes(
        r#"
function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type) -> unknown throws unknown { ident<unreflect(t)>(1)! }
"#,
        &["E0010"],
    );
}

/// A spread cannot transport the result at all: it demands a known class
/// shape, and the erased `unknown` is not one. Nothing to publish, nothing to
/// escape — the refusal is the ordinary type error, made long before this
/// rule.
#[test]
fn a_spread_of_the_result_is_refused_by_its_own_rule() {
    assert_error_codes(
        r#"
class Point { x int, y int }

function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type) -> unknown throws unknown {
    Point { ...ident<unreflect(t)>(Point { x: 1, y: 2 }), x: 3 }
}
"#,
        &["E0001"],
    );
}

/// An array spread has no spelling in the grammar at all, so there is no road
/// for a result to travel that way. Pinned because the audit's answer is again
/// "this shape does not exist", and a grammar that grew one should come back
/// through this rule.
#[test]
fn an_array_spread_is_not_a_spelling_this_rule_has_to_cover() {
    assert_error_codes(
        r#"
function listed<T>(v: T) -> T[] throws never { [v] }

function main(t: reflect.Type) -> unknown { [...listed<unreflect(t)>(1)] }
"#,
        &["E0010"],
    );
}

/// An annotation cannot name a call-scoped parameter — there is no spelling
/// for it — so a binding's declared type can only ever hold the erased one.
#[test]
fn an_annotated_binding_of_the_result_stays_legal() {
    assert_accepted(
        r#"
function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type) -> unknown {
    let held: unknown = ident<unreflect(t)>(1)
    held
}
"#,
    );
}

/// A container literal built from the result is typed from the erased element,
/// never from the parameter.
#[test]
fn collecting_the_result_into_a_literal_stays_legal() {
    assert_accepted(
        r#"
function ident<T>(v: T) -> T throws never { v }

function main(t: reflect.Type) -> unknown throws unknown {
    let collected = [ident<unreflect(t)>(1), ident<unreflect(t)>(2)]
    collected
}
"#,
    );
}

/// `unreflect` is contextual syntax in one exact position. It stays an
/// ordinary identifier everywhere else — as a user's own class name, as a
/// local binding, and as a function called with the very `unreflect(` shape
/// the type-argument lookahead keys on.
#[test]
fn unreflect_outside_a_type_argument_slot_is_an_ordinary_name() {
    assert_accepted(
        r#"
class unreflect { value int }

function unreflect_of(value: int) -> unreflect throws never {
    unreflect { value: value }
}

function main() -> int throws unknown {
    let unreflect = unreflect_of(3)
    unreflect.value
}
"#,
    );
    assert_accepted(
        r#"
class Marker { value int }

function unreflect(value: int) -> Marker throws never {
    Marker { value: value }
}

function main() -> int throws unknown { unreflect(3).value }
"#,
    );
}

// ── The suggestion, applied ────────────────────────────────────────────────

/// The refusal is only useful if its suggestion works. This is the ticket's
/// Agent scenario (the #4501 oracle) rewritten exactly as E0168 spells it: the
/// same program with `type Out = unreflect(...)` in front and `Out` in the
/// slots. It compiles, dispatches through `implements Runner<Out>`, and parses
/// the reflected output type.
#[tokio::test]
async fn applying_the_suggestion_compiles_and_runs() {
    let scenario = |bind: &str, slot: &str| {
        format!(
            r##"
        {PROBE_CLIENT}

        function DynamicOutput<T>() -> T {{
            client: DefaultClient
            prompt: `${{ctx.output_format}}`
        }}

        function main() -> string throws unknown {{
            let output_type = reflect.class.new("RuntimeOutput", {{
                "name": reflect.Type.of<string>(),
            }}).as_type()
            {bind}
            let run = ai.Agent<{slot}>.new(
                client = ProbeClient {{ reply: `{{"name":"Pixel"}}` }},
            ).run(DynamicOutput@spec<{slot}>())
            reflect.class.get_field<string>(run.value, "name")
        }}
        "##
        )
    };

    assert_escapes(&scenario("", "unreflect(output_type)"), 2);

    // Exactly the rewrite E0168 prints: name the type first, then use the name.
    let applied = scenario("type Out = unreflect(output_type)", "Out");
    assert_accepted(&applied);
    let output = baml_test!(&applied);
    assert_eq!(output.result, Ok(BexExternalValue::String("Pixel".into())));
}

/// The suggestion has to work for the OTHER escape too, and this is the shape
/// that caught a real defect: with the caller's own `throws` clause omitted,
/// the named rewrite used to reach MIR still carrying the block-scoped
/// parameter in the effect channel and abort with "type variable not found in
/// type args: Out". The inline spelling was refused, so the only road out of
/// E0168 was the one that crashed.
///
/// Now the block erases its parameter from the effect channel exactly as it
/// does from the value: the program compiles, runs, and publishes
/// `Boom<unknown>` — which the third case reads back out of the contract
/// violation, the one place that prints the effect a body may throw.
#[tokio::test]
async fn applying_the_suggestion_to_a_throws_escape_compiles_and_runs() {
    let scenario = |bind: &str, slot: &str, clause: &str| {
        format!(
            r#"
class Boom<T> {{ payload T }}

function risky<T>(v: T, fail: bool) -> int throws Boom<T> {{
    if (fail) {{ throw Boom<T> {{ payload: v }} }}
    0
}}

function main() -> int{clause} {{
    let t = reflect.Type.of<string>()
    {bind}
    risky<{slot}>("x", false)
}}
"#
        )
    };

    assert_single_escape(&scenario("", "unreflect(t)", ""));

    // Exactly the rewrite E0168 prints, on a caller that declares no clause of
    // its own — the effect is inferred, which is what used to leak.
    let applied = scenario("type Out = unreflect(t)", "Out", "");
    assert_accepted(&applied);
    let output = baml_test!(&applied);
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));

    // The effect the caller publishes, quoted back by the contract check: the
    // erased `Boom<unknown>`, never the block-scoped `Boom<Out>`.
    assert_eq!(
        compile_errors(&scenario("type Out = unreflect(t)", "Out", " throws never")),
        vec![(
            "E0096".to_string(),
            "declared throws is `never`, but this function may also throw `Boom<unknown>`"
                .to_string(),
        )],
    );
}

/// The other half of that erasure, and the reason it stops where it does. A
/// block-scoped name is erased from what LEAVES the block — the effect the
/// enclosing function publishes, and the compiler-derived copy of it a stashed
/// violation quotes — and from nothing else. A clause the author WROTE inside
/// the block is quoted back verbatim, at a caret one row under the line that
/// spells it, where the name is in scope and on screen.
///
/// So one program reports both spellings, deliberately: the lambda's own
/// `Boom<Out>` as written, and the owner's published `Boom<unknown>`.
#[test]
fn a_lambda_clause_inside_the_block_is_quoted_as_written() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Boom<T> { payload T }

function main(t: reflect.Type) -> unknown throws never {
    type Out = unreflect(t)
    let f = (v: int) -> int throws Boom<Out> { throw "plain" }
    f(1)
}
"#
        ),
        @r#"
    E0096

      × declared throws is `Boom<Out>`, but this function may also throw `string`
       ╭─[test.baml:6:54]
     6 │     let f = (v: int) -> int throws Boom<Out> { throw "plain" }
       ·                                                      ───────
       ╰────

    E0096

      × declared throws is `never`, but this function may also throw `Boom<unknown>`
       ╭─[test.baml:7:5]
     7 │     f(1)
       ·     ─
       ╰────
    "#
    );
}

/// The bar the audit is held to: a shape that compiles must also RUN. Every
/// accepted transport above, on one runtime type, asking each value what class
/// it actually carries — the tag rides on the value, so all five agree, and
/// none of them reaches a runtime failure.
#[tokio::test]
async fn the_accepted_transports_run_and_keep_the_runtime_type() {
    let program = r#"
class Source {
    function pick<T>(self, v: T) -> T throws never { v }
}

function ident<T>(v: T) -> T throws string { v }

function main() -> string throws unknown {
    let t = reflect.Type.of<string>()
    let src = Source {}
    let direct = ident<unreflect(t)>("a")
    let awaited = await spawn { ident<unreflect(t)>("b") }
    let caught = ident<unreflect(t)>("c") catch (e) { _ => 0 }
    let collected = [ident<unreflect(t)>("d")]
    let chained = src?.pick<unreflect(t)>("e")
    `${reflect.Type.of_value(direct).to_string()} ${reflect.Type.of_value(awaited).to_string()} ${reflect.Type.of_value(caught).to_string()} ${reflect.Type.of_value(collected[0]).to_string()} ${reflect.Type.of_value(chained).to_string()}`
}
"#;
    assert_accepted(program);
    let output = baml_test!(program);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "string string string string string".into()
        ))
    );
}
