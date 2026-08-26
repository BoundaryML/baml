//! Phase 3A must-fix gap tests.
//!
//! Each test documents a gap from the Phase 3A checklist. Snapshots capture
//! the current (possibly incorrect) behavior so regressions are visible
//! as the gaps get fixed.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

#[test]
fn explicit_local_id_is_structural_call_metadata() {
    use baml_compiler2_hir_ty::infer::ParamBinding;
    use baml_compiler2_ppir::item_data::{file_functions, function_data};

    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function choose<T>(value: T, fallback: T = value) -> T {
  value
}

function main(id: boundary.LocalId) -> int {
  choose(1, $id = id)
}
"#,
    );

    let rendered = render_tir(&db, file);
    assert!(
        !rendered.contains("!!"),
        "a trailing LocalId side channel must compile cleanly:\n{rendered}"
    );

    let main_loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == "main")
        .expect("main function");
    let inference = baml_compiler2_hir_ty::infer::infer_body(
        &db,
        baml_compiler2_hir::body::BodyOwnerId::Function(main_loc),
    );
    let plans = inference.call_plans.iter().collect::<Vec<_>>();
    assert_eq!(plans.len(), 1, "main contains exactly one call: {rendered}");

    let plan = plans[0].1;
    assert!(
        plan.runtime_id.is_some(),
        "CallPlan must retain the explicit LocalId expression"
    );
    assert_eq!(
        plan.bindings
            .iter()
            .filter(|binding| matches!(binding, ParamBinding::Provided { .. }))
            .count(),
        1,
        "the LocalId must not count as an ordinary argument"
    );
    assert!(matches!(
        plan.bindings.as_slice(),
        [
            ParamBinding::Provided { param_index: 0, .. },
            ParamBinding::OmittedDefault { param_index: 1, .. }
        ]
    ));
    assert_eq!(
        plan.type_args.len(),
        1,
        "generic inference must still record only the ordinary argument's T"
    );
}

#[test]
fn explicit_local_id_has_targeted_call_diagnostics() {
    let cases = [
        (
            "positional_after_id",
            "target($id = id, 1)",
            "`$id` must be the final call argument",
        ),
        (
            "named_after_id",
            "target($id = id, x = 1)",
            "`$id` must be the final call argument",
        ),
        (
            "duplicate_id",
            "target(1, $id = id, $id = id)",
            "duplicate `$id` call argument",
        ),
        (
            "wrong_type",
            "target(1, $id = \"not-a-local-id\")",
            "`$id` at a call site expects `boundary.LocalId`, got",
        ),
        (
            "missing_ordinary_arg",
            "target($id = id)",
            "expected 1 argument(s), got 0",
        ),
    ];

    for (label, call, expected) in cases {
        let mut db = make_db();
        let source = format!(
            r#"
function target(x: int) -> int {{ x }}
function main(id: boundary.LocalId) -> int {{
  {call}
}}
"#
        );
        let file = db.file("test.baml", &source);
        let rendered = render_tir(&db, file);
        assert!(
            rendered.contains(expected),
            "[{label}] expected {expected:?}, got:\n{rendered}"
        );
    }
}

#[test]
fn explicit_local_id_preserves_real_named_argument_diagnostics() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function target(a: int, b: int) -> int { a + b }
function main(id: boundary.LocalId) -> int {
  target(a = 1, $id = id)
}
"#,
    );

    let rendered = render_tir(&db, file);
    assert!(
        rendered.contains("missing required argument `b`"),
        "a real named argument must retain per-parameter diagnostics:\n{rendered}"
    );
    assert!(
        !rendered.contains("expected 2 argument(s), got 1"),
        "the trailing LocalId must not hide the real named argument:\n{rendered}"
    );
}

#[test]
fn explicit_local_id_on_native_target_remains_a_runtime_contract() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function main(id: boundary.LocalId) -> string {
  baml.json.to_string(7, $id = id) catch (e) {
    baml.errors.InvalidArgument => "caught"
  }
}
"#,
    );
    let rendered = render_tir(&db, file);
    assert!(
        !rendered.contains("!!"),
        "TIR must allow the VM to throw the catchable native-target error:\n{rendered}"
    );
}

#[test]
fn backtick_llm_function_compiles_to_agent_loop() {
    // Single-path world: a backtick prompt in an LLM function desugars to the
    // ai Agent loop — the direct-call body runs
    // `ai.Agent<Out>.new(client = client).run(Greet@spec(...))` and the
    // `Greet$spec` companion builds the bound `ai.FunctionSpec`. The `${name}`
    // interp captures the function param inside the spec's prompt closure.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
client MyClient = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "k");

function Greet(name: string) -> string {
  client: MyClient
  prompt: `Hello ${name}!`
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("!!"),
        "backtick LLM function should compile clean, got:\n{tir}"
    );
    assert!(
        tir.contains("Greet$spec") && tir.contains("FunctionSpec"),
        "the spec companion should build an ai.FunctionSpec, got:\n{tir}"
    );
}

#[test]
fn llm_companions_name_defaulted_spec_arguments() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
client MyClient = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "k");

function Greet(name: string, suffix: string = "!") -> string {
  client: MyClient
  prompt: `Hello ${name}${suffix}`
}

