//! Auto-derived `from_json` synthesis for user-defined classes.
//!
//! Pure AST transform — no type information needed. For every user
//! `class C { f1: T1, f2: T2, ... }` that does not already define `from_json`,
//! this pass appends a synthesized method marked `FunctionOrigin::AutoDerive`:
//!
//! ```baml
//! function from_json(j: json) -> C throws ... {
//!     C {
//!         f1: baml.json.from_json<T1>(baml.json.field(j, "f1")),
//!         f2: baml.json.from_json<T2>(baml.json.field(j, "f2")),
//!         ...
//!     }
//! }
//! ```
//!
//! Every field routes through `baml.json.from_json<F>` so override dispatch
//! is centralized in the native handler: it `YieldToCall`s the field type's
//! `<fqn>.from_json` (user or auto-derived) for class fields, walks `List<C>`
//! and `Map<string, C>` element-by-element honoring per-element overrides,
//! and falls back to structural decode for primitives, enums, etc. The
//! `baml.json.field(j, "f")` helper extracts a json object's field value
//! since the `json` union doesn't typecheck under direct indexing.
//!
//! For generic classes the `Self` type-arg expands to `C<T1, T2, ...>`; the
//! BEP-039 type-arg threading machinery routes the runtime `T_i` from
//! `Object::Instance::class_type_args` through the call.
//!
//! `to_json` is NOT auto-derived: serialization is owned by the `baml.ToJson`
//! interface, and `baml.json.from(value)` is the universal driver that renders
//! non-implementors structurally (a bare `to_json` method trips the HIR
//! `ToJsonMustImplementInterface` ban). `from_json` remains synthesized because
//! deserialization has no interface yet (see the `FromJson` follow-up). A
//! user-defined `from_json` suppresses synthesis.

use baml_base::{Name, TypePath};
use la_arena::Arena;
use text_size::TextRange;

use crate::ast::{
    AstSourceMap, CallArg, ClassDef, Expr, ExprBody, ExprId, FunctionBodyDef, FunctionDef,
    FunctionDefaults, FunctionOrigin, Param, SpannedTypeExpr, TypeExpr,
};

/// Run the auto-derive pass on a class. Appends a synthesized `from_json`,
/// suppressed only when the user already defines `from_json`. (`to_json` is no
/// longer auto-derived — see the module docs.)
pub(crate) fn maybe_synthesize_derived_methods(class: &mut ClassDef) {
    let span = class.name_span;
    let has_from_json = class.methods.iter().any(|m| m.name.as_str() == "from_json");
    if !has_from_json {
        class.methods.push(synthesize_from_json(class, span));
    }
    // NB: `to_json` and `to_string` are intentionally NOT auto-derived. The
    // `baml.ToJson` / `baml.ToString` interfaces own user-facing serialization;
    // `baml.json.from(value)` / `string.from(value)` are the universal drivers
    // that render non-implementors structurally (and the `obj.to_json()` /
    // `obj.to_string()` sugar lowers to them), so no per-class method is
    // synthesized — and a synthesized one would trip the HIR rule that forbids a
    // direct `to_json` / `to_string` method
    // (`ToJsonMustImplementInterface` / `ToStringMustImplementInterface`).
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
            associated_type_bindings: vec![],
            attrs: vec![],
        })
        .collect();
    TypeExpr::Path {
        segments: vec![class.name.clone()],
        generic_args,
        associated_type_bindings: vec![],
        attrs: vec![],
    }
}

/// `baml.json.json` — the json type alias.
fn json_type() -> TypeExpr {
    TypeExpr::Path {
        segments: vec![Name::new("baml"), Name::new("json"), Name::new("json")],
        generic_args: vec![],
        associated_type_bindings: vec![],
        attrs: vec![],
    }
}

