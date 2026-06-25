//! Auto-derived `from_json` synthesis for user-defined classes.
//!
//! Pure AST transform — no type information needed. For every user
//! `class C { ... }` that does not already define `from_json`, this pass appends
//! a thin synthesized delegate marked `FunctionOrigin::AutoDerive`:
//!
//! ```baml
//! function from_json(j: json) -> C throws ... {
//!     baml.json.to<C>(j)
//! }
//! ```
//!
//! The override-honoring per-field decode lives in the `baml.json.to` driver
//! (VM `class_from_json_start`): it decodes each field through `baml.json.to`,
//! dispatching a field type's `baml.FromJson` override (and walking lists / maps
//! / optionals), and structurally decodes leaves. So the synthesized body is the
//! *structural default* for non-implementors; a class customizes deserialization
//! by `implements baml.FromJson` instead (a bare user `from_json` method trips
//! the HIR `FromJsonMustImplementInterface` ban — the synthesized delegate is
//! exempt). There is no recursion: the driver decodes fields directly rather
//! than re-calling this `from_json`.
//!
//! `to_json` is NOT auto-derived at all — `baml.ToJson` + the `baml.json.from`
//! Rust walker own serialization. `from_json` keeps a thin delegate because the
//! deserialize side constructs `Self` (a static, receiver-less call), which the
//! delegate expresses without a static-call sugar.

use baml_base::Name;
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
    // Suppress the delegate when the user already provides `from_json` — a
    // (E0143-banned) bare method, or an `implements baml.FromJson { function
    // from_json }` override (in `class.implements`). For an implementor the
    // override IS the `C.from_json`, so no delegate is needed.
    let has_from_json = class.methods.iter().any(|m| m.name.as_str() == "from_json")
        || class
            .implements
            .iter()
            .any(|b| b.methods.iter().any(|m| m.name.as_str() == "from_json"));
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
fn spanned(expr: TypeExpr, span: TextRange) -> SpannedTypeExpr {
    SpannedTypeExpr { expr, span }
}

/// Synthesize the thin delegate
/// `function from_json(j: json) -> Self throws JsonDecodeError { baml.json.to<Self>(j) }`.
fn synthesize_from_json(class: &ClassDef, span: TextRange) -> FunctionDef {
    let j_param = Param {
        name: Name::new("j"),
        type_expr: Some(spanned(json_type(), span)),
        default: None,
        span,
        name_span: span,
    };

    let (body, source_map) = build_from_json_delegate_body(class, span);

    FunctionDef {
        name: Name::new("from_json"),
        // `from_json` does not introduce its own generic params; the class's
        // generic params are in scope via the enclosing class.
        generic_params: vec![],
        generic_param_bounds: vec![],
        params: vec![j_param],
        defaults: FunctionDefaults::empty(),
        return_type: Some(spanned(class_self_type(class), span)),
        // Decoding a `json` value into `Self` throws `JsonDecodeError` on shape
        // mismatch; it never parses, so it cannot throw `JsonParseError`.
        throws: Some(spanned(baml_json_class("JsonDecodeError"), span)),
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

/// Build the structural-default `from_json` body: `baml.json.to<Self>(j)`.
///
/// The override-honoring per-field decode lives in the `baml.json.to` driver
/// (VM `class_from_json_start`), so the auto-derived body is a thin delegate: it
/// routes through any `baml.FromJson` override on a field's type and decodes
/// structurally otherwise, with no recursion (the driver decodes fields directly
/// rather than re-calling this `from_json`). The `baml.FromJson` interface owns
/// user-facing customization; this delegate is the structural default for
/// non-implementors.
fn build_from_json_delegate_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };
    let to_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("to"),
    ]));
    let j_arg = alloc(Expr::Path(vec![Name::new("j")]));
    let to_call = alloc(Expr::Call {
        callee: to_callee,
        type_args: vec![class_self_type(class)],
        args: vec![CallArg::positional(j_arg)],
    });
    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(to_call),
    };
    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };
    (body, source_map)
}