function main() -> string {
  Greet$render_prompt("Ada").text()
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("!!"),
        "defaulted LLM companions should compile without diagnostics:\n{tir}"
    );
    assert!(
        tir.contains("Greet$render_prompt(name: string, suffix: string = \"!\")")
            && tir.contains("Greet$build_request(name: string, suffix: string = \"!\", client:")
            && tir.contains("suffix = suffix"),
        "render-prompt/build-request companions must preserve the defaulted argument as named:\n{tir}"
    );
    assert!(
        tir.contains(
            "ai.Agent.new(client = client, on_event = on_event).run(Greet$spec(name, suffix = suffix)).value"
        ),
        "the direct-call companion must name the defaulted spec argument and thread on_event:\n{tir}"
    );
    // Bind the argument tuple to the from_spec call itself: the tuple must
    // appear right after this marker's generic args, not just anywhere in the
    // rendered TIR.
    let stream_call = tir
        .split("ai.stream.from_spec<")
        .nth(1)
        .unwrap_or_else(|| panic!("stream companion must call ai.stream.from_spec:\n{tir}"));
    let window = &stream_call[..stream_call.len().min(200)];
    assert!(
        window
            .contains("(Greet$spec(name, suffix = suffix), client = client, on_event = on_event)"),
        "the stream companion must pass spec, client, and on_event to from_spec:\n{tir}"
    );
}

#[test]
fn new_mode_failures_have_good_diagnostics() {
    // BEP-049 M5: every way a new-mode (backtick) `prompt` can go wrong must
    // surface a diagnostic that points at the user's `${…}` source with a
    // user-facing message — never a `0..0` span and never a leaked internal
    // desugaring type. The prompt body is lowered into a synthesized
    // `(ctx: ai.Context) -> PromptAst` closure, so the risk is that
    // errors land on compiler-generated nodes. These cases pin that they don't.
    //
    // `expect_substr` is asserted; the span column is checked to be non-`0..0`
    // for every emitted diagnostic.
    let cases = [
        // (label, client clause, prompt body, a phrase the diagnostic must contain)
        (
            "undef_var",
            "client: C",
            "prompt: `Hi ${nobody}!`",
            "unresolved name: nobody",
        ),
        (
            "ctx_bad_field",
            "client: C",
            "prompt: `${ctx.nope}`",
            "has no member `nope`",
        ),
        (
            "arith_type_err",
            "client: C",
            r#"prompt: `${1 + "a"}`"#,
            "operator `+`",
        ),
        // `ctx.output_format_with` is removed surface: only `output_format`
        // exists on the spec ctx, so this is a member error now.
        (
            "removed_ctx_method",
            "client: C",
            "prompt: `${ctx.output_format_with(5)}`",
            "has no member `output_format_with`",
        ),
        (
            "bare_output_format",
            "client: C",
            "prompt: `${ctx.output_format}`",
            "`output_format` must be called; use `output_format()`",
        ),
        (
            "bad_client",
            "client: Nope",
            "prompt: `Hi ${name}!`",
            "unresolved name: Nope",
        ),
        // Block-tag interps (`${for}`) must also report at the user's
        // source. (A non-bool `${if}` condition is intentionally NOT an error —
        // it matches plain `if`/`while`, which BAML does not bool-check.)
        (
            "for_non_iterable",
            "client: C",
            "prompt: `${for (let x in 5)}${x}${endfor}`",
            "cannot iterate over type `5`",
        ),
        (
            "for_body_type_err",
            "client: C",
            r#"prompt: `${for (let x in [1, 2])}${x + "a"}${endfor}`"#,
            "operator `+`",
        ),
        (
            "role_marker_requires_name",
            "client: C",
            "prompt: `${role()}hi`",
            "expected 1 argument(s), got 0",
        ),
    ];
    for (label, client, body, expect_substr) in cases {
        let mut db = make_db();
        let src = format!(
            "client C = openai.ResponsesClient.new(model = \"m\", api_key = \"k\");\n\nfunction Greet(name: string) -> string {{\n  {client}\n  {body}\n}}\n"
        );
        let file = db.file("test.baml", &src);
        let tir = render_tir(&db, file);
        let diags: Vec<&str> = tir
            .lines()
            .map(str::trim_start)
            .filter(|l| l.starts_with("!!"))
            .collect();
        assert!(
            !diags.is_empty(),
            "[{label}] expected a diagnostic, got clean TIR:\n{tir}"
        );
        assert!(
            diags.iter().any(|d| d.contains(expect_substr)),
            "[{label}] expected a diagnostic containing {expect_substr:?}, got:\n{}",
            diags.join("\n")
        );
        assert!(
            !diags.iter().any(|d| d.starts_with("!! 0..0:")),
            "[{label}] a diagnostic collapsed to a 0..0 span (internal node leaked):\n{}",
            diags.join("\n")
        );
    }
}

#[test]
fn output_format_must_be_called_on_context_values() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function bare(render_ctx: ai.Context) -> string {
  `${render_ctx.output_format}`
}

function called(render_ctx: ai.Context) -> string {
  `${render_ctx.output_format()}`
}

function stringified(render_ctx: ai.Context) -> string {
  `${render_ctx.output_format.to_string()}`
}
"#,
    );
    let tir = render_tir(&db, file);
    let message = "`output_format` must be called; use `output_format()`";

    assert_eq!(
        tir.matches(message).count(),
        2,
        "bare output_format references should fail while calls remain valid:\n{tir}"
    );
}

#[test]
fn nested_lambda_diagnostic_has_real_span() {
    // Regression: a type error inside a nested lambda body must point at the
    // offending expression, not collapse to a `0..0` span. The lambda body is
    // inferred *inline* in the enclosing scope, so its diagnostics carry the
    // lambda's arena IDs; their spans are frozen against the lambda's source
    // map (see `InferContext::freeze_diagnostic_spans_from`). Before the fix
    // this rendered as `!! 0..0: operator `+` ...`.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f() -> () -> int throws never {\n  let g = () -> { let x: int = 5; let y: string = \"a\"; x + y }\n  g\n}\n",
    );
    let tir = render_tir(&db, file);
    let diag = tir
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with("!!") && l.contains("operator `+`"))
        .unwrap_or_else(|| panic!("expected an operator `+` type error, got:\n{tir}"));
    assert!(
        !diag.starts_with("!! 0..0:"),
        "nested-lambda binary-op error must have a real span, got: {diag}"
    );
}

// ── 3A-1. Union normalization ────────────────────────────────────────────