/// `baml.json.<name>` — used for error class type expressions.
fn baml_json_class(name: &str) -> TypeExpr {
    TypeExpr::Path {
        segments: vec![Name::new("baml"), Name::new("json"), Name::new(name)],
        generic_args: vec![],
        associated_type_bindings: vec![],
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

/// Synthesize `function from_json(j: json) -> Self throws JsonParseError | JsonDecodeError {
///     baml.json.from_string<Self>(baml.json.stringify(j))
/// }`
fn synthesize_from_json(class: &ClassDef, span: TextRange) -> FunctionDef {
    let j_param = Param {
        name: Name::new("j"),
        type_expr: Some(spanned(json_type(), span)),
        default: None,
        span,
        name_span: span,
    };

    let (body, source_map) = build_from_json_body(class, span);

    FunctionDef {
        name: Name::new("from_json"),
        // `from_json` does not introduce its own generic params; the class's
        // generic params are in scope via the enclosing class.
        generic_params: vec![],
        generic_param_bounds: vec![],
        params: vec![j_param],
        defaults: FunctionDefaults::empty(),
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
        docstring: None,
        is_tagged_template_tag: false,
        span,
        name_span: span,
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
/// are safe. `TypeVar` references (`T` where `T` is a class generic parameter)
/// are handled at the per-field level by routing through the
/// `baml.json.to_json(v)` runtime-dispatch helper, so they don't disqualify a
/// class from per-field synthesis.
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
        // All other types (primitives, concrete paths, literals, media, TypeVars) are safe
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

/// Build the fallback AST `baml.json.from_string<Self>(baml.json.stringify(j))`.
///
/// Used when the class has fields with types that don't support per-field
/// dispatch — e.g., `$rust_type`, function types, `unknown`.
fn build_from_json_wrapper_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
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
        args: vec![CallArg::positional(j_arg)],
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
        args: vec![CallArg::positional(stringify_call)],
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
        ..Default::default()
    };
    (body, source_map)
}

/// Build the AST for the BEP-038 per-field class-instance constructor:
///
/// ```baml
/// C<T1, ...> {
///     f1: baml.json.from_json<F1>(baml.json.field(j, "f1")),
///     f2: baml.json.from_json<F2>(baml.json.field(j, "f2")),
///     ...
/// }
/// ```
///
/// Every field routes through `baml.json.from_json<F>` so override dispatch
/// is centralized in the native handler. The native looks up `<fqn>.from_json`
/// for class field types and `YieldToCall`s it (honoring user overrides), and
/// walks `List<C>` / `Map<string, C>` per-element honoring nested overrides.
///
/// If any field has a type that doesn't support per-field dispatch (e.g.,
/// `$rust_type`, function types, `unknown`), falls back to the wrapper body
/// (`baml.json.from_string<Self>(stringify(j))`).
fn build_from_json_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    // Fall back to wrapper body for classes with unsafe field types.  Reuses
    // the same predicate as `to_json` so the two synthesizers stay aligned.
    if !class_is_safe_for_per_field_synthesis(class) {
        return build_from_json_wrapper_body(class, span);
    }

    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();

    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let mut entries: Vec<(Name, ExprId)> = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        // Field type, used as the type-arg to `baml.json.from_json<F>`.  If
        // the field has no annotation (legal but unusual), the wrapper-body
        // branch above would already have caught it via
        // `class_is_safe_for_per_field_synthesis`.
        let field_ty = field
            .type_expr
            .as_ref()
            .map(|st| st.expr.clone())
            .unwrap_or(TypeExpr::Path {
                segments: vec![Name::new("unknown")],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![],
            });

        // `baml.json.field`
        let field_helper = alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("json"),
            Name::new("field"),
        ]));
        // `j`
        let j_path = alloc(Expr::Path(vec![Name::new("j")]));
        // `"<field_name>"`
        let key_lit = alloc(Expr::Literal(crate::ast::Literal::String(
            field.name.as_str().to_string(),
        )));
        // `baml.json.field(j, "<field_name>")`
        let field_json = alloc(Expr::Call {
            callee: field_helper,
            type_args: vec![],
            args: vec![CallArg::positional(j_path), CallArg::positional(key_lit)],
        });

        // `baml.json.from_json`
        let from_json_callee = alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("json"),
            Name::new("from_json"),
        ]));
        // `baml.json.from_json<F>(baml.json.field(j, "<field_name>"))`
        let value_expr = alloc(Expr::Call {
            callee: from_json_callee,
            type_args: vec![field_ty],
            args: vec![CallArg::positional(field_json)],
        });

        entries.push((field.name.clone(), value_expr));
    }

    // `C<T1, ...> { f1: ..., f2: ..., ... }`
    let generic_args: Vec<TypeExpr> = class
        .generic_params
        .iter()
        .map(|gp| TypeExpr::Path {
            segments: vec![gp.clone()],
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        })
        .collect();
    let object = alloc(Expr::Object {
        type_name: TypePath::bare(class.name.clone()),
        type_args: generic_args,
        fields: entries,
        spreads: vec![],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(object),
    };
    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };
    (body, source_map)
}
