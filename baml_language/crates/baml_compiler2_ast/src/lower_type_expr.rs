//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.
//!
//! Types carry no attributes (BEP-075); `@` attributes belong to declarations.

use baml_base::Name;
use baml_compiler_syntax::{
    FunctionTypeParam, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodeExt, SyntaxToken,
    ast::{FunctionType as CstFunctionType, TypeExpr as CstTypeExpr},
};
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        AssociatedTypeBinding, FunctionTypeParam as AstFunctionTypeParam, TypeExpr, TypeExprKind,
    },
};

/// Convert a CST `TypeExpr` node to our `ast::TypeExpr` recursive enum.
///
/// Called by `lower_cst.rs` for field/alias/param/return-type positions.
pub(crate) fn lower_type_expr_node(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    if type_expr.is_union() {
        return lower_union(type_expr, diags);
    }

    // Union operands use the ordinary lowering path, with coverage checking
    // so recognizing one child cannot silently discard the rest of the type.
    let mut syntax = TypeSyntaxCoverage::new(type_expr, is_union_member(type_expr));
    let base = lower_base(type_expr, diags, &mut syntax);
    let modifiers = type_expr.postfix_modifiers();
    for modifier in &modifiers {
        match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => {
                syntax.token(SyntaxKind::QUESTION);
            }
            baml_compiler_syntax::ast::TypePostFixModifier::Array => {
                syntax.token(SyntaxKind::L_BRACKET);
                syntax.token(SyntaxKind::R_BRACKET);
            }
        }
    }
    if let Some(error) = syntax.report_remaining(type_expr.syntax(), diags) {
        return error;
    }
    apply_modifiers(base, &modifiers)
}

fn is_union_member(type_expr: &CstTypeExpr) -> bool {
    type_expr
        .syntax()
        .parent()
        .and_then(CstTypeExpr::cast)
        .is_some_and(|parent| parent.is_union())
}

/// Lower complete union operands through the same path as standalone types.
fn lower_union(type_expr: &CstTypeExpr, diags: &mut Vec<LoweringDiagnostic>) -> TypeExpr {
    let mut syntax = TypeSyntaxCoverage::new(type_expr, true);
    let variants = type_expr
        .union_members()
        .map(|member| {
            syntax.node(member.syntax());
            lower_type_expr_node(&member, diags)
        })
        .collect();
    // Only separators and trivia may occur outside the operand nodes.
    syntax
        .remaining
        .retain(|element| element.kind() != SyntaxKind::PIPE);
    if let Some(error) = syntax.report_remaining(type_expr.syntax(), diags) {
        return error;
    }
    TypeExprKind::Union { variants }.at(type_expr.syntax().span_range())
}

/// Apply postfix modifiers (`[]`, `?`) to a base type, wrapping it in
/// `List` / `Optional` layers.
fn apply_modifiers(
    base: TypeExpr,
    modifiers: &[baml_compiler_syntax::ast::TypePostFixModifier],
) -> TypeExpr {
    let mut result = base;
    for modifier in modifiers {
        let span = result.span;
        match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => {
                result = TypeExprKind::Optional {
                    inner: Box::new(result),
                }
                .at(span);
            }
            baml_compiler_syntax::ast::TypePostFixModifier::Array => {
                result = TypeExprKind::List {
                    inner: Box::new(result),
                }
                .at(span);
            }
        }
    }
    result
}