#[test]
fn union_normalization_deduplicates() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f(x: int | int) -> int { return x; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | int) -> int throws never {
      { : never
        return x : int
      }
    }
    ");
}

#[test]
fn union_normalization_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "type A = int | string\nfunction f(x: A) -> string { return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    type user.A = int | string
    function user.f(x: user.A) -> string throws never {
      { : never
        return x : user.A
      }
      !! 58..59: type mismatch: expected string, got A
    }
    type user.A$stream = int | string
    ");
}

// ── 3A-2. UnknownType diagnostic ─────────────────────────────────────────

#[test]
fn unknown_type_in_param() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(x: Nonexistent) -> int { return 0; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: !error) -> int throws never {
      { : never
        return 0 : 0
      }
      !! 14..25: unresolved type: Nonexistent
    }
    ");
}

#[test]
fn unknown_type_in_return() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> DoesNotExist { return 0; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> !error throws never {
      { : never
        return 0 : 0
      }
      !! 16..28: unresolved type: DoesNotExist
    }
    ");
}

// ── 3A-3. UnresolvedName diagnostic ──────────────────────────────────────

#[test]
fn unresolved_variable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f() -> int { return nonexistent_var; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        return nonexistent_var : !error
      }
      !! 29..44: unresolved name: nonexistent_var
    }
    ");
}

#[test]
fn unresolved_variable_in_let() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f() -> int { let x = unknown_thing; return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        let x = unknown_thing : !error
        return x : !error
      }
      !! 30..43: unresolved name: unknown_thing
    }
    ");
}

#[test]
fn property_shorthand_suggests_explicit_mapping_for_nearby_variable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(option: string) -> map<string, string> {
  { options }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains(
            "property shorthand `options` requires an in-scope value named `options`. Did you \
             mean `options: option`?"
        ),
        "expected a shorthand-specific near-match diagnostic, got:\n{tir}"
    );
    assert!(
        !tir.contains("unresolved name: options"),
        "shorthand should not fall back to the generic unresolved-name diagnostic:\n{tir}"
    );
}

/// Shorthand-ness is the parser's fact, not key text: a WRITTEN
/// `{ "key": key }` is an ordinary entry, so an unbound value reports the
/// generic unresolved-name diagnostic, never the shorthand rewrite hint.
#[test]
fn quoted_key_matching_value_name_is_not_property_shorthand() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(v: string?) -> map<string, string> {
  if let other: string = v { { "key": key } } else { {} }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("unresolved name: key"),
        "a quoted entry with an unbound value is a plain unresolved name, got:\n{tir}"
    );
    assert!(
        !tir.contains("property shorthand"),
        "a quoted key is not shorthand and must not draw the shorthand diagnostic:\n{tir}"
    );
}

/// The shorthand value is an ordinary path expression, so it is in scope
/// exactly when a plain use is - including under a pattern binder, which the
/// body-scope-only binding list could not see.
#[test]
fn property_shorthand_resolves_pattern_binders() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(v: string?) -> map<string, string> {
  if let key: string = v { { key } } else { {} }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("property shorthand"),
        "an `if let` binder satisfies the shorthand, got:\n{tir}"
    );
    assert!(
        !tir.contains("unresolved name"),
        "an `if let` binder satisfies the shorthand, got:\n{tir}"
    );
}

/// Near-match candidates come from the EXPRESSION's scope, so a pattern
/// binder is offered as the explicit-mapping suggestion.
#[test]
fn property_shorthand_suggests_a_pattern_binder() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(v: string?) -> map<string, string> {
  if let option: string = v { { options } } else { {} }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains(
            "property shorthand `options` requires an in-scope value named `options`. Did you \
             mean `options: option`?"
        ),
        "expected the `if let` binder as a near match, got:\n{tir}"
    );
}

#[test]
fn property_shorthand_in_parameter_default_uses_structural_syntax() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(option: string, config: map<string, string> = { options }) -> map<string, string> {
  config
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains(
            "property shorthand `options` requires an in-scope value named `options`. Did you \
             mean `options: option`?"
        ),
        "expected a shorthand diagnostic in the parameter-default arena, got:\n{tir}"
    );
}

#[test]
fn property_shorthand_uses_the_value_expression_scope() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(input: string?) -> map<string, string> {
  if let option: string = input {
    { options }
  } else {
    {}
  }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains(
            "property shorthand `options` requires an in-scope value named `options`. Did you \
             mean `options: option`?"
        ),
        "expected the if-let binding in shorthand suggestions, got:\n{tir}"
    );
}

#[test]
fn explicit_quoted_map_key_is_not_property_shorthand() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build() -> map<string, string> {
  { "options": options }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("unresolved name: options"),
        "expected the ordinary unresolved-name diagnostic, got:\n{tir}"
    );
    assert!(
        !tir.contains("property shorthand"),
        "an explicit quoted key must not use shorthand diagnostics:\n{tir}"
    );
}

#[test]
fn if_let_binding_resolves_in_property_shorthand() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function build(input: string?) -> map<string, string> {
  if let options: string = input {
    { options }
  } else {
    {}
  }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("property shorthand") && !tir.contains("unresolved name"),
        "the ordinary resolver should see the if-let binding:\n{tir}"
    );
}

#[test]
fn class_property_shorthand_suggests_field_to_variable_mapping() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Config { options string }
function build(option: string) -> Config {
  Config { option }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains(
            "class `Config` has no field `option` for property shorthand. Did you mean \
             `options: option`?"
        ),
        "expected an exact-field-name shorthand diagnostic, got:\n{tir}"
    );
    assert!(
        !tir.contains("unresolved name: option"),
        "the shorthand value resolves; only the class-field mismatch should be diagnosed:\n{tir}"
    );
}

