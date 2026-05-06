//! Auto-derived `to_json` / `from_json` synthesis for user-defined classes.
//!
//! Pure AST transform — no type information needed. For every user
//! `class C { f1: T1, f2: T2, ... }` that does not already define `to_json`
//! or `from_json`, this pass appends two synthesized methods marked
//! `FunctionOrigin::AutoDerive`:
//!
//! ```baml
//! function to_json(self) -> json throws ... {
//!     return {
//!         "f1": self.f1.to_json(),
//!         "f2": self.f2.to_json(),
//!         ...
//!     }
//! }
//!
//! function from_json(j: json) -> C throws ... {
//!     baml.json.from_string<C>(baml.json.stringify(j))
//! }
//! ```
//!
//! `to_json` honors user overrides at every depth via ordinary BAML method
//! dispatch (`self.f.to_json()` calls whatever `to_json` the field's type
//! defines — synthesized or user-written). For this to bottom out, every
//! built-in type (`string`, `int`, `Array<T>`, `Map<K,V>`, media types)
//! must also have `to_json`; those impls live in `baml_builtins2`.
//!
//! `from_json` is currently a wrapper around the runtime walker
//! (`baml.json.from_string<Self>` + `stringify`). This does *not* honor
//! user `from_json` overrides on nested fields — known follow-up.
//!
//! For generic classes the `Self` type-arg expands to `C<T1, T2, ...>`; the
//! BEP-039 type-arg threading machinery routes the runtime `T_i` from
//! `Object::Instance::class_type_args` through the call.
//!
//! User-defined `to_json` or `from_json` on the class suppresses synthesis
//! of *both* methods (matches the BEP "user override wins" rule).

use baml_base::Name;
use la_arena::Arena;
use text_size::TextRange;

use crate::ast::{
    AstSourceMap, ClassDef, Expr, ExprBody, ExprId, FunctionBodyDef, FunctionDef, FunctionOrigin,
    Param, SpannedTypeExpr, TypeExpr,
};

/// Run the auto-derive pass on a class. Appends synthesized `to_json` and
/// `from_json` methods unless the user already defined either one.
pub(crate) fn maybe_synthesize_json_methods(class: &mut ClassDef) {
    if class
        .methods
        .iter()
        .any(|m| m.name.as_str() == "to_json" || m.name.as_str() == "from_json")
    {
        return;
    }
    let span = class.name_span;
    class.methods.push(synthesize_to_json(class, span));
    class.methods.push(synthesize_from_json(class, span));
}

/// Build a `TypeExpr::Path` referencing the class itself with its own generic
/// parameters threaded through. For `class Container<T>` this produces
/// `Container<T>`; for `class User` this produces `User`.
fn class_self_type(class: &ClassDef) -> TypeExpr {
    let generic_args: Vec<TypeExpr> = class
        .generic_params
        .iter()
        .map(|gp| TypeExpr::Path {
            segments: vec![gp.clone()],
            generic_args: vec![],
            attrs: vec![],
        })
        .collect();
    TypeExpr::Path {
        segments: vec![class.name.clone()],
        generic_args,
        attrs: vec![],
    }
}

/// `baml.json.json` — the json type alias.
fn json_type() -> TypeExpr {
    TypeExpr::Path {
        segments: vec![Name::new("baml"), Name::new("json"), Name::new("json")],
        generic_args: vec![],
        attrs: vec![],
    }
}

/// `baml.json.<name>` — used for error class type expressions.
fn baml_json_class(name: &str) -> TypeExpr {
    TypeExpr::Path {
        segments: vec![Name::new("baml"), Name::new("json"), Name::new(name)],
        generic_args: vec![],
        attrs: vec![],
    }
}

fn union(variants: Vec<TypeExpr>) -> TypeExpr {
    TypeExpr::Union {
        variants,
        attrs: vec![],
    }
}

fn spanned(expr: TypeExpr, span: TextRange) -> SpannedTypeExpr {
    SpannedTypeExpr { expr, span }
}

/// Synthesize `function to_json(self) -> json throws JsonSerializationError | JsonParseError {
///     baml.json.parse(baml.json.to_string<Self>(self))
/// }`
fn synthesize_to_json(class: &ClassDef, span: TextRange) -> FunctionDef {
    let self_param = Param {
        name: Name::new("self"),
        type_expr: None,
        span,
        name_span: span,
    };

    let (body, source_map) = build_to_json_body(class, span);

    FunctionDef {
        name: Name::new("to_json"),
        generic_params: vec![],
        params: vec![self_param],
        return_type: Some(spanned(json_type(), span)),
        throws: Some(spanned(
            union(vec![
                baml_json_class("JsonSerializationError"),
                baml_json_class("JsonParseError"),
            ]),
            span,
        )),
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: FunctionOrigin::AutoDerive,
        attributes: vec![],
        span,
        name_span: span,
    }
}