/// Extract the base type (function types, parens, terminals).
/// No modifier handling. Unions are handled by `lower_union`.
fn lower_base(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    syntax: &mut TypeSyntaxCoverage,
) -> TypeExpr {
    let span = type_expr.syntax().span_range();
    if let Some(unreflect) = type_expr
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::UNREFLECT_TYPE)
    {
        syntax.node(&unreflect);
        // The parser keeps the removed `unreflect(expr)` as a type atom so it
        // recovers cleanly, and has already reported it (E0098).
        return TypeExprKind::Error.at(unreflect.span_range());
    }
    // BUG: a qualified projection captures only a single member — `(base as I).A.B`
    // drops the trailing `.B` (`associated_type_projection` returns one member). A chained
    // explicit qualifier (`(T as Outer).Asdf.Assoc`) therefore silently loses `.Assoc`,
    // unlike the unqualified `T.Asdf.Assoc`. Fixing this is a grammar/CST change: parse the
    // full member chain after `(base as I)` and fold it into nested projections here.
    if let Some((base, interface, member)) = type_expr.associated_type_projection() {
        syntax.node(base.syntax());
        syntax.node(interface.syntax());
        syntax.exact_token(&member);
        syntax.parens();
        syntax.token(SyntaxKind::KW_AS);
        syntax.token(SyntaxKind::DOT);
        return TypeExprKind::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_node(&base, diags)),
            interface: Some(Box::new(lower_type_expr_node(&interface, diags))),
            member: Name::new(member.text()),
        }
        .at(span);
    }

    // Handle function types like `(x: int, y: int) -> bool`. A function type
    // cannot declare its own generic parameters (rejected by the parser); any
    // leading `<...>` is left in the CST for recovery and ignored here.
    if let Some(function) = type_expr.function_type() {
        syntax.node(function.syntax());
        return lower_function_type(&function, diags, is_union_member(type_expr));
    }

    // Handle parenthesized types like `(int | string)`
    if let Some(inner) = type_expr.inner_type_expr() {
        syntax.parens();
        if let Some(param) = type_expr.function_type_params().first() {
            syntax.node(param.syntax());
        } else {
            syntax.node(inner.syntax());
        }
        return lower_type_expr_node(&inner, diags);
    }

    // Handle parenthesized unions: `(A | B)` where the union is inside parens
    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 && !is_union_member(type_expr) {
            let members: Vec<TypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_node(&t, diags))
                .collect();
            if !members.is_empty() {
                return TypeExprKind::Union { variants: members }.at(span);
            }
        }
    }

    lower_base_type(type_expr, diags, syntax)
}

fn lower_function_type(
    function: &CstFunctionType,
    diags: &mut Vec<LoweringDiagnostic>,
    check_syntax: bool,
) -> TypeExpr {
    let span = function.syntax().span_range();
    let mut syntax = TypeSyntaxCoverage::new_node(function.syntax(), check_syntax);
    syntax.parens();
    syntax.token(SyntaxKind::ARROW);
    for param in function.params() {
        syntax.node(param.syntax());
    }
    // Commas separate parameters, including an optional trailing comma.
    syntax
        .remaining
        .retain(|element| element.kind() != SyntaxKind::COMMA);
    if let Some(ret) = function.return_type() {
        syntax.node(ret.syntax());
        let mut return_syntax = TypeSyntaxCoverage::new_node(ret.syntax(), check_syntax);
        if let Some(ty) = ret.ty() {
            return_syntax.node(ty.syntax());
        }
        if let Some(error) = return_syntax.report_remaining(ret.syntax(), diags) {
            return error;
        }
    }
    if let Some(throws) = function.throws_clause() {
        syntax.node(throws.syntax());
    }
    let params = function
        .params()
        .iter()
        .map(|p| {
            let name = p.name().map(|s| Name::new(&s));
            let optional = p.is_optional();
            let ty = p
                .ty()
                .map(|t| lower_type_expr_node(&t, diags))
                .unwrap_or_else(|| TypeExprKind::Missing.at(span));
            AstFunctionTypeParam { name, optional, ty }
        })
        .collect();
    let ret = function
        .return_type()
        .and_then(|ret| ret.ty())
        .map(|t| lower_type_expr_node(&t, diags))
        .unwrap_or_else(|| TypeExprKind::Missing.at(span));
    let throws = function
        .throws_clause()
        .and_then(|throws| throws.type_expr())
        .map(|t| Box::new(lower_type_expr_node(&t, diags)));
    if let Some(error) = syntax.report_remaining(function.syntax(), diags) {
        return error;
    }
    TypeExprKind::Function {
        params,
        ret: Box::new(ret),
        throws,
    }
    .at(span)
}