#[test]
fn explicit_unknown_class_field_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Config { goodField int }
function build() -> Config {
  Config { badField: 5 }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("class `Config` has no field `badField`"),
        "expected an unknown-field diagnostic for an explicit property, got:\n{tir}"
    );
    assert!(
        !tir.contains("property shorthand"),
        "explicit properties should use the general unknown-field diagnostic:\n{tir}"
    );
}

#[test]
fn explicit_same_name_unknown_class_field_is_not_shorthand() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Config { goodField int }
function build(badField: int) -> Config {
  Config { badField: badField }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("class `Config` has no field `badField`"),
        "expected an unknown-field diagnostic, got:\n{tir}"
    );
    assert!(
        !tir.contains("property shorthand"),
        "an explicit same-name field must not use shorthand diagnostics:\n{tir}"
    );
}

#[test]
fn inferred_object_rejects_explicit_unknown_class_field() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Config { goodField int }
function build() -> Config {
  let config = Config { badField: 5 };
  config
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("class `Config` has no field `badField`"),
        "expected inference to diagnose an explicit unknown field, got:\n{tir}"
    );
    assert!(
        !tir.contains("property shorthand"),
        "explicit properties should use the general unknown-field diagnostic:\n{tir}"
    );
}

#[test]
fn unresolved_function_call_reports_callee_span() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function Main() -> int {
  MissingFunction(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.Main() -> int throws never {
      { : !error
        MissingFunction(1) : !error
      }
      !! 28..43: unresolved name: MissingFunction
    }
    ");
}

#[test]
fn unresolved_function_call_in_testset_reports_call_site_span() {
    let mut db = make_db();
    let source = r#"
function ReviewInvoicee() -> int {
  1
}

testset "invoice pipeline" {
  test "full pipeline report" {
    assert.equal(ReviewInvoice(), 1);
  }
}
"#;
    db.file("test.baml", source);

    let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
    let unresolved = diagnostics
        .iter()
        .filter(|diag| {
            diag.id == baml_compiler_diagnostics::DiagnosticId::UnknownVariable
                && diag.message.contains("`ReviewInvoice`")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        unresolved.len(),
        1,
        "expected one unresolved ReviewInvoice diagnostic, got:\n{diagnostics:#?}"
    );
    let span = unresolved[0]
        .primary_span()
        .expect("unresolved name diagnostic should have a primary span");
    assert_ne!(
        u32::from(span.range.start()),
        u32::from(span.range.end()),
        "unresolved name diagnostic should not have an empty span:\n{diagnostics:#?}"
    );
    assert_ne!(
        u32::from(span.range.start()),
        0,
        "unresolved name diagnostic should not point at 0..0:\n{diagnostics:#?}"
    );
    let start = usize::from(span.range.start());
    let end = usize::from(span.range.end());
    assert_eq!(
        &source[start..end],
        "ReviewInvoice",
        "unresolved name diagnostic should point at the missing callee name:\n{diagnostics:#?}"
    );
}

// ── Optional function parameters ─────────────────────────────────────────

#[test]
fn optional_params_accept_omission_and_named_override() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function search(query: string, max: int = 10) -> string { query }
function f() -> string {
    let a = search("cats")
    return search("dogs", max = 5)
}
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_params_accept_omission_and_named_override", tir);
    assert!(
        tir.contains("function user.search(query: string, max: int = 10) -> string"),
        "{tir}"
    );
    assert!(tir.contains("let a = search(\"cats\") : string"), "{tir}");
    assert!(
        tir.contains("return search(\"dogs\", max = 5) : string"),
        "{tir}"
    );
    assert!(!tir.contains("!!"), "unexpected diagnostics:\n{tir}");
}

#[test]
fn llm_client_override_argument_is_callable_on_function() {
    // The compiler injects a `client: ai.Client? = null` override parameter on
    // every LLM function; a call site can pass any ai.Client value for it.
    // The generated `$build_request` companion shares the client override with
    // the parent LLM function.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r##"
client DefaultClient = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "default-key");
client OverrideClient = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "override-key");

function Ask(input: string) -> string {
  client: DefaultClient
  prompt: `${input}`
}

function call_overrides() -> string {
  let answer = Ask("hello", client = OverrideClient)
  answer
}
"##,
    );
    let tir = render_tir(&db, file);

    assert!(
        tir.contains("function user.Ask(input: string, client: ai.Client | null = null, on_event:"),
        "{tir}"
    );
    assert!(
        tir.contains(r#"Ask("hello", client = OverrideClient) : string"#),
        "{tir}"
    );
    assert!(!tir.contains("!!"), "unexpected diagnostics:\n{tir}");
}

#[test]
fn raw_generic_constructor_infers_typevar_from_field_value() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Box<T> {
  value T
  function unwrap(self) -> T { self.value }
}

function f() -> int {
  let b = Box { value: 42 }
  let get = b.unwrap
  get()
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("let b = Box { value: 42 } : user.Box"),
        "{tir}"
    );
    assert!(tir.contains("get() : int"), "{tir}");
    assert!(!tir.contains("!!"), "unexpected diagnostics:\n{tir}");
}

#[test]
fn misspelled_explicit_constructor_in_checked_context_errors() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class ValidationIssue {
  path string
  severity string
  message string
}

function f() -> ValidationIssue[] {
  [
    ValidationIssu {
      path: "due_date",
      severity: "warn",
      message: "missing due date",
    },
  ]
}
"#,
    );
    let tir = render_tir(&db, file);
    let unresolved_count = tir.matches("unresolved type: ValidationIssu").count();
    assert!(
        unresolved_count == 1,
        "expected misspelled explicit constructor to error, got:\n{tir}"
    );
}

