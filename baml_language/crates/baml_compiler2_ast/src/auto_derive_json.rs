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
//! `from_json` mirrors the same shape using a per-field class-instance
//! constructor:
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
//! User-defined `to_json` or `from_json` on the class suppresses synthesis
//! of *both* methods (matches the BEP "user override wins" rule).

use baml_base::{Name, TypePath};
use la_arena::Arena;
use text_size::TextRange;

use crate::ast::{
    AstSourceMap, CallArg, ClassDef, Expr, ExprBody, ExprId, FunctionBodyDef, FunctionDef,
    FunctionDefaults, FunctionOrigin, Param, SpannedTypeExpr, TypeExpr,
};

/// Run the auto-derive pass on a class. Appends a synthesized `to_json` and
/// `from_json` independently — each is suppressed only when the user already
/// defines that specific method. This matches BEP-038's `to_json` / `from_json`
/// Protocol, which treats the two contracts as independent and lets a class
/// override only one (e.g., a custom `from_json` that returns sentinel
/// instances while keeping the auto-derived structural `to_json`).
pub(crate) fn maybe_synthesize_json_methods(class: &mut ClassDef) {
    let span = class.name_span;
    let has_to_json = class.methods.iter().any(|m| m.name.as_str() == "to_json");
    let has_from_json = class.methods.iter().any(|m| m.name.as_str() == "from_json");
    if !has_to_json {
        class.methods.push(synthesize_to_json(class, span));
    }
    if !has_from_json {
        class.methods.push(synthesize_from_json(class, span));
    }
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
        default: None,
        span,
        name_span: span,
    };

    let (body, source_map) = build_to_json_body(class, span);

    FunctionDef {
        name: Name::new("to_json"),
        generic_params: vec![],
        params: vec![self_param],
        defaults: FunctionDefaults::empty(),
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
        docstring: None,
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

/// Returns `true` if `ty` is a bare reference to one of the class's generic
/// parameters (e.g., `T` in `class Box<T> { value T }`).
///
/// At MIR lowering, `Tir2Ty::TypeVar` is erased to `Ty::Void`, so MIR cannot
/// route a `MemberAccess` on a `TypeVar`-typed receiver to a concrete `to_json`
/// method — it falls through to a generic map-element load that panics at
/// runtime. The auto-derive synthesizer detects this case at AST time and
/// emits `baml.json.to_json(self.<f>)` instead, which dispatches via the
/// runtime value's actual type (`make_to_json_callee` in `bex_vm`).
fn type_expr_is_typevar_reference(
    ty: &TypeExpr,
    generic_params: &std::collections::HashSet<&str>,
) -> bool {
    matches!(
        ty,
        TypeExpr::Path { segments, generic_args, .. }
            if segments.len() == 1
                && generic_args.is_empty()
                && generic_params.contains(segments[0].as_str())
    )
}

/// Returns `true` if `ty` is a `TypeVar` reference, possibly wrapped in
/// `Optional` or a nullable `Union`. Once null is short-circuited away by
/// `?.` or narrowing, the live receiver still has `TypeVar` type — so a
/// `.to_json()` call would hit the same MIR `Ty::Void` erasure described
/// on `type_expr_is_typevar_reference`. Detect those cases up front and
/// route them through `baml.json.to_json(self.<f>)` instead.
fn type_expr_resolves_to_typevar(
    ty: &TypeExpr,
    generic_params: &std::collections::HashSet<&str>,
) -> bool {
    if type_expr_is_typevar_reference(ty, generic_params) {
        return true;
    }
    match ty {
        TypeExpr::Optional { inner, .. } => type_expr_resolves_to_typevar(inner, generic_params),
        TypeExpr::Union { variants, .. } => variants.iter().any(|v| {
            !matches!(v, TypeExpr::Null { .. }) && type_expr_resolves_to_typevar(v, generic_params)
        }),
        _ => false,
    }
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
        args: vec![CallArg::positional(self_arg)],
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
        args: vec![CallArg::positional(to_string_call)],
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
        call_arg_label_spans: std::collections::HashMap::new(),
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
/// provided by `baml_builtins2`, so this bottoms out for all reachable
/// field types.
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

    let generic_params: std::collections::HashSet<&str> = class
        .generic_params
        .iter()
        .map(smol_str::SmolStr::as_str)
        .collect();

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

        // Detect whether the field type is (or unwraps to) a class generic
        // parameter — bare `T`, `T?`, or `T | null`. MIR's
        // `lower_member_access` cannot dispatch a method through a TypeVar
        // receiver (the receiver type is erased to `Ty::Void` before reaching
        // MIR, so `field_idx` is `None` and the lowering falls through to a
        // dynamic-map load that panics on primitive/instance values). The
        // `T?` / `T | null` cases hit the same panic after `?.` short-circuits
        // away null. Route these through `baml.json.to_json(self.<f>)` so the
        // free function dispatches on the runtime value's actual type via
        // `make_to_json_callee`, preserving user `to_json` overrides through
        // the generic boundary.
        let is_typevar = field
            .type_expr
            .as_ref()
            .map(|st| type_expr_resolves_to_typevar(&st.expr, &generic_params))
            .unwrap_or(false);

        //   1. `self`
        let self_path = alloc(Expr::Path(vec![Name::new("self")]));
        //   2. `self.<field>`
        let field_access = alloc(Expr::MemberAccess {
            base: self_path,
            member: field.name.clone(),
        });

        let value = if is_typevar {
            // TypeVar-typed field: `baml.json.to_json(self.<field>)`.  The
            // free function performs runtime dispatch on the value's actual
            // type, so this works whether `T` is a primitive, a class with a
            // user `to_json` override, an array, or a map.
            let to_json_path = alloc(Expr::Path(vec![
                Name::new("baml"),
                Name::new("json"),
                Name::new("to_json"),
            ]));
            alloc(Expr::Call {
                callee: to_json_path,
                type_args: vec![],
                args: vec![CallArg::positional(field_access)],
            })
        } else if is_nullable {
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
        call_arg_label_spans: std::collections::HashMap::new(),
    };
    (body, source_map)
}

/// Build the fallback AST `baml.json.from_string<Self>(baml.json.stringify(j))`.
///
/// Used when the class has fields with types that don't support per-field
/// dispatch — e.g., `$rust_type`, function types, `unknown`.  Mirrors the
/// `to_json` wrapper fallback (`build_to_json_wrapper_body`).
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
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        member_access_member_spans: std::collections::HashMap::new(),
        path_segment_spans: std::collections::HashMap::new(),
        call_arg_label_spans: std::collections::HashMap::new(),
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
            attrs: vec![],
        })
        .collect();
    let object = alloc(Expr::Object {
        type_name: Some(TypePath::bare(class.name.clone())),
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
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        member_access_member_spans: std::collections::HashMap::new(),
        path_segment_spans: std::collections::HashMap::new(),
        call_arg_label_spans: std::collections::HashMap::new(),
    };
    (body, source_map)
}