/// Synthesize `function from_json(j: json) -> Self throws JsonParseError | JsonDecodeError {
///     baml.json.from_string<Self>(baml.json.stringify(j))
/// }`
fn synthesize_from_json(class: &ClassDef, span: TextRange) -> FunctionDef {
    let j_param = Param {
        name: Name::new("j"),
        type_expr: Some(spanned(json_type(), span)),
        span,
        name_span: span,
    };

    let (body, source_map) = build_from_json_body(class, span);

    FunctionDef {
        name: Name::new("from_json"),
        // `from_json` does not introduce its own generic params; the class's
        // generic params are in scope via the enclosing class.
        generic_params: vec![],
        params: vec![j_param],
        return_type: Some(spanned(class_self_type(class), span)),
        throws: Some(spanned(
            union(vec![
                baml_json_class("JsonParseError"),
                baml_json_class("JsonDecodeError"),
            ]),
            span,
        )),
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: FunctionOrigin::AutoDerive,
        attributes: vec![],
        span,
        name_span: span,
    }
}

/// Returns `true` if the type expression is definitely nullable — i.e., the
/// type system allows the value to be `null` at runtime.
///
/// This covers the two main surface forms:
/// - `T?` — `TypeExpr::Optional`
/// - `null` — `TypeExpr::Null`
/// - `A | B | null | ...` — a `TypeExpr::Union` containing a `Null` variant
///
/// We only inspect the top-level type expression; deeper structures (e.g.,
/// `(T?)[]`) are not nullable *at the field level* and don't need special
/// treatment here.
fn type_expr_is_nullable(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Optional { .. } => true,
        TypeExpr::Null { .. } => true,
        TypeExpr::Union { variants, .. } => {
            variants.iter().any(|v| matches!(v, TypeExpr::Null { .. }))
        }
        _ => false,
    }
}

/// Returns `true` if the type expression (and all nested types) are safe to
/// use with per-field `to_json()` synthesis. Returns `false` for types that do
/// not have a `to_json` method:
/// - `$rust_type` — opaque Rust-managed types have no BAML `to_json`
/// - function types — lambdas/function values have no `to_json`
/// - `unknown` / `error` / `builtin unknown` — unresolved types
///
/// Container types (`T?`, `T[]`, `map<K,V>`) are safe if their inner types
/// are safe, since `Array<T>.to_json()` and `Map<K,V>.to_json()` are
/// available from `baml_builtins2`.
fn type_expr_is_safe_for_per_field(ty: &TypeExpr) -> bool {
    match ty {
        // Types that have no `to_json` method — fall back to wrapper body
        TypeExpr::Rust { .. } => false,
        TypeExpr::Function { .. } => false,
        TypeExpr::Unknown { .. } => false,
        TypeExpr::Error { .. } => false,
        TypeExpr::BuiltinUnknown { .. } => false,
        // Container types: safe if inner types are safe
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            type_expr_is_safe_for_per_field(inner)
        }
        TypeExpr::Map { key, value, .. } => {
            type_expr_is_safe_for_per_field(key) && type_expr_is_safe_for_per_field(value)
        }
        TypeExpr::Union { variants, .. } => variants.iter().all(type_expr_is_safe_for_per_field),
        // All other types (primitives, paths, literals, media, etc.) are safe
        _ => true,
    }
}

/// Returns `true` if all fields of the class have types that are safe for
/// per-field `to_json()` synthesis.
fn class_is_safe_for_per_field_synthesis(class: &ClassDef) -> bool {
    class.fields.iter().all(|f| {
        f.type_expr
            .as_ref()
            .map(|st| type_expr_is_safe_for_per_field(&st.expr))
            .unwrap_or(false) // fields with missing type annotations are not safe
    })
}

/// Build the fallback AST for `baml.json.parse(baml.json.to_string<Self>(self))`.
///
/// Used when the class has fields with types that don't support `to_json()` —
/// e.g., `$rust_type`, function types, or `unknown`. The runtime serialization
/// walker handles these opaquely.
fn build_to_json_wrapper_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();

    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // `baml.json.to_string`
    let to_string_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("to_string"),
    ]));
    // `self`
    let self_arg = alloc(Expr::Path(vec![Name::new("self")]));
    // `baml.json.to_string<Self>(self)`
    let to_string_call = alloc(Expr::Call {
        callee: to_string_callee,
        type_args: vec![class_self_type(class)],
        args: vec![self_arg],
    });

    // `baml.json.parse`
    let parse_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("parse"),
    ]));
    // `baml.json.parse(baml.json.to_string<Self>(self))`
    let parse_call = alloc(Expr::Call {
        callee: parse_callee,
        type_args: vec![],
        args: vec![to_string_call],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(parse_call),
    };
    let source_map = AstSourceMap {
        expr_spans,
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        member_access_member_spans: std::collections::HashMap::new(),
        path_segment_spans: std::collections::HashMap::new(),
    };
    (body, source_map)
}