#[test]
fn optional_param_call_binding_diagnostics() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function search(query: string, max: int = 10) -> string { query }
function positional_default() -> string { search("cats", 5) }
function positional_after_named() -> string { search(query = "cats", 5) }
function duplicate_named() -> string { search(query = "cats", max = 1, max = 2) }
function unknown_named() -> string { search(q = "cats") }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_param_call_binding_diagnostics", tir);
    assert!(tir.contains("defaulted parameter `max` must be passed by name"));
    assert!(tir.contains("positional arguments cannot appear after named arguments"));
    assert!(tir.contains("duplicate named argument `max`"));
    assert!(tir.contains("unknown named argument `q`"));
    assert!(tir.contains("missing required argument `query`"));
}

#[test]
fn optional_param_default_declaration_diagnostics() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function type_mismatch(a: int = "bad") -> int { a }
function forward_ref(a: int = b, b: int = 1) -> int { a }
function forward_ref_in_match(seed: int, a: int = match (seed) { 1 => b, _ => 0 }, b: int = 1) -> int { a }
function required_after_default(a: int = 1, b: int) -> int { b }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_param_default_declaration_diagnostics", tir);
    assert!(tir.contains("type mismatch: expected int, got \"bad\""));
    assert!(tir.contains("default for parameter `a` cannot reference later parameter `b`"));
    assert!(tir.contains("function user.forward_ref_in_match"));
    assert!(tir.contains("required parameter `b` cannot appear after a defaulted parameter"));
}

#[test]
fn optional_param_default_forward_reference_is_scope_aware() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function shadow_later_param(a: int = { let b = 1; b }, b: int = 2) -> int { a }
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("default for parameter `a` cannot reference later parameter `b`"),
        "{tir}"
    );
}

#[test]
fn optional_param_default_forward_reference_checks_lambda_bodies() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function lambda_capture_later_param(a: int = { let f = () -> int { b }; f() }, b: int = 1) -> int { a }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!(
        "optional_param_default_forward_reference_checks_lambda_bodies",
        tir.as_str()
    );
    assert!(
        tir.contains("default for parameter `a` cannot reference later parameter `b`"),
        "{tir}"
    );
}

#[test]
fn self_param_default_reports_single_semantic_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Counter {
  value int

  function Current(self = null) -> int {
    self.value
  }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert_eq!(tir.matches("`self` cannot have a default value").count(), 1);
    assert!(
        !tir.contains("type mismatch: expected user.Counter, got null"),
        "{tir}"
    );
}

// ── 3A-4. ArgumentCountMismatch diagnostic ───────────────────────────────

#[test]
fn too_many_args() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function add(a: int, b: int) -> int { return a + b; }\nfunction f() -> int { return add(1, 2, 3); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.add(a: int, b: int) -> int throws never {
      { : never
        return a + b : int
      }
    }
    function user.f() -> int throws never {
      { : never
        return add(1, 2, 3) : int
      }
      !! 83..95: expected 2 argument(s), got 3
    }
    ");
}

#[test]
fn too_few_args() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function add(a: int, b: int) -> int { return a + b; }\nfunction f() -> int { return add(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.add(a: int, b: int) -> int throws never {
      { : never
        return a + b : int
      }
    }
    function user.f() -> int throws never {
      { : never
        return add(1) : int
      }
      !! 83..89: expected 2 argument(s), got 1
    }
    ");
}

// ── 3A-5. NotCallable diagnostic ─────────────────────────────────────────

#[test]
fn calling_non_function() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f() -> int { let x = 42; return x(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        let x = 42 : 42 -> int
        return x(1) : !error
      }
      !! 41..42: `int` is not a function — it cannot be called
    }
    ");
}

#[test]
fn calling_class_as_function() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "class Foo { name string }\nfunction f() -> int { return Foo(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Foo {
      name: string
    }
    function user.f() -> int throws never {
      { : never
        return Foo(1) : !error
      }
      !! 55..58: unresolved name: Foo
    }
    class user.Foo$stream {
      name: string | null
    }
    ");
}

// ── 3A-6. MissingReturnExpression diagnostic ─────────────────────────────

#[test]
fn missing_return() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> int { let x = 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : void
        let x = 1 : 1 -> int
      }
      !! 20..34: type mismatch: expected int, got void
    }
    ");
}

#[test]
fn block_ending_in_stmt() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> string { let x = \"hello\"; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> string throws never {
      { : void
        let x = "hello" : "hello" -> string
      }
      !! 23..43: type mismatch: expected string, got void
    }
    "#);
}

// ── 3A-7. InvalidBinaryOp / InvalidUnaryOp diagnostics ──────────────────

#[test]
fn invalid_binary_op_string_minus_int() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> int { return \"hello\" - 5; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> int throws never {
      { : never
        return "hello" - 5 : !error
      }
      !! 29..40: operator `-` cannot be applied to `"hello"` and `5`
    }
    "#);
}

#[test]
fn invalid_binary_op_bool_add() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> int { return true + false; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        return true + false : !error
      }
      !! 29..41: operator `+` cannot be applied to `true` and `false`
    }
    ");
}

#[test]
fn invalid_binary_op_float_plus_bigint() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> bigint { return 1.5 + 100n; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> bigint throws never {
      { : never
        return 1.5 + 100n : !error
      }
      !! 32..42: operator `+` cannot be applied to `1.5` and `100n`
    }
    ");
}

#[test]
fn invalid_binary_op_bigint_plus_float() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> bigint { return 100n + 1.5; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> bigint throws never {
      { : never
        return 100n + 1.5 : !error
      }
      !! 32..42: operator `+` cannot be applied to `100n` and `1.5`
    }
    ");
}

#[test]
fn compound_assign_float_plus_string_is_rejected_in_tir() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> float {
  let g = 3.5;
  g += "add";
  g
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("operator `+` cannot be applied") && tir.contains("and `\"add\"`"),
        "expected compound assignment to fail during TIR inference:\n{tir}"
    );
}

#[test]
fn invalid_binary_op_float_lt_bigint() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> bool { return 1.5 < 100n; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> bool throws never {
      { : never
        return 1.5 < 100n : bool
      }
      !! 30..40: cannot order `1.5` and `100n` with `<`: ordering requires both operands to have the same type
    }
    ");
}

