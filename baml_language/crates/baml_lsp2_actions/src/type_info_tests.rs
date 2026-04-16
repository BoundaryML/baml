#[cfg(test)]
mod tests {
    use crate::{testing::CursorTest, type_info::TypeInfo};

    // ── Class definition hover ──────────────────────────────────────────────

    #[test]
    fn hover_class_no_generics() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Person {
    name: string
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        assert_eq!(
            info,
            TypeInfo::Class {
                name: "Person".into(),
                fields: vec![("name".into(), "string".into())],
            }
        );
    }

    #[test]
    fn hover_generic_class_definition_shows_type_params() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Handler<E> {
    run: () -> null throws E
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Class { name, .. } => {
                assert_eq!(
                    name, "Handler<E>",
                    "Class hover should include generic params"
                );
            }
            other => panic!("Expected TypeInfo::Class, got: {other:?}"),
        }
    }

    #[test]
    fn hover_generic_class_multiple_type_params() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Pair<A, B> {
    first: A
    second: B
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Class { name, .. } => {
                assert_eq!(
                    name, "Pair<A, B>",
                    "Class hover should show all generic params"
                );
            }
            other => panic!("Expected TypeInfo::Class, got: {other:?}"),
        }
    }

    // ── Class usage-site hover (resolves to definition) ─────────────────────

    #[test]
    fn hover_generic_class_at_usage_site() {
        // Hovering over `Handler` in a param type resolves to the class definition.
        // The hover should show `Handler<E>` (the definition's generic params).
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function use_handler(h: <[CURSOR]Handler<string>) -> null {
    h.run()
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Class { name, .. } => {
                assert_eq!(
                    name, "Handler<E>",
                    "Usage-site hover resolves to definition, should show generic params"
                );
            }
            other => panic!("Expected TypeInfo::Class, got: {other:?}"),
        }
    }

    // ── Function definition hover ───────────────────────────────────────────

    #[test]
    fn hover_function_no_generics() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]greet(name: string) -> string {
    name
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { name, .. } => {
                assert_eq!(name, "greet");
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
    }

    #[test]
    fn hover_generic_function_shows_type_params() {
        let test = CursorTest::new(
            r#"
class Wrapper<T> {
    inner: T
}

function <[CURSOR]unwrap<T>(w: Wrapper<T>) -> T {
    w.inner
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { name, .. } => {
                assert_eq!(
                    name, "unwrap<T>",
                    "Function hover should include generic params"
                );
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
    }

    // ── Hover markdown rendering ────────────────────────────────────────────

    #[test]
    fn hover_markdown_generic_class() {
        let test = CursorTest::new(
            r#"
class <[CURSOR]Handler<E> {
    run: () -> null throws E
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        let md = info.to_hover_markdown();
        assert!(
            md.contains("class Handler<E>"),
            "Markdown should contain 'class Handler<E>', got: {md}"
        );
    }

    #[test]
    fn hover_markdown_generic_function() {
        let test = CursorTest::new(
            r#"
class Wrapper<T> {
    inner: T
}

function <[CURSOR]unwrap<T>(w: Wrapper<T>) -> T {
    w.inner
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        let md = info.to_hover_markdown();
        assert!(
            md.contains("function unwrap<T>"),
            "Markdown should contain 'function unwrap<T>', got: {md}"
        );
        // The param type should render with generic args thanks to display_type_expr
        assert!(
            md.contains("Wrapper<T>"),
            "Param type should render as Wrapper<T>, got: {md}"
        );
    }

    #[test]
    fn hover_markdown_function_with_generic_param_type() {
        // Hover on `maybe_handler` — the function itself is not generic,
        // but its parameter type `Handler<string>?` has generic args.
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function <[CURSOR]maybe_handler(h: Handler<string>?) -> null {
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        let md = info.to_hover_markdown();
        assert!(
            md.contains("Handler<string>?"),
            "Param type should render as Handler<string>?, got: {md}"
        );
    }

    #[test]
    fn hover_markdown_function_with_nested_generics() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

class Wrapper<T> {
    inner: T
}

function <[CURSOR]use_nested(w: Wrapper<Handler<string>>) -> null {
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        let md = info.to_hover_markdown();
        assert!(
            md.contains("Wrapper<Handler<string>>"),
            "Param type should show nested generics, got: {md}"
        );
    }

    #[test]
    fn hover_markdown_function_with_generic_array_param() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function <[CURSOR]many_handlers(hs: Handler<int>[]) -> null {
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        let md = info.to_hover_markdown();
        assert!(
            md.contains("Handler<int>[]"),
            "Param type should render as Handler<int>[], got: {md}"
        );
    }

    // ── Throws display in hover ─────────────────────────────────────────────

    #[test]
    fn hover_function_no_throws_omits_throws_clause() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]safe_fn(x: int) -> string {
    "hello"
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(
                    throws, &None,
                    "Non-throwing function should have throws: None"
                );
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            !md.contains("throws"),
            "Non-throwing function hover should not contain 'throws', got: {md}"
        );
    }

    #[test]
    fn hover_function_with_declared_throws() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]risky_fn(x: int) -> string throws string {
    "hello"
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert!(
                    throws.is_some(),
                    "Function with declared throws should have throws: Some(...)"
                );
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Hover should contain 'throws string', got: {md}"
        );
    }

    #[test]
    fn hover_function_with_inferred_throws_from_member_call() {
        // use_handler calls h.run() which throws string — the inferred throws
        // should propagate and appear in the hover.
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function <[CURSOR]use_handler(h: Handler<string>) -> null {
    h.run()
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert!(
                    throws.is_some(),
                    "Function with inferred throws should have throws: Some(...)"
                );
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Hover should show inferred 'throws string', got: {md}"
        );
    }

    #[test]
    fn hover_function_with_declared_union_throws() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]multi_throw(x: int) -> string throws string | int {
    "hello"
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string | int"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string | int"),
            "Hover should preserve the declared throws clause, got: {md}"
        );
    }

    #[test]
    fn hover_method_with_inferred_throws_from_member_call() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

class Runner {
    function <[CURSOR]use_handler(self, h: Handler<string>) -> null {
        h.run()
    }
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Method hover should show inferred throws, got: {md}"
        );
    }

    #[test]
    fn hover_method_with_declared_union_throws_preserves_source_clause() {
        let test = CursorTest::new(
            r#"
class Runner {
    function <[CURSOR]declared(self) -> null throws string | int {
        null
    }
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string | int"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string | int"),
            "Method hover should preserve the declared throws clause, got: {md}"
        );
    }

    #[test]
    fn hover_function_with_inferred_throws_from_returned_receiver_call() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function make_handler() -> Handler<string> {
    Handler { run: () -> null { throw "boom" } }
}

function <[CURSOR]use_returned_receiver() -> null {
    make_handler().run()
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Hover should show inferred throws from returned receiver call, got: {md}"
        );
    }

    #[test]
    fn hover_method_with_inferred_throws_from_self_field_call() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