/// Build the AST for the BEP-038 per-field map literal:
///
/// ```baml
/// {
///     "f1": self.f1.to_json(),
///     "f2": self.f2.to_json(),
///     ...
/// }
/// ```
///
/// Each `self.<field>.to_json()` dispatches through ordinary BAML method
/// lookup, so user overrides on field types (nested classes, arrays, maps)
/// are honored automatically. Every built-in type has a `to_json` method
/// provided by `baml_builtins2` (Phase 5b.1–5b.4), so this bottoms out for
/// all reachable field types.
///
/// If any field has a type that doesn't support `to_json()` (e.g., `$rust_type`,
/// function types, `unknown`), falls back to the wrapper body.
fn build_to_json_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    use crate::ast::Literal;

    // Fall back to wrapper body for classes with unsafe field types
    if !class_is_safe_for_per_field_synthesis(class) {
        return build_to_json_wrapper_body(class, span);
    }

    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();

    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let mut entries: Vec<(ExprId, ExprId)> = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        // Key: the field name as a string literal — `"field_name"`.
        let key = alloc(Expr::Literal(Literal::String(
            field.name.as_str().to_string(),
        )));

        // Detect whether the field type is nullable (T? / null / T | null).
        // For nullable fields we must use optional chaining so that a null
        // value doesn't cause a "member access on null" error at runtime.
        let is_nullable = field
            .type_expr
            .as_ref()
            .map(|st| type_expr_is_nullable(&st.expr))
            .unwrap_or(false);

        //   1. `self`
        let self_path = alloc(Expr::Path(vec![Name::new("self")]));
        //   2. `self.<field>`
        let field_access = alloc(Expr::MemberAccess {
            base: self_path,
            member: field.name.clone(),
        });

        let value = if is_nullable {
            // Nullable field: `self.<field>?.to_json()`
            //
            // The `?.` short-circuits to `null` when the field is null.  Since
            // `null` is one of the arms of `baml.json.json`, the resulting
            // `json?` type is accepted directly wherever `json` is expected
            // (the compiler treats them as equivalent here).
            //
            //   3. `self.<field>?.to_json`  (optional member access)
            let to_json_callee = alloc(Expr::OptionalMemberAccess {
                base: field_access,
                member: Name::new("to_json"),
            });
            //   4. `self.<field>?.to_json()`  (regular call on the optional accessor)
            let call = alloc(Expr::Call {
                callee: to_json_callee,
                type_args: vec![],
                args: vec![],
            });
            //   5. `(self.<field>?.to_json())`  (OptionalChain scope delimiter)
            alloc(Expr::OptionalChain { expr: call })
        } else {
            // Non-nullable field: `self.<field>.to_json()`
            //   3. `self.<field>.to_json`
            let to_json_callee = alloc(Expr::MemberAccess {
                base: field_access,
                member: Name::new("to_json"),
            });
            //   4. `self.<field>.to_json()`
            alloc(Expr::Call {
                callee: to_json_callee,
                type_args: vec![],
                args: vec![],
            })
        };

        entries.push((key, value));
    }

    // Root expression: `{ "f1": self.f1.to_json(), ... }`.
    let map_expr = alloc(Expr::Map { entries });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(map_expr),
    };
    let source_map = AstSourceMap {
        expr_spans,
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        member_access_member_spans: std::collections::HashMap::new(),
        path_segment_spans: std::collections::HashMap::new(),
    };
    (body, source_map)
}

/// Build the AST for `baml.json.from_string<Self>(baml.json.stringify(j))`.
fn build_from_json_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();

    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // `baml.json.stringify`
    let stringify_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("stringify"),
    ]));
    // `j`
    let j_arg = alloc(Expr::Path(vec![Name::new("j")]));
    // `baml.json.stringify(j)`
    let stringify_call = alloc(Expr::Call {
        callee: stringify_callee,
        type_args: vec![],
        args: vec![j_arg],
    });

    // `baml.json.from_string`
    let from_string_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("from_string"),
    ]));
    // `baml.json.from_string<Self>(<above>)`
    let from_string_call = alloc(Expr::Call {
        callee: from_string_callee,
        type_args: vec![class_self_type(class)],
        args: vec![stringify_call],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(from_string_call),
    };
    let source_map = AstSourceMap {
        expr_spans,
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        member_access_member_spans: std::collections::HashMap::new(),
        path_segment_spans: std::collections::HashMap::new(),
    };
    (body, source_map)
}