#[test]
fn bigint_eq_float_permitted() {
    // `==` is valid for any operand pair, so `bigint == float` type-checks to `bool` with
    // no diagnostic. The always-false lint (a warning on provably-disjoint operands) lands
    // with `==` lowering in Phase 3B, where it matches the concrete-equality runtime.
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> bool { return 100n == 1.5; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> bool throws never {
      { : never
        return 100n == 1.5 : bool
      }
    }
    ");
}

#[test]
fn ordering_unrelated_classes_is_error() {
    // `<` `>` `<=` `>=` are exact-type; two different classes can't be ordered.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "class Dog { name string }\nclass Cat { name string }\n\
         function f(a: Dog, b: Cat) -> bool { return a < b; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("cannot order `Dog` and `Cat`"),
        "expected OrderingDifferentTypes, got:\n{tir}"
    );
}

#[test]
fn ordering_subtype_related_is_error() {
    // Exact-type: even though `int <: int?`, ordering requires the *same* type — only
    // `==` may span a subtype relationship.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(a: int, b: int?) -> bool { return a < b; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("ordering requires both operands to have the same type"),
        "expected OrderingDifferentTypes, got:\n{tir}"
    );
}

#[test]
fn ordering_non_compare_class_is_error() {
    // A common type is found (both `Widget`), but `Widget` does not implement `Compare`,
    // so it has no ordering.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "class Widget { id int }\n\
         function f(a: Widget, b: Widget) -> bool { return a < b; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("`Widget` does not implement `Compare`"),
        "expected OrderingRequiresCompare, got:\n{tir}"
    );
}

#[test]
fn equality_disjoint_types_warns_always_false() {
    // `==` is valid for any pair, but provably-disjoint operands (here `int` vs
    // `string` — distinct concrete types) make it always false, so it warns
    // (`ComparisonAlwaysDisjoint`) rather than erroring.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(a: int, b: string) -> bool { return a == b; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("share no value, so this comparison is always false"),
        "expected ComparisonAlwaysDisjoint warning, got:\n{tir}"
    );
}

#[test]
#[ignore = "`Array.filled` mutable-literal aliasing warning is not yet implemented. Un-ignore when the warning lands."]
fn array_filled_with_mutable_literal_warns_aliasing() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let rows = baml.Array.filled(3, [0])
  return rows.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("reuses the same mutable value in every slot"),
        "expected Array.filled aliasing warning, got:\n{tir}"
    );
    assert!(
        tir.contains("??"),
        "expected warning marker for mutable literal aliasing, got:\n{tir}"
    );
}

#[test]
fn array_filled_with_primitive_value_has_no_aliasing_warning() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let xs = baml.Array.filled(3, 0)
  return xs.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("reuses the same mutable value"),
        "did not expect mutable-value aliasing warning, got:\n{tir}"
    );
}

#[test]
#[ignore = "`Array.filled` mutable-literal aliasing warning is not yet implemented. Un-ignore when the warning lands."]
fn array_filled_with_map_literal_warns_aliasing() {
    // A map literal (`Expr::Map`) is a reference type: every slot would alias
    // the same map, so it warns like the array-literal case.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let rows = baml.Array.filled(3, {})
  return rows.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("reuses the same mutable value in every slot"),
        "expected Array.filled map-literal aliasing warning, got:\n{tir}"
    );
    assert!(
        tir.contains("??"),
        "expected warning marker for map-literal aliasing, got:\n{tir}"
    );
}

#[test]
#[ignore = "`Array.filled` mutable-literal aliasing warning is not yet implemented. Un-ignore when the warning lands."]
fn array_filled_with_class_instance_literal_warns_aliasing() {
    // A class-instance literal (`Expr::Object`) is a reference type too, so the
    // same object is shared across every slot: warn.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class Cell { n int }
function f() -> int {
  let rows = baml.Array.filled(3, Cell { n: 0 })
  return rows.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("reuses the same mutable value in every slot"),
        "expected Array.filled class-instance aliasing warning, got:\n{tir}"
    );
    assert!(
        tir.contains("??"),
        "expected warning marker for class-instance aliasing, got:\n{tir}"
    );
}

#[test]
#[ignore = "`Array.filled` mutable-literal aliasing warning is not yet implemented. Un-ignore when the warning lands."]
fn array_filled_named_value_arg_warns_aliasing() {
    // The fill value can be passed by name (`value = ...`) rather than
    // positionally; the mutable-literal detection must handle that path too.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let rows = baml.Array.filled(3, value = [0])
  return rows.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("reuses the same mutable value in every slot"),
        "expected Array.filled named-`value` aliasing warning, got:\n{tir}"
    );
    assert!(
        tir.contains("??"),
        "expected warning marker for named-`value` aliasing, got:\n{tir}"
    );
}

#[test]
fn array_filled_with_variable_bound_mutable_value_does_not_warn() {
    // KNOWN LIMITATION (Linear B-548): detection is purely *syntactic* — it only
    // fires when the fill value is written inline as a literal. Binding the same
    // mutable value to a variable first (`let x = [0]; Array.filled(3, x)`) still
    // aliases every slot at runtime, but produces NO warning because the arg is a
    // `Path`, not a literal. This characterizes (does not endorse) that gap; the
    // real fix (Linear B-638) is the `Array.generate(length, f)` factory, which
    // calls `f` once per index and so builds an independent value per slot.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let x = [0]
  let rows = baml.Array.filled(3, x)
  return rows.length()
}"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("reuses the same mutable value"),
        "variable-bound mutable value is a known false-negative (must not warn), got:\n{tir}"
    );
}

#[test]
fn aliased_float_plus_bigint_is_rejected() {
    // Aliases on either side must still trip the float×bigint reject —
    // `infer_binary_op` peels them at entry before classifying.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "type FF = float\nfunction f(x: FF) -> bigint { return x + 100n; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("operator `+` cannot be applied"),
        "expected InvalidBinaryOp diagnostic, got:\n{tir}"
    );
}