fn lower_base_type(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    syntax: &mut TypeSyntaxCoverage,
) -> TypeExpr {
    let span = type_expr.syntax().span_range();
    if let Some(s) = type_expr.string_literal() {
        if let Some(node) = type_expr
            .syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::STRING_LITERAL)
        {
            syntax.node(&node);
        }
        return TypeExprKind::Literal {
            value: baml_base::Literal::String(s),
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.bigint_literal() {
        syntax.exact_token(&tok);
        if negated {
            syntax.token(SyntaxKind::MINUS);
        }
        let value = crate::lower_bigint_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Bigint(if negated { -value } else { value }),
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.integer_literal() {
        syntax.exact_token(&tok);
        if negated {
            syntax.token(SyntaxKind::MINUS);
        }
        let value = crate::lower_int_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Int(if negated { -value } else { value }),
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.float_literal() {
        syntax.exact_token(&tok);
        if negated {
            syntax.token(SyntaxKind::MINUS);
        }
        let text = baml_base::num_lit::normalize_float_literal(tok.text());
        return TypeExprKind::Literal {
            value: baml_base::Literal::Float(if negated { format!("-{text}") } else { text }),
        }
        .at(span);
    }

    if let Some(b) = type_expr.bool_literal() {
        syntax.path(type_expr, if b { "true" } else { "false" });
        return TypeExprKind::Literal {
            value: baml_base::Literal::Bool(b),
        }
        .at(span);
    }

    // Check for map type with type args
    if let Some(name) = type_expr.dotted_name() {
        syntax.path(type_expr, &name);
        if let Some(args) = type_expr.type_args() {
            syntax.node(&args);
        }
        let args = type_expr.type_arg_exprs();
        let associated_type_bindings = type_expr
            .type_arg_associated_bindings()
            .iter()
            .filter_map(|b| lower_associated_type_binding(b, diags))
            .collect();
        // Named type (primitive or user-defined), preserving generic args
        let generic_args: Vec<TypeExpr> = args
            .iter()
            .map(|arg| lower_type_expr_node(arg, diags))
            .collect();
        return lower_from_type_name_with_generic_args(
            &name,
            generic_args,
            associated_type_bindings,
            span,
            diags,
        );
    }

    TypeExprKind::Missing.at(span)
}

/// Account for each immediate token and child node represented by a union
/// member's AST. Nested union members are checked recursively during lowering.
/// Finding a familiar child is not permission to ignore its siblings.
struct TypeSyntaxCoverage {
    remaining: Vec<SyntaxElement>,
}

impl TypeSyntaxCoverage {
    fn new(type_expr: &CstTypeExpr, enabled: bool) -> Self {
        Self::new_node(type_expr.syntax(), enabled)
    }

    fn new_node(node: &SyntaxNode, enabled: bool) -> Self {
        let remaining = if enabled {
            node.children_with_tokens()
                .filter(|element| !element.kind().is_trivia())
                .collect()
        } else {
            Vec::new()
        };
        Self { remaining }
    }

    fn node(&mut self, node: &SyntaxNode) {
        self.remaining
            .retain(|element| element.as_node() != Some(node));
    }

    fn exact_token(&mut self, token: &SyntaxToken) {
        self.remaining
            .retain(|element| element.as_token() != Some(token));
    }

    fn token(&mut self, kind: SyntaxKind) {
        if let Some(index) = self
            .remaining
            .iter()
            .position(|element| element.as_token().is_some_and(|token| token.kind() == kind))
        {
            self.remaining.remove(index);
        }
    }

    fn parens(&mut self) {
        self.token(SyntaxKind::L_PAREN);
        self.token(SyntaxKind::R_PAREN);
    }

    fn path(&mut self, type_expr: &CstTypeExpr, name: &str) {
        // Consume exactly the leading path recognized by dotted_name.
        let tokens = type_expr
            .syntax()
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|token| !token.kind().is_trivia());
        for token in tokens
            .skip_while(|token| token.kind() != SyntaxKind::WORD)
            .take(name.split('.').count() * 2 - 1)
        {
            self.exact_token(&token);
        }
    }

    fn report_remaining(
        &self,
        node: &SyntaxNode,
        diags: &mut Vec<LoweringDiagnostic>,
    ) -> Option<TypeExpr> {
        let (span, syntax) = self.remaining()?;
        diags.push(LoweringDiagnostic::UnconsumedTypeSyntax { syntax, span });
        Some(TypeExprKind::Error.at(node.span_range()))
    }

    fn remaining(&self) -> Option<(TextRange, String)> {
        let first = self.remaining.first()?;
        Some((
            first
                .as_node()
                .map_or_else(|| first.text_range(), SyntaxNodeExt::span_range),
            self.remaining
                .iter()
                .map(|element| format!("{:?}", element.kind()))
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }
}

/// Create a `TypeExpr` from a type name string with optional generic arguments.
///
/// User-defined paths preserve their arguments. Compiler aliases check their
/// declared arity before lowering, so invalid arguments cannot disappear.
pub(crate) fn lower_associated_type_binding(
    binding: &baml_compiler_syntax::ast::AssociatedTypeDecl,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<AssociatedTypeBinding> {
    let name = binding.name()?;
    let ty = binding
        .default_or_binding()
        .map(|ty| lower_type_expr_node(&ty, diags))
        .unwrap_or_else(|| TypeExprKind::Missing.at(TextRange::default()));
    Some(AssociatedTypeBinding {
        name: Name::new(name.text()),
        ty: Box::new(ty),
    })
}

fn lower_from_type_name_with_generic_args(
    name: &str,
    mut generic_args: Vec<TypeExpr>,
    associated_type_bindings: Vec<AssociatedTypeBinding>,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    use baml_type::{
        PrimitiveType as P,
        compiler_aliases::{self, AliasTarget},
    };
    let path = |generic_args, associated_type_bindings| TypeExprKind::Path {
        path: baml_base::Path::new(name.split('.').map(Name::new).collect()),
        generic_args,
        associated_type_bindings,
    };
    let kind = if let Some(alias) = compiler_aliases::by_spelling(name) {
        if generic_args.len() != alias.arity || !associated_type_bindings.is_empty() {
            diags.push(LoweringDiagnostic::InvalidBuiltinTypeArguments {
                name: name.to_owned(),
                expected: alias.arity,
                got: generic_args.len(),
                associated_bindings: associated_type_bindings.len(),
                span,
            });
            // The syntax parsed successfully. Recover without a second,
            // misleading "could not parse type expression" diagnostic.
            return TypeExprKind::Unknown.at(span);
        }
        match alias.target {
            AliasTarget::Primitive(primitive) => match primitive {
                P::Int => TypeExprKind::Int,
                P::Bigint => TypeExprKind::Bigint,
                P::Float => TypeExprKind::Float,
                P::String => TypeExprKind::String,
                P::Bool => TypeExprKind::Bool,
                P::Null => TypeExprKind::Null,
                P::Uint8Array => TypeExprKind::Uint8Array,
                P::Image | P::Audio | P::Video | P::Pdf => TypeExprKind::Media {
                    kind: match primitive {
                        P::Image => baml_base::MediaKind::Image,
                        P::Audio => baml_base::MediaKind::Audio,
                        P::Video => baml_base::MediaKind::Video,
                        P::Pdf => baml_base::MediaKind::Pdf,
                        _ => unreachable!("media primitive"),
                    },
                },
            },
            AliasTarget::Never => TypeExprKind::Never,
            // `void` is the unit type's second spelling: it denotes `null` in
            // every position, so it lowers to the very same node.
            AliasTarget::Void => TypeExprKind::Null,
            AliasTarget::Unknown => TypeExprKind::Unknown,
            AliasTarget::Type => TypeExprKind::Type,
            AliasTarget::Map => {
                let value = generic_args.pop().expect("checked map arity");
                let key = generic_args.pop().expect("checked map arity");
                TypeExprKind::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                }
            }
            AliasTarget::Json => {
                let definition = alias.definition.expect("json has an alias declaration");
                TypeExprKind::Path {
                    path: definition.source_path(),
                    generic_args,
                    associated_type_bindings,
                }
            }
            AliasTarget::Future => path(generic_args, associated_type_bindings),
            AliasTarget::List => unreachable!("arrays use postfix syntax"),
        }
    } else {
        match name {
            "_" => TypeExprKind::Infer,
            "$rust_type" => TypeExprKind::Rust,
            _ => path(generic_args, associated_type_bindings),
        }
    };
    kind.at(span)
}

/// Validate `_` placement in a `throws` clause, neutralizing illegal holes.
///
/// `_` is the open-contract marker (`throws AppError | _` or bare `throws _`),
/// so it is allowed as a top-level member of the clause and is left in place
/// (the TIR firewall fills it from the inferred throw set). But a `_` *nested*
/// inside a thrown type (`throws Err<_>`) has nothing to infer from; it is
/// reported and rewritten to an error sentinel (see [`check_wildcard_type`]) so
/// it never reaches type checking as an inference hole.
pub(crate) fn check_throws_wildcard(
    type_expr: &mut TypeExpr,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    match &mut type_expr.kind {
        // Bare `throws _` — the whole error set is inferred; keep the hole.
        TypeExprKind::Infer => {}
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                // A top-level `_` member is the open slot (kept); anything
                // deeper is a non-inferable nested hole (rewritten to an error).
                if !matches!(v.kind, TypeExprKind::Infer) {
                    check_wildcard_type(v, "a `throws` clause", span, diags);
                }
            }
        }
        _ => check_wildcard_type(type_expr, "a `throws` clause", span, diags),
    }
}

/// Reject — and neutralize — the `_` wildcard type at a DECLARATION-site type
/// position, where it cannot be inferred.
///
/// `_` is an inference hole. This firewall governs only the declaration sites it
/// is applied to (a signature parameter/return, a field, an alias, a generic
/// bound): there is nothing local to infer a hole from there, so every
/// occurrence is reported AND rewritten to [`TypeExprKind::Error`]. It then
/// lowers to the error-recovery `Ty::Error` sentinel rather than an inference hole
/// — which would otherwise reach type normalization, where an inference hole has
/// no sound form (see `baml_type::normalize`).
///
/// An inference hole therefore reaches type checking only from the positions this
/// firewall does NOT cover, each of which fills-or-rejects the hole itself:
///   * a `let` binding annotation (filled from the initializer),
///   * a top-level `throws`-clause member (filled from the inferred throw set;
///     nested `throws` holes ARE firewalled, see `check_throws_wildcard`), and
///   * the expression-context type positions — a call turbofish, an object
///     construction, a generic-apply value, an upcast target — handled during
///     inference (see the expression-context `_` hole policy in
///     the retired TIR builder).
///
/// Emits `WildcardTypeNotAllowed` for every occurrence at any depth.
pub(crate) fn check_wildcard_type(
    type_expr: &mut TypeExpr,
    context: &str,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    match &mut type_expr.kind {
        TypeExprKind::Infer => {
            diags.push(LoweringDiagnostic::WildcardTypeNotAllowed {
                context: context.to_string(),
                span,
            });
            // Neutralize: a hole must never survive into TIR type checking.
            type_expr.kind = TypeExprKind::Error;
        }
        TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
            check_wildcard_type(inner, context, span, diags);
        }
        TypeExprKind::Map { key, value, .. } => {
            check_wildcard_type(key, context, span, diags);
            check_wildcard_type(value, context, span, diags);
        }
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                check_wildcard_type(v, context, span, diags);
            }
        }
        TypeExprKind::Path {
            generic_args,
            associated_type_bindings,
            ..
        } => {
            for arg in generic_args {
                check_wildcard_type(arg, context, span, diags);
            }
            for binding in associated_type_bindings {
                check_wildcard_type(&mut binding.ty, context, span, diags);
            }
        }
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                check_wildcard_type(&mut p.ty, context, span, diags);
            }
            check_wildcard_type(ret, context, span, diags);
            if let Some(throws) = throws {
                check_wildcard_type(throws, context, span, diags);
            }
        }
        TypeExprKind::AssociatedTypeProjection {
            base, interface, ..
        } => {
            check_wildcard_type(base, context, span, diags);
            if let Some(iface) = interface {
                check_wildcard_type(iface, context, span, diags);
            }
        }
        // Primitives and other leaves cannot contain a wildcard.
        _ => {}
    }
}
