//! E0150–E0152 — semantic validation of the LLM capability markers
//! (`_plan/llm-desugar-capabilities-plan.md` §1.2):
//!
//! - E0150 `//baml:llm_capability` interfaces must (transitively)
//!   `requires baml.ai.Provider`;
//! - E0151 `//baml:llm_companion(<suffix>)` drivers must match the driver
//!   convention (top-level, `<T>`/`<TPartial, T>`, `client: baml.ai.Provider`,
//!   `prompt: baml.llm.PromptAst`);
//! - E0152 companion suffixes are unique session-wide (first wins).

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_project::{ProjectDatabase, collect_diagnostics};

fn compile_errors(source: &str) -> Vec<String> {
    compile_errors_multi(&[("main.baml", source)])
}

fn compile_errors_multi(files: &[(&str, &str)]) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    for (path, source) in files {
        db.add_file(*path, source);
    }
    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(&db)).collect();
    collect_diagnostics(&db, project, &all_files)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

#[track_caller]
fn assert_has_error(errors: &[String], needle: &str) {
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_no_error_with(errors: &[String], needle: &str) {
    assert!(
        !errors.iter().any(|e| e.contains(needle)),
        "expected no error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

/// A conforming driver, reused across tests. Body leads with `let` because a
/// `prompt`-named param otherwise trips the LLM-body misparse (gotchas).
const VALID_DRIVER: &str = r#"
//baml:llm_companion(echoed)
function drive_echoed<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#;

// ── E0150 ────────────────────────────────────────────────────────────────────

#[test]
fn e0150_capability_without_requires_provider() {
    let errors = compile_errors(
        r#"
//baml:llm_capability
interface Moderated {
  function call_moderated(self, policy: string) -> string
}
"#,
    );
    assert_has_error(&errors, "[E0150]");
}

#[test]
fn e0150_ok_with_direct_provider_requires() {
    let errors = compile_errors(
        r#"
//baml:llm_capability
interface Moderated requires baml.ai.Provider {
  function call_moderated(self, policy: string) -> string
}
"#,
    );
    assert_no_error_with(&errors, "[E0150]");
}

#[test]
fn e0150_ok_via_transitive_stdlib_capability() {
    // `baml.ai.HttpProvider` itself `requires Provider` *unqualified inside
    // the stdlib* — reaching Provider requires resolving that clause in the
    // stdlib's own context (the E0125-shaped trap).
    let errors = compile_errors(
        r#"
//baml:llm_capability
interface Fancy requires baml.ai.HttpProvider {
  function fancy(self) -> string
}
"#,
    );
    assert_no_error_with(&errors, "[E0150]");
}

#[test]
fn unmarked_interface_never_hits_e0150() {
    let errors = compile_errors(
        r#"
interface Plain {
  function f(self) -> string
}
"#,
    );
    assert_no_error_with(&errors, "[E0150]");
}

// ── E0151 ────────────────────────────────────────────────────────────────────

#[test]
fn valid_driver_has_no_e0151() {
    let errors = compile_errors(VALID_DRIVER);
    assert_no_error_with(&errors, "[E0151]");
}

#[test]
fn e0151_two_type_param_driver_is_legal() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(streamy)
function drive_streamy<TPartial, T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#,
    );
    assert_no_error_with(&errors, "[E0151]");
}

#[test]
fn e0151_passthrough_generics_are_legal() {
    // `drive_with`-shaped drivers thread extra generics (V/E2) past the `T`
    // slot — legal under the name-based convention.
    let errors = compile_errors(
        r#"
//baml:llm_companion(projected)
function drive_projected<T, V, E2>(
  client: baml.ai.Provider,
  prompt: baml.llm.PromptAst,
  extra: V,
) -> V {
  let p = client;
  extra
}
"#,
    );
    assert_no_error_with(&errors, "[E0151]");
}

#[test]
fn e0151_zero_generics() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(nogen)
function drive_nogen(client: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
    assert_has_error(&errors, "generic parameter");
}

#[test]
fn e0151_wrong_first_param_name() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(misnamed)
function drive_misnamed<T>(c: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = c;
  "ok"
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
    assert_has_error(&errors, "must be named `client`");
}

#[test]
fn e0151_client_not_provider_typed() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(strclient)
function drive_strclient<T>(client: string, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
    assert_has_error(&errors, "baml.ai.Provider");
}

#[test]
fn e0151_prompt_not_prompt_ast() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(strprompt)
function drive_strprompt<T>(client: baml.ai.Provider, prompt: string) -> string {
  let p = client;
  "ok"
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
    assert_has_error(&errors, "PromptAst");
}

#[test]
fn e0151_too_few_params() {
    let errors = compile_errors(
        r#"
//baml:llm_companion(oneparam)
function drive_oneparam<T>(client: baml.ai.Provider) -> string {
  let p = client;
  "ok"
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
}

#[test]
fn e0151_marker_on_method() {
    let errors = compile_errors(
        r#"
class Holder {
  x: string

  //baml:llm_companion(methodical)
  function drive_methodical<T>(self) -> string {
    self.x
  }
}
"#,
    );
    assert_has_error(&errors, "[E0151]");
    assert_has_error(&errors, "top-level function");
}

// ── E0152 ────────────────────────────────────────────────────────────────────

#[test]
fn e0152_duplicate_suffix_across_files() {
    let errors = compile_errors_multi(&[
        ("a.baml", VALID_DRIVER),
        (
            "b.baml",
            r#"
//baml:llm_companion(echoed)
function drive_echoed_again<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#,
        ),
    ]);
    assert_has_error(&errors, "[E0152]");
    assert_has_error(&errors, "drive_echoed");
}

#[test]
fn unique_suffixes_have_no_e0152() {
    let errors = compile_errors_multi(&[
        ("a.baml", VALID_DRIVER),
        (
            "b.baml",
            r#"
//baml:llm_companion(other)
function drive_other<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst) -> string {
  let p = client;
  "ok"
}
"#,
        ),
    ]);
    assert_no_error_with(&errors, "[E0152]");
}