#[test]
fn aliased_int_arithmetic_resolves_to_int() {
    // Plain aliased arithmetic must not get rejected just because the alias
    // wraps the primitive — `infer_arithmetic` should classify aliased
    // operands the same as bare ones after entry-level peeling.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "type II = int\nfunction f(x: II, y: int) -> int { return x + y; }",
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("!!"),
        "aliased int arithmetic should compile cleanly, got:\n{tir}"
    );
    assert!(
        tir.contains("return x + y : int"),
        "expected `int` result type, got:\n{tir}"
    );
}

#[test]
fn invalid_unary_op_neg_string() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> int { return -\"hello\"; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> int throws never {
      { : never
        return Neg "hello" : !error
      }
      !! 30..37: operator `-` cannot be applied to `"hello"`
    }
    "#);
}

// ── 3A-8. NotIndexable diagnostic ────────────────────────────────────────

#[test]
fn indexing_bool() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f(x: bool) -> int { return x[0]; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int throws never {
      { : never
        return x[0] : !error
      }
      !! 36..40: type `bool` is not indexable
    }
    ");
}

#[test]
fn indexing_int() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f(x: int) -> int { return x[0]; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int) -> int throws never {
      { : never
        return x[0] : !error
      }
      !! 35..39: type `int` is not indexable
    }
    ");
}

// ── 3A-9. FloatLiteral in TypeExpr ───────────────────────────────────────

#[test]
fn float_literal_in_annotation() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(x: 3.14 | 2.72) -> float { return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: 3.14 | 2.72) -> float throws never {
      { : never
        return x : 2.72 | 3.14
      }
    }
    ");
}

// ── 3A-10. if-without-else should produce Optional(T) ────────────────────

#[test]
fn if_without_else_optional() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(x: bool) -> int? { return if (x) { 5 }; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int | null throws never {
      { : never
        return : void
          if (x : bool) : void
            { : 5
              5 : 5
            }
      }
      !! 37..49: type mismatch: expected int | null, got void
    }
    block user.f {
    }
    ");
}

#[test]
fn if_without_else_let_binding() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(x: bool) -> int { let y = if (x) { 5 }; return y ?? 0; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int throws never {
      { : never
        let y = : void
          if (x : bool) : void
            { : 5
              5 : 5
            }
        return y ?? 0 : void | 0
      }
      !! 37..49: cannot use return value of a void function
      !! 58..64: type mismatch: expected int, got void | 0
    }
    block user.f {
    }
    ");
}

// ── 3A-11. Match expression: pattern binding + scrutinee narrowing ───────

#[test]
fn match_enum_variants() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"enum Color { Red
Green
Blue }
function f(x: Color) -> string {
  return match (x) {
    Color.Red => "red"
    Color.Green => "green"
    Color.Blue => "blue"
  };
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    enum user.Color
    function user.f(x: user.Color) -> string throws never {
      { : never
        return : "blue" | "green" | "red"
          match (x : user.Color) : "blue" | "green" | "red"
            Color.Red =>
              "red" : "red"
            Color.Green =>
              "green" : "green"
            Color.Blue =>
              "blue" : "blue"
      }
    }
    "#);
}

#[test]
fn match_catch_all() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int) -> int {
  return match (x) {
    let y => y + 1
  };
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int) -> int throws never {
      { : never
        return : int
          match (x : int) : int
            y =>
              y + 1 : int
      }
    }
    ");
}

// ── 3A-12. Union member field access ─────────────────────────────────────

#[test]
fn union_field_access_shared() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class Cat { name string
legs int }
class Dog { name string
legs int }
function f(x: Cat | Dog) -> string { return x.name; }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Cat {
      name: string
      legs: int
    }
    class user.Dog {
      name: string
      legs: int
    }
    function user.f(x: user.Cat | user.Dog) -> string throws never {
      { : never
        return x.name : string
      }
    }
    class user.Cat$stream {
      name: string | null
      legs: int | null
    }
    class user.Dog$stream {
      name: string | null
      legs: int | null
    }
    ");
}

#[test]
fn union_field_access_missing_on_some() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class Cat { name string
whiskers int }
class Dog { name string
tail bool }
function f(x: Cat | Dog) -> int { return x.whiskers; }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Cat {
      name: string
      whiskers: int
    }
    class user.Dog {
      name: string
      tail: bool
    }
    function user.f(x: user.Cat | user.Dog) -> int throws never {
      { : never
        return x.whiskers : !error
      }
      !! 116..126: type `Cat | Dog` has no member `whiskers`: its members implement no common interface that declares `whiskers`
    }
    class user.Cat$stream {
      name: string | null
      whiskers: int | null
    }
    class user.Dog$stream {
      name: string | null
      tail: bool | null
    }
    ");
}

#[test]
fn union_field_access_missing_on_one_of_three() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class A { name string }
class B { name string }
class C { age int }
function f(x: A | B | C) -> string { return x.name; }"#,
    );
    // C has no `name` field → error on the whole union
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.A {
      name: string
    }
    class user.B {
      name: string
    }
    class user.C {
      age: int
    }
    function user.f(x: user.A | user.B | user.C) -> string throws never {
      { : never
        return x.name : !error
      }
      !! 112..118: type `A | B | C` has no member `name`: its members implement no common interface that declares `name`
    }
    class user.A$stream {
      name: string | null
    }
    class user.B$stream {
      name: string | null
    }
    class user.C$stream {
      age: int | null
    }
    ");
}