class Runner {
    primary: Handler<string>

    function <[CURSOR]use_self(self) -> null {
        self.primary.run()
    }
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Method hover should show inferred throws from self field call, got: {md}"
        );
    }

    #[test]
    fn hover_function_with_qualified_declared_throws_preserves_source_clause() {
        let test = CursorTest::new(
            r#"
namespace root.errors {
    class Io {
        message: string
    }
}

function <[CURSOR]qualified() -> null throws root.errors.Io | string {
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("root.errors.Io | string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws root.errors.Io | string"),
            "Hover should preserve the qualified declared throws clause, got: {md}"
        );
    }

    #[test]
    fn hover_function_with_inferred_throws_from_inline_heterogeneous_collection() {
        let test = CursorTest::new(
            r#"
class Task<E> {
    name: string
    run: () -> null throws E
}

function <[CURSOR]run_inline_tasks() -> null {
    let tasks = [
        Task { name: "a", run: () -> null { throw "error" } },
        Task { name: "b", run: () -> null { throw 42 } },
    ]
    for (let task in tasks) {
        task.run()
    }
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("int | string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws int | string"),
            "Hover should show inferred throws from inline stored callbacks, got: {md}"
        );
    }

    #[test]
    fn hover_local_var_preserves_generic_payload_for_inline_task_array() {
        let test = CursorTest::new(
            r#"
class Task<E> {
    name: string
    run: () -> null throws E
}

function run_inline_tasks() -> null {
    let <[CURSOR]tasks = [
        Task { name: "a", run: () -> null { throw "error" } },
        Task { name: "b", run: () -> null { throw 42 } },
    ]
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::LocalVar { name, ty } => {
                assert_eq!(name, "tasks");
                assert_eq!(ty, "user.Task<string | int>[]");
            }
            other => panic!("Expected TypeInfo::LocalVar, got: {other:?}"),
        }
    }

    #[test]
    fn hover_function_with_caught_optional_call_omits_throws_clause() {
        let test = CursorTest::new(
            r#"
function <[CURSOR]optional_call_caught(cb: (() -> int throws string)?) -> int? {
    cb?.() catch (e) {
        _ => null
    }
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws, &None);
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            !md.contains(") -> int? throws"),
            "Caught optional callback hover should omit throws, got: {md}"
        );
    }

    #[test]
    fn hover_function_with_optional_method_call_shows_inferred_throws() {
        let test = CursorTest::new(
            r#"
class Handler<E> {
    run: () -> null throws E
}

function <[CURSOR]maybe_use_string_handler(h: Handler<string>?) -> null {
    h?.run()
    null
}
"#,
        );
        let info = test.type_info().expect("should resolve");
        match &info {
            TypeInfo::Function { throws, .. } => {
                assert_eq!(throws.as_deref(), Some("string"));
            }
            other => panic!("Expected TypeInfo::Function, got: {other:?}"),
        }
        let md = info.to_hover_markdown();
        assert!(
            md.contains("throws string"),
            "Optional method call hover should show inferred throws, got: {md}"
        );
    }
}
