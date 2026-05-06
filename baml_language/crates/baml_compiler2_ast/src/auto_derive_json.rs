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

/// Build the AST for `baml.json.parse(baml.json.to_string<Self>(self))`.
///
/// **Temporary wrapper shape**, kept as-is until primitive companion classes
/// (`Int`, `Float`, `Bool`, `Null`) gain `to_json` methods so a per-field
/// `self.f.to_json()` map literal can bottom out cleanly. See the module
/// docstring's TODO above.
fn build_to_json_body(class: &ClassDef, span: TextRange) -> (ExprBody, AstSourceMap) {
    let mut exprs: Arena<Expr> = Arena::new();
    let mut expr_spans: Arena<TextRange> = Arena::new();

    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let to_string_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("to_string"),
    ]));
    let self_arg = alloc(Expr::Path(vec![Name::new("self")]));
    let to_string_call = alloc(Expr::Call {
        callee: to_string_callee,
        type_args: vec![class_self_type(class)],
        args: vec![self_arg],
    });

    let parse_callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("json"),
        Name::new("parse"),
    ]));
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