#[test]
fn union_field_access_missing_on_two_of_three() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class A { name string }
class B { age string }
class C { age int }
function f(x: A | B | C) -> string { return x.name; }"#,
    );
    // C has no `name` field → error on the whole union
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.A {
      name: string
    }
    class user.B {
      age: string
    }
    class user.C {
      age: int
    }
    function user.f(x: user.A | user.B | user.C) -> string throws never {
      { : never
        return x.name : !error
      }
      !! 111..117: type `A | B | C` has no member `name`: its members implement no common interface that declares `name`
    }
    class user.A$stream {
      name: string | null
    }
    class user.B$stream {
      age: string | null
    }
    class user.C$stream {
      age: int | null
    }
    ");
}

#[test]
fn union_field_access_different_types() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class A { value int }
class B { value string }
function f(x: A | B) -> string { return x.value; }"#,
    );
    // Both have `value` but different types → union of field types
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.A {
      value: int
    }
    class user.B {
      value: string
    }
    function user.f(x: user.A | user.B) -> string throws never {
      { : never
        return x.value : int | string
      }
      !! 87..94: type mismatch: expected string, got int | string
    }
    class user.A$stream {
      value: int | null
    }
    class user.B$stream {
      value: string | null
    }
    ");
}

#[test]
fn union_field_access_optional_member() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"class A { name string }
class B { name string }
function f(x: A | B | null) -> string { return x.name; }"#,
    );
    // null in union → can't access field (needs narrowing first)
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.A {
      name: string
    }
    class user.B {
      name: string
    }
    function user.f(x: user.A | user.B | null) -> string throws never {
      { : never
        return x.name : !error
      }
      !! 95..101: type `A | B | null` has no member `name`: its members implement no common interface that declares `name`
    }
    class user.A$stream {
      name: string | null
    }
    class user.B$stream {
      name: string | null
    }
    ");
}

// ── Null coalescing operator (??) ──────────────────────────────────────────

#[test]
fn null_coalesce_unwraps_optional() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f(x: int?) -> int { x ?? 0 }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : int
        x ?? 0 : int
      }
    }
    ");
}

#[test]
fn null_coalesce_with_variable_default() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f(x: int?, y: int) -> int { x ?? y }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null, y: int) -> int throws never {
      { : int
        x ?? y : int
      }
    }
    ");
}

#[test]
fn null_coalesce_with_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(name: string?) -> string { let x = "Anonymous"; name ?? x }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(name: string | null) -> string throws never {
      { : string
        let x = "Anonymous" : "Anonymous" -> string
        name ?? x : string
      }
    }
    "#);
}

// ── Optional chaining (?.) ─────────────────────────────────────────────────

#[test]
fn optional_field_access() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User { name string }
function f(u: User?) -> string? { u?.name }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_chaining_with_null_coalesce() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User { name string }
function f(u: User?, fallback: string) -> string { u?.name ?? fallback }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn chained_optional_field_access() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Address { street string }
class User { address Address? }
function f(u: User?) -> string? { u?.address?.street }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_method_call_basic() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User {
    function getName(self) -> string { self.name }
    name string
}
function f(u: User?) -> string? { u?.getName() }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_call_chain_continues() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User { name string }
function f(callback: (() -> User throws never)?) -> string? {
    callback?.()?.name
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_field_access_through_optional_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User { name string }
type MaybeUser = User?
function f(u: MaybeUser) -> string? { u?.name }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_index_through_optional_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
type MaybeInts = int[]?
function f(xs: MaybeInts) -> int? { xs?.[0] }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Void return type ───────────────────────────────────────────────────────

#[test]
fn void_function_basic() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> void { }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : void
      }
    }
    ");
}

#[test]
fn void_function_bare_return() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> void { return; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : never
        return
      }
    }
    ");
}

#[test]
fn void_function_return_value_error() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> void { return 42; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : never
        return 42 : 42
      }
      !! 30..32: type mismatch: expected void, got 42
    }
    ");
}

#[test]
fn void_function_result_used_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function g() -> void { }
function f() -> int { let x = g(); 1 }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.g() -> void throws never {
      { : void
      }
    }
    function user.f() -> int throws never {
      { : 1
        let x = g() : void
        1 : 1
      }
      !! 56..59: cannot use return value of a void function
    }
    ");
}

#[test]
fn void_function_bare_call_ok() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function g() -> void { }
function f() -> int { g(); 1 }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.g() -> void throws never {
      { : void
      }
    }
    function user.f() -> int throws never {
      { : 1
        g() : void
        1 : 1
      }
    }
    ");
}

#[test]
fn lambda_checks_against_aliased_and_optional_function_contexts() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
type Body = () -> void throws never

function takes_direct(cb: Body) -> void {
    cb()
}

function takes_optional(cb: Body?) -> void {
    cb?.()
}

function main() -> void {
    takes_direct(() -> { assert.is_true(true); })
    takes_optional(() -> { assert.is_true(true); })
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("type mismatch"),
        "expected lambda alias checking without mismatches, got:\n{tir}"
    );
    assert!(
        tir.contains("() -> { ... } : () -> void throws never"),
        "expected lambdas to inherit void-returning aliased function context, got:\n{tir}"
    );
}

#[test]
fn explicit_unknown_list_annotation_pins_element_type() {
    // Regression (BEP-049 M5): `let xs: unknown[] = []` must honour the
    // explicit `unknown[]` annotation instead of starting an evolving
    // `never[]` that pins to the first pushed value's type. The built-in
    // `prompt` tag's `values` accumulator depends on this to hold a
    // heterogeneous mix (a `Role`, then a string), so a regression here
    // surfaces as a bogus "expected Role, got string" on the second push.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function main() -> int {
  let xs: unknown[] = []
  let r = ai.Role { name: "x", metadata: {} }
  xs.push(r)
  xs.push("hello")
  return 0
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        !output.contains("type mismatch"),
        "heterogeneous pushes into `unknown[]` should type-check, got:\n{output}"
    );
    assert!(
        !output.contains("(evolving)"),
        "explicit `unknown[]` annotation should pin the element type, not evolve, got:\n{output}"
    );
}
