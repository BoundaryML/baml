//! BEP-066 ruling (A): an inline `unreflect(value)` type argument is legal
//! only while the runtime type stays out of the expression's published type.
//!
//! The parameter an inline `unreflect(...)` introduces is rigid for exactly
//! one call. A result that IS that parameter (`parse<T>(..) -> T`) is fine —
//! the value carries its own runtime tag and nothing static claims more than
//! `unknown`. A result that EMBEDS it (`Wrapper<T>`, `T[]`, a constructed
//! `Agent<T>`) is not: the published type would keep naming a parameter that
//! no longer exists, and every later dispatch re-derives its arguments from
//! that type. Those spellings are refused with E0168, which suggests the
//! lexical `type Out = unreflect(v)` binding that does outlive a call.

use baml_compiler_diagnostics::Severity;
use baml_project::{collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn compile_errors(source: &str) -> Vec<(String, String)> {
    collect_diagnostics(&setup_test_db(source))
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

/// Render one file's diagnostics the way `baml check` shows them, so the
/// snapshot below pins the headline, the note at the `unreflect(...)` slot and
/// the suggested rewrite together — the three parts users read.
fn render_errors(source: &str) -> String {
    use std::collections::HashMap;

    use baml_compiler_diagnostics::render::{DiagnosticFormat, RenderConfig, render_diagnostics};

    let db = setup_test_db(source);
    let diagnostics: Vec<_> = collect_diagnostics(&db)
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

        function main(t: type, c: ai.Client) -> unknown throws unknown {{
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

function main(t: type) -> unknown {
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

function main(t: type) -> unknown throws unknown {
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

function main(t: type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
"#,
    );
}

/// The same report from the CALL side, whose span and rewrite travel a
/// different road than the class literal's: the slot span comes from
/// `DiagnosticLocation::UnreflectArg` through `AstSourceMap`, and the rewrite
/// is assembled in `TirDiagnostic::render_with_type_refs` from the file text.
/// The code+message assertions above would not notice either degrading.
#[test]
fn the_report_at_a_call_site_names_the_slot_and_spells_the_fix() {
    insta::assert_snapshot!(
        render_errors(
            r#"
class Wrapper<T> { inner T }

function wrap<T>(v: T) -> Wrapper<T> throws never { Wrapper<T> { inner: v } }

function main(t: type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
"#
        ),
        @"
    E0168

      × this runtime type must be given a name before it can be used here
       ╭─[test.baml:6:57]
     6 │ function main(t: type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
       ·                                                         ──────┬─────
       ·                                                               ╰── a type created at runtime only lasts for one call when written inline with `unreflect(...)`, but the value this expression creates would still need it afterwards
       ╰────
      ╰─▶   ☞ name the type first, then use the name:
            │     type Out = unreflect(t);
            │     wrap<Out>(1)
             ╭─[test.baml:6:57]
           6 │ function main(t: type) -> unknown throws unknown { wrap<unreflect(t)>(1) }
             ·                                                         ────────────
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

function main(t: type) -> unknown throws unknown { listed<unreflect(t)>(1) }
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

function main(t: type) -> unknown {
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

        function main(t: type, document: string) -> unknown throws unknown {{
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

        function main(t: type, document: string) -> string throws unknown {{
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

function main(t: type) -> unknown throws unknown { erase<unreflect(t)>(1) }
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

function main(t: type) -> unknown throws unknown {
    type Out = unreflect(t)
    let literal = Holder<Out> { label: "h" }
    let wrapped = wrap<Out>(1)
    wrapped.inner
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
                "name": type.of<string>(),
            }}).as_type()
            {bind}
            let run = ai.Agent<{slot}>.new(
                client = ProbeClient {{ reply: #"{{"name":"Pixel"}}"# }},
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
