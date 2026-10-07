//! Compile-only declaration checks; these tests do not execute hooks.

use baml_compiler_diagnostics::Severity;
use baml_tests::stdlib_prefix::{check_user_files, setup_multi_file_db, setup_test_db};

fn errors(source: &str) -> Vec<String> {
    check_user_files(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect()
}

#[test]
fn constant_and_argument_aware_hooks_compile() {
    assert_eq!(
        errors(
            r#"
function policy(value: int, settings: trace.Settings) -> trace.Options throws never {
    if (value > 3) { trace.span(inputs = true) } else { trace.timing() }
}
function unchanged(value: unknown, settings: unknown) -> trace.Options? throws never { null }
/// baml:$trace=policy
function target(value: int = 7) -> int { value }
/// baml:$trace=unchanged
function broad(value: string) -> string { value }
/// baml:$trace=trace.hidden
function hidden(value: int) -> int { value }
/// baml:$trace=trace.timing
function timing() -> int { 1 }
/// baml:$trace=trace.empty_span
function rich(value: int) -> int { value }
function record() -> trace.Options throws never { trace.span(output = true) }
/// baml:$trace=record
function span() -> int { 1 }
function settings_only(settings: trace.Settings) -> trace.Options? { null }
/// baml:$trace=settings_only
function zero() -> int { 1 }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn generic_hook_is_checked_in_target_environment() {
    assert_eq!(
        errors(
            r#"
function policy<T>(value: T, settings: trace.Settings) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target<T>(value: T) -> T { value }
/// baml:$trace=policy
function concrete(value: int) -> int { value }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn hook_resolves_across_files_in_declaration_namespace() {
    let db = setup_multi_file_db(&[
        (
            "ns_app/policy.baml",
            "function policy(value: int, settings: trace.Settings) -> trace.Options throws never { trace.timing() }",
        ),
        (
            "ns_app/target.baml",
            "/// baml:$trace=policy\nfunction target(value: int) -> int { value }",
        ),
    ]);
    let messages: Vec<_> = check_user_files(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert_eq!(messages, Vec::<String>::new());
}

#[test]
fn argument_aware_hooks_allow_extra_defaulted_parameters() {
    assert_eq!(
        errors(
            r#"
function policy(settings: trace.Settings, extra: int = 0) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn all_defaulted_hooks_are_called_without_target_arguments() {
    assert_eq!(
        errors(
            r#"
function policy(label: string = "record", enabled: bool = true) -> trace.Options throws never {
    trace.span(inputs = enabled)
}
/// baml:$trace=policy
function custom(value: int) -> int { value }
/// baml:$trace=trace.span
function span(value: int, other: string) -> int { value }
/// baml:$trace=trace.span
function zero() -> int { 1 }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn defaulted_hooks_still_check_return_and_throws() {
    assert_eq!(
        errors(
            r#"
function wrong_return(value: int = 0) -> int throws never { value }
function throwing(value: int = 0) -> trace.Options throws string { throw "bad"; }
/// baml:$trace=wrong_return
function first() -> int { 1 }
/// baml:$trace=throwing
function second() -> int { 1 }
"#
        ),
        [
            r#"Invalid trace hook `wrong_return` for function `first`.
The hook returns `int`, but must return `trace.Options` or `null`.
help: Return tracing options (for example `trace.span(inputs = true)`) or `null` to keep the current settings."#,
            r#"Invalid trace hook `throwing` for function `second`.
The hook may throw `string`, but trace hooks require `throws never`.
help: Handle these errors inside the hook or its default expressions."#,
        ]
    );
}

#[test]
fn optional_settings_do_not_replace_required_hook_arguments() {
    assert_eq!(
        errors(
            r#"
function policy(value: int, settings: trace.Settings? = null) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target(value: int) -> int { value }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook has 1 required parameter(s), but this target needs 2.
Expected hook signature: (value: int, settings: trace.Settings) -> trace.Options? throws never
help: Accept the target's arguments in declaration order, followed by settings, or use a hook callable without arguments."#]
    );
}

#[test]
fn omitted_hook_defaults_must_not_throw() {
    assert_eq!(
        errors(
            r#"
function default_value() -> int throws string { throw "bad"; }
function policy(value: int = default_value()) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook's omitted default expressions may throw `string`, but trace hooks require `throws never`.
help: Handle these errors inside the default expressions."#]
    );
}

#[test]
fn hook_default_effects_use_the_call_site_generic_instantiation() {
    assert_eq!(
        errors(
            r#"
function policy<E>(callback: () -> int throws E, settings: trace.Settings, extra: int = callback()) -> trace.Options throws never {
    trace.timing()
}
/// baml:$trace=policy
function target(callback: () -> int throws never) -> int { 1 }
"#
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        errors(
            r#"
function policy<E>(callback: () -> int throws E, settings: trace.Settings, extra: int = callback()) -> trace.Options throws never {
    trace.timing()
}
/// baml:$trace=policy
function target(callback: () -> int throws string) -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook's omitted default expressions may throw `string`, but trace hooks require `throws never`.
help: Handle these errors inside the default expressions."#]
    );
}

#[test]
fn unsupported_attachments_are_rejected() {
    for source in [
        r#"/// baml:$trace=missing
class Data {}"#,
        r#"/// baml:$trace=missing
enum Choice { First }"#,
        r#"/// baml:$trace=missing
type Alias = int"#,
        r#"/// baml:$trace=missing
interface Data {}"#,
        r#"class Data {
    /// baml:$trace=missing
    value: int,
}"#,
        r#"interface Data {
    /// baml:$trace=missing
    function value(self) -> int throws never;
}"#,
        r#"function target() -> int {
    /// baml:$trace=missing
    1
}"#,
        r#"function target() -> int { 1 }
/// baml:$trace=missing"#,
    ] {
        assert_eq!(
            errors(source),
            [r#"Trace hooks can only be attached to function definitions.
help: Place `/// baml:$trace=...` immediately before a function definition."#],
            "{source}"
        );
    }
}

#[test]
fn directive_text_in_strings_is_not_an_attachment() {
    assert_eq!(
        errors(r#"function target() -> string { "/// baml:$trace=missing" }"#),
        Vec::<String>::new()
    );
}

#[test]
fn hook_arity_is_exact_or_zero() {
    assert_eq!(
        errors(
            r#"
function policy(value: int) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target(value: int) -> int { value }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook has 1 required parameter(s), but this target needs 2.
Expected hook signature: (value: int, settings: trace.Settings) -> trace.Options? throws never
help: Accept the target's arguments in declaration order, followed by settings, or use a hook callable without arguments."#]
    );
}

#[test]
fn unresolved_hook_is_an_error() {
    assert_eq!(
        errors(
            r#"
/// baml:$trace=missing
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `missing` for function `target`.
Cannot resolve `missing` in the target's declaration scope.
help: Define the hook or use its qualified function name in `/// baml:$trace=...`."#]
    );
}

#[test]
fn duplicate_hook_is_an_error() {
    assert_eq!(
        errors(
            r#"
/// baml:$trace=trace.hidden
/// baml:$trace=trace.timing
function target() -> int { 1 }
"#
        ),
        [r#"Function `target` declares more than one trace hook.
help: Keep one `/// baml:$trace=...` directive."#]
    );
}

#[test]
fn malformed_hook_is_an_error() {
    for directive in [
        "/// baml:$trace",
        "/// baml:$trace=",
        "/// baml:$trace=trace.hidden()",
    ] {
        assert_eq!(
            errors(&format!("{directive}\nfunction target() -> int {{ 1 }}")),
            [r#"Invalid trace hook directive on function `target`.
Expected `/// baml:$trace=hook_name` with a function reference.
help: Use a function name without call parentheses, for example `/// baml:$trace=trace.empty_span`."#]
        );
    }
}

#[test]
fn wrong_return_type_is_an_error() {
    assert_eq!(
        errors(
            r#"
function policy() -> int throws never { 1 }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook returns `int`, but must return `trace.Options` or `null`.
help: Return tracing options (for example `trace.span(inputs = true)`) or `null` to keep the current settings."#]
    );
}

#[test]
fn wrong_target_argument_type_is_an_error() {
    assert_eq!(
        errors(
            r#"
function policy(value: string, settings: trace.Settings) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target(value: int) -> int { value }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
Target parameter `value` has type `int`, but hook parameter `value` expects `string`.
help: Change hook parameter `value` to accept `int` or a compatible broader type."#]
    );
}

#[test]
fn parameter_errors_identify_the_attachment_and_both_declarations_across_files() {
    let db = setup_multi_file_db(&[
        (
            "ns_app/hooks.baml",
            r#"function capture_search(query: string, maximum: string, settings: trace.Settings) -> trace.Options throws never { trace.span(inputs = true) }"#,
        ),
        (
            "ns_app/search.baml",
            r#"/// baml:$trace=capture_search
function search(query: string, limit: int = 10) -> string { query }"#,
        ),
    ]);
    let diagnostics: Vec<_> = check_user_files(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.message,
        r#"Invalid trace hook `capture_search` for function `app.search`.
Target parameter `limit` has type `int`, but hook parameter `maximum` expects `string`.
help: Change hook parameter `maximum` to accept `int` or a compatible broader type."#
    );
    let target = db
        .get_file(std::path::Path::new("ns_app/search.baml"))
        .unwrap();
    let hook = db
        .get_file(std::path::Path::new("ns_app/hooks.baml"))
        .unwrap();
    let primary = diagnostic.primary_span().unwrap();
    assert_eq!(primary.file_id, target.file_id(&db));
    assert_eq!(
        &target.text(&db)[usize::from(primary.range.start())..usize::from(primary.range.end())],
        "/// baml:$trace=capture_search"
    );
    let related: Vec<_> = diagnostic
        .related_info
        .iter()
        .map(|note| {
            let file = if note.span.file_id == hook.file_id(&db) {
                hook
            } else {
                assert_eq!(note.span.file_id, target.file_id(&db));
                target
            };
            let text = file.text(&db);
            (
                note.message.as_str(),
                text[usize::from(note.span.range.start())..usize::from(note.span.range.end())]
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(
        related,
        [
            (
                "hook parameter `maximum` expects `string`",
                "maximum: string".into()
            ),
            (
                "target parameter `limit` has type `int`",
                "limit: int = 10".into()
            ),
        ]
    );
}

#[test]
fn wrong_settings_type_is_an_error() {
    assert_eq!(
        errors(
            r#"
function policy(settings: int) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The runtime supplies `trace.Settings` as the final argument, but hook parameter `settings` expects `int`.
help: Make the final required hook parameter accept `trace.Settings`."#]
    );
}

#[test]
fn method_hook_errors_point_to_the_hook_method_declaration() {
    let source = r#"class Policies {
    function capture() -> int throws never { 1 }
}
class Handler {
    /// baml:$trace=Policies.capture
    function search(self, value: int) -> int { value }
}"#;
    let db = setup_test_db(source);
    let diagnostics: Vec<_> = check_user_files(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.message,
        r#"Invalid trace hook `Policies.capture` for function `Handler.search`.
The hook returns `int`, but must return `trace.Options` or `null`.
help: Return tracing options (for example `trace.span(inputs = true)`) or `null` to keep the current settings."#
    );
    assert_eq!(diagnostic.related_info.len(), 1);
    let related = &diagnostic.related_info[0];
    assert_eq!(related.message, "hook declared here");
    assert_eq!(
        &source[usize::from(related.span.range.start())..usize::from(related.span.range.end())],
        "capture"
    );
}

#[test]
fn throwing_hook_is_an_error() {
    assert_eq!(
        errors(
            r#"
function policy() -> trace.Options throws string { throw "bad" }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook may throw `string`, but trace hooks require `throws never`.
help: Handle these errors inside the hook or its default expressions."#]
    );
}

#[test]
fn target_parameters_do_not_shadow_the_hook_reference() {
    assert_eq!(
        errors(
            r#"
function policy(value: int, settings: trace.Settings) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function target(policy: int) -> int { policy }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn methods_and_qualified_parameterless_hooks_compile() {
    assert_eq!(
        errors(
            r#"
function policy(receiver: unknown, value: int, settings: trace.Settings) -> trace.Options throws never { trace.timing() }
class Handler {
    /// baml:$trace=root.policy
    function process(self, value: int) -> int { value }
    function quiet() -> trace.Options throws never { trace.hidden() }
}
/// baml:$trace=Handler.quiet
function target(value: int) -> int { value }
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn llm_hooks_receive_authored_arguments() {
    assert_eq!(
        errors(
            r#"
client Local = openai.ResponsesClient.new(model = "gpt-5.4-nano", api_key = "not-used");
function policy(text: string, settings: trace.Settings) -> trace.Options throws never { trace.timing() }
/// baml:$trace=policy
function Demo(text: string) -> string {
    client: Local
    prompt: `${role("user")}Say hello: ${text}`
}
"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn void_hook_is_not_a_value_returning_hook() {
    assert_eq!(
        errors(
            r#"
function policy() -> void throws never { }
/// baml:$trace=policy
function target() -> int { 1 }
"#
        ),
        [r#"Invalid trace hook `policy` for function `target`.
The hook returns `void`, but must return `trace.Options` or `null`.
help: Return tracing options (for example `trace.span(inputs = true)`) or `null` to keep the current settings."#]
    );
}
