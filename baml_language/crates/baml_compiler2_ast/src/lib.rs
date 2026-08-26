//! `baml_compiler2_ast` — Concrete AST structs and CST → AST lowering.
//!
//! This crate isolates all CST messiness in one boundary layer. After
//! `lower_file` returns, the CST is never needed again — all structural
//! content is owned by the returned `Vec<Item>`.
//!
//! No Salsa dependency. Everything downstream works with owned data and
//! can be constructed directly in tests without parsing.

pub mod ast;
pub mod cleanup_guard;
pub(crate) mod companions;
pub(crate) mod disambiguate;
pub mod docstring;
pub(crate) mod lower_cst;
pub(crate) mod lower_expr_body;
pub(crate) mod lower_type_expr;
pub mod lowering_diagnostic;
pub mod traverse;

pub use ast::*;
/// Decode common escape sequences in a quoted string literal body.
///
/// Re-exported from [`baml_base::escape::unescape_string_literal`] so existing
/// callers don't need to change their import path.
pub use baml_base::escape::unescape_string_literal;
pub use disambiguate::{FIELD_ATTR_NAMES, is_field_attr};
pub use docstring::extract_docstring;
pub use lower_cst::{
    lower_file, lower_file_with_path, lower_file_with_path_and_test_owner,
    lower_session_file_with_path_and_test_owner,
};
pub use lower_expr_body::{EnvVarRef, synthesize_spec_stream_body};
pub use lowering_diagnostic::LoweringDiagnostic;
// Re-exported so callers of `TypeExprKind::at(span)` can name the span type
// without depending on `text_size` directly.
pub use text_size::TextRange;
pub use traverse::BodyNode;

/// The BEP-044 `default` receiver keyword. Inside an `implements` block,
/// `default.method(...)` invokes the interface's *default* method body,
/// deliberately bypassing the class's override. It is a **contextual** keyword:
/// the lexer produces an ordinary identifier, and TIR/MIR recognize it by name
/// at the root of a path — so a local named `default` shadows it. This constant
/// is the single source of truth for that spelling; prefer the
/// `is_default_receiver_root` helpers over comparing the literal string.
pub const DEFAULT_RECEIVER_KEYWORD: &str = "default";

/// Parse a quoted string attribute value into its runtime string.
///
/// The input is the raw, still-quoted token text as it appears in
/// [`RawAttributeArg::value`]. Returns `None` if the value is not a recognized
/// string literal. This is the single source of truth for turning an
/// `@alias`/`@description` argument into the value used both by the emitter
/// (for the runtime alias) and by HIR validation (for effective-key collision
/// detection), so the two agree on quote/escape/raw-string normalization.
pub fn parse_string_attr_value(raw: &str) -> Option<String> {
    // Double-quoted string: "text"
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        return Some(unescape_string_literal(&raw[1..raw.len() - 1]));
    }
    // Single-quoted string: 'text'
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Some(unescape_string_literal(&raw[1..raw.len() - 1]));
    }

    None
}

/// Push diagnostics for a failed numeric literal. `InvalidDigits` gets one
/// diagnostic per offending digit with a one-character span (rustc-style);
/// every other error spans the whole token.
fn push_num_lit_error(
    error: baml_base::num_lit::IntLitError,
    token_range: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    use baml_base::num_lit::IntLitError;
    if let IntLitError::InvalidDigits { positions, .. } = &error {
        for (offset, ch) in positions {
            let offset = u32::try_from(*offset).expect("literal token exceeds u32 length");
            let start = token_range.start() + text_size::TextSize::from(offset);
            let span = text_size::TextRange::new(start, start + text_size::TextSize::of(*ch));
            diags.push(LoweringDiagnostic::InvalidNumericLiteral {
                error: error.clone(),
                span,
            });
        }
    } else {
        diags.push(LoweringDiagnostic::InvalidNumericLiteral {
            error,
            span: token_range,
        });
    }
}

/// Lower the raw text of an `INTEGER_LITERAL` token (`42`, `1_000`, `0xFF`,
/// `0o755`, `0b1010`) into its value, emitting diagnostics for invalid
/// literals and returning `0` as the placeholder (compilation already
/// failed). Signs are handled by callers; the VM's i63 `int` range is
/// enforced later in type inference.
pub fn lower_int_literal(
    text: &str,
    token_range: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> i64 {
    match baml_base::num_lit::parse_int_literal(text) {
        Ok(v) => v,
        Err(e) => {
            push_num_lit_error(e, token_range, diags);
            0
        }
    }
}

/// Lower the raw text of a `BIGINT_LITERAL` token (digits plus trailing
/// lowercase `n` suffix, e.g. `42n` or `0xFFn`) into a
/// [`num_bigint::BigInt`], emitting diagnostics for invalid literals and
/// returning `0` as the placeholder. The lexer guarantees the suffix is
/// present, so its absence panics with `unreachable!`.
pub fn lower_bigint_literal(
    text: &str,
    token_range: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> num_bigint::BigInt {
    let digits = text
        .strip_suffix('n')
        .unwrap_or_else(|| unreachable!("BIGINT_LITERAL missing 'n' suffix: {text:?}"));
    match baml_base::num_lit::parse_bigint_literal(digits) {
        Ok(v) => v,
        Err(e) => {
            push_num_lit_error(e, token_range, diags);
            num_bigint::BigInt::from(0)
        }
    }
}

// `unescape_string_literal` lives in `baml_base::escape` and is re-exported
// above. The pre-merge canary copy was dropped here in favor of the shared
// implementation introduced by BEP-049 M1.
#[cfg(test)]
mod tests {
    use baml_base::FileId;
    use baml_compiler_lexer::lex_lossless;
    use baml_compiler_parser::parse_file;
    use baml_compiler_syntax::{SyntaxKind, SyntaxNode};

    use crate::{
        ast::{BuiltinKind, Expr, FunctionBodyDef, Item, Stmt, TypeExpr, TypeExprKind},
        lower_cst::lower_file,
        unescape_string_literal,
    };

    #[test]
    fn unescape_string_literal_decodes_supported_escapes() {
        assert_eq!(unescape_string_literal(r"line\nbreak"), "line\nbreak");
        assert_eq!(unescape_string_literal(r"tab\there"), "tab\there");
        assert_eq!(unescape_string_literal(r"cr\rhere"), "cr\rhere");
        assert_eq!(unescape_string_literal(r"nul\0here"), "nul\0here");
        assert_eq!(unescape_string_literal(r"back\\slash"), "back\\slash");
        assert_eq!(unescape_string_literal(r#"a\"b"#), "a\"b");
    }

    #[test]
    fn unescape_string_literal_preserves_unknown_sequences() {
        assert_eq!(unescape_string_literal(r"\x41"), "\\x41");
        assert_eq!(unescape_string_literal(r"\u0041"), "\\u0041");
    }

    #[test]
    fn unescape_string_literal_preserves_trailing_backslash() {
        assert_eq!(unescape_string_literal("trailing\\"), "trailing\\");
    }

    #[test]
    fn unescape_string_literal_handles_empty_and_plain_text() {
        assert_eq!(unescape_string_literal(""), "");
        assert_eq!(unescape_string_literal("plain text"), "plain text");
    }

    /// Build a `TypeExpr` value for use in `assert_eq!` comparisons.
    /// All spans are zeroed. Attrs go inside the variant constructor:
    ///
    /// ```ignore
    /// type_expr!(Path("Foo", Attr("stream.done")))
    /// type_expr!(WithAttrs((List(String)), Attr("stream.done")))
    /// type_expr!(Union((Path("A")), (Path("B", Attr("stream.done")))))
    /// ```
    macro_rules! type_expr {
        // ── Helper: build attr vec from Attr("name") args ──
        (@attrs) => { vec![] };
        (@attrs $(, Attr($attr_name:expr))+) => {
            vec![$(crate::ast::RawAttribute {
                name: baml_base::Name::new($attr_name),
                args: vec![],
                span: text_size::TextRange::default(),
            }),+]
        };

        // ── Leaves ──
        (Int $(, Attr($a:expr))*) => { TypeExprKind::Int { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Bigint $(, Attr($a:expr))*) => { TypeExprKind::Bigint { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Float $(, Attr($a:expr))*) => { TypeExprKind::Float { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (String $(, Attr($a:expr))*) => { TypeExprKind::String { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Bool $(, Attr($a:expr))*) => { TypeExprKind::Bool { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Null $(, Attr($a:expr))*) => { TypeExprKind::Null { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Never $(, Attr($a:expr))*) => { TypeExprKind::Never { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };
        (Rust $(, Attr($a:expr))*) => { TypeExprKind::Rust { attrs: type_expr!(@attrs $(, Attr($a))*) }.at(text_size::TextRange::default()) };

        // ── Path ──
        (Path($name:expr $(, Attr($a:expr))*)) => {
            TypeExprKind::Path {
                segments: vec![baml_base::Name::new($name)],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: type_expr!(@attrs $(, Attr($a))*),
            }
            .at(text_size::TextRange::default())
        };

        // ── Containers ──
        (Optional($($inner:tt)+)) => {
            TypeExprKind::Optional {
                inner: Box::new(type_expr!($($inner)+)),
                attrs: vec![],
            }
            .at(text_size::TextRange::default())
        };
        (List($($inner:tt)+)) => {
            TypeExprKind::List {
                inner: Box::new(type_expr!($($inner)+)),
                attrs: vec![],
            }
            .at(text_size::TextRange::default())
        };

        // ── Union: each variant is wrapped in parens ──
        (Union($(($($variant:tt)+)),+ $(,)?)) => {
            TypeExprKind::Union {
                variants: vec![$(type_expr!(($($variant)+))),+],
                attrs: vec![],
            }
            .at(text_size::TextRange::default())
        };

        // ── Attach attrs to any type: WithAttrs((List(String)), Attr("stream.done")) ──
        (WithAttrs(($($inner:tt)+), $(Attr($a:expr)),+)) => {{
            let mut te = type_expr!($($inner)+);
            *te.attrs_mut() = type_expr!(@attrs $(, Attr($a))+);
            te
        }};

        // ── Paren passthrough: ((Int)) → type_expr!(Int) ──
        (($($inner:tt)+)) => {
            type_expr!($($inner)+)
        };
    }

    /// Strip all `TextRange` spans from a `TypeExpr` tree (recursively),
    /// replacing them with `TextRange::default()`. This allows `assert_eq!`
    /// comparison against hand-built expected values.
    fn strip_spans(expr: &TypeExpr) -> TypeExpr {
        fn strip_attr(attr: &crate::ast::RawAttribute) -> crate::ast::RawAttribute {
            crate::ast::RawAttribute {
                name: attr.name.clone(),
                args: attr
                    .args
                    .iter()
                    .map(|a| crate::ast::RawAttributeArg {
                        key: a.key.clone(),
                        value: a.value.clone(),
                        span: text_size::TextRange::default(),
                    })
                    .collect(),
                span: text_size::TextRange::default(),
            }
        }

        fn strip_attrs(attrs: &[crate::ast::RawAttribute]) -> Vec<crate::ast::RawAttribute> {
            attrs.iter().map(strip_attr).collect()
        }

        let __stripped = match &expr.kind {
            TypeExprKind::Int { attrs } => TypeExprKind::Int {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Bigint { attrs } => TypeExprKind::Bigint {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Float { attrs } => TypeExprKind::Float {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::String { attrs } => TypeExprKind::String {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Bool { attrs } => TypeExprKind::Bool {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Null { attrs } => TypeExprKind::Null {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Uint8Array { attrs } => TypeExprKind::Uint8Array {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Never { attrs } => TypeExprKind::Never {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Void { attrs } => TypeExprKind::Void {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Rust { attrs } => TypeExprKind::Rust {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
                attrs,
            } => TypeExprKind::Path {
                segments: segments.clone(),
                generic_args: generic_args.iter().map(strip_spans).collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| crate::ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(strip_spans(&binding.ty)),
                    })
                    .collect(),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::AssociatedTypeProjection {
                base,
                interface,
                member,
                attrs,
            } => TypeExprKind::AssociatedTypeProjection {
                base: Box::new(strip_spans(base)),
                interface: interface
                    .as_ref()
                    .map(|interface| Box::new(strip_spans(interface))),
                member: member.clone(),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
                inner: Box::new(strip_spans(inner)),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::List { inner, attrs } => TypeExprKind::List {
                inner: Box::new(strip_spans(inner)),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Map { key, value, attrs } => TypeExprKind::Map {
                key: Box::new(strip_spans(key)),
                value: Box::new(strip_spans(value)),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
                variants: variants.iter().map(strip_spans).collect(),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Literal { value, attrs } => TypeExprKind::Literal {
                value: value.clone(),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Function {
                params,
                ret,
                throws,
                attrs,
            } => TypeExprKind::Function {
                params: params
                    .iter()
                    .map(|p| crate::ast::FunctionTypeParam {
                        name: p.name.clone(),
                        optional: p.optional,
                        ty: strip_spans(&p.ty),
                    })
                    .collect(),
                ret: Box::new(strip_spans(ret)),
                throws: throws.as_ref().map(|throws| Box::new(strip_spans(throws))),
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Media { kind, attrs } => TypeExprKind::Media {
                kind: *kind,
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::BuiltinUnknown { attrs } => TypeExprKind::BuiltinUnknown {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Type { attrs } => TypeExprKind::Type {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Error { attrs } => TypeExprKind::Error {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Unknown { attrs } => TypeExprKind::Unknown {
                attrs: strip_attrs(attrs),
            },
            TypeExprKind::Infer { attrs } => TypeExprKind::Infer {
                attrs: strip_attrs(attrs),
            },
        };
        __stripped.at(text_size::TextRange::default())
    }

    /// Parse BAML source text and return the CST root.
    fn parse(source: &str) -> SyntaxNode {
        let tokens = lex_lossless(source, FileId::new(0));
        let (green, errors) = parse_file(&tokens);
        assert!(
            errors.is_empty(),
            "expected no parse errors, got: {errors:#?}"
        );
        SyntaxNode::new_root(green)
    }

    /// Parse BAML source and lower to AST items.
    pub(super) fn parse_and_lower(source: &str) -> Vec<Item> {
        let root = parse(source);
        let (items, diags, _env_var_refs) = lower_file(&root);
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:#?}");
        items
    }

    fn parse_and_lower_with_diagnostics(
        source: &str,
    ) -> (Vec<Item>, Vec<crate::LoweringDiagnostic>) {
        let root = parse(source);
        let (items, diags, _env_var_refs) = lower_file(&root);
        (items, diags)
    }

    /// Generic parameters rendered back to source form (`T extends A & B`), so
    /// one assertion covers both the names and the full conjunction of bounds.
    fn rendered_generic_params(params: &[crate::ast::GenericParam]) -> Vec<String> {
        params
            .iter()
            .map(|param| {
                if param.bounds.is_empty() {
                    return param.name.as_str().to_string();
                }
                let bounds: Vec<String> = param.bounds.iter().map(ToString::to_string).collect();
                format!("{} extends {}", param.name.as_str(), bounds.join(" & "))
            })
            .collect()
    }

    fn first_function(items: Vec<Item>) -> crate::ast::FunctionDef {
        items
            .into_iter()
            .find_map(|item| {
                if let Item::Function(f) = item {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("expected a FunctionDef")
    }

    #[test]
    fn call_unreflect_bare_path_keeps_its_operand() {
        let function = first_function(parse_and_lower(
            "function main(t: reflect.Type) -> reflect.Type { return reflect.Type.of<unreflect(t)>() }",
        ));
        let Some(crate::ast::FunctionBodyDef::Expr(body, _)) = function.body else {
            panic!("expected expression body")
        };
        let operand = body
            .exprs
            .iter()
            .find_map(|(_, expr)| match expr {
                Expr::Call { type_args, .. } => type_args.iter().find_map(|arg| match arg {
                    crate::ast::TypeArg::Unreflect(operand) => Some(*operand),
                    crate::ast::TypeArg::Static(_) => None,
                }),
                _ => None,
            })
            .expect("expected unreflect type argument");
        assert!(
            matches!(&body.exprs[operand], Expr::Path(path) if path.len() == 1 && path[0].as_str() == "t"),
            "unreflect operand lowered as {:?}",
            body.exprs[operand]
        );
    }

    #[test]
    fn llm_function_user_client_param_is_reserved() {
        let source = r##"
function Extract(client: string, text: string) -> string {
  client: "openai/gpt-4o"
  prompt: `${text} ${ctx.output_format()}`
}
"##;

        let (items, diags) = parse_and_lower_with_diagnostics(source);
        assert!(
            diags.iter().any(|diag| matches!(
                diag,
                crate::LoweringDiagnostic::ReservedLlmParam {
                    function_name,
                    param_name,
                    ..
                } if function_name == "Extract" && param_name == "client"
            )),
            "expected reserved LLM client param diagnostic, got: {diags:#?}"
        );

        let function = items
            .into_iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name.as_str() == "Extract" => Some(function),
                _ => None,
            })
            .expect("expected Extract function");
        let param_names: Vec<&str> = function.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(param_names, vec!["text", "client", "on_event"]);
        for injected in [&function.params[1], &function.params[2]] {
            let default_id = injected
                .default
                .unwrap_or_else(|| panic!("expected {} default", injected.name.as_str()));
            let default_expr = &function.defaults.exprs.exprs[default_id.expr()];
            assert!(
                matches!(default_expr, Expr::Null),
                "the injected {} override defaults to null, got {default_expr:#?}",
                injected.name.as_str()
            );
        }
    }

    #[test]
    fn client_block_is_a_migration_error() {
        let source = r#"
client<llm> C {
  provider openai
  options { model "gpt-4o" }
}
"#;
        let (items, diags) = parse_and_lower_with_diagnostics(source);
        assert!(items.is_empty(), "client blocks lower to no items");
        assert!(
            diags.iter().any(|diag| matches!(
                diag,
                crate::LoweringDiagnostic::ClientBlockRemoved { name, .. } if name == "C"
            )),
            "expected ClientBlockRemoved, got: {diags:#?}"
        );
    }

    #[test]
    fn client_value_decl_lowers_to_client_let() {
        use crate::ast::LetOrigin;
        let source = r#"
client Fast = openai.ResponsesClient.new(model = "gpt-4o-mini");
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1, "expected exactly one item");
        let Item::Let(let_def) = &items[0] else {
            panic!("expected Item::Let, got {:?}", items[0]);
        };
        assert_eq!(let_def.name.as_str(), "Fast");
        assert_eq!(let_def.origin, LetOrigin::Client);
        let (body, _) = let_def.initializer.as_ref().expect("expected initializer");
        let root = body.root_expr.expect("expected root expr");
        assert!(
            matches!(&body.exprs[root], Expr::Call { .. }),
            "initializer is the user's constructor call"
        );
    }

    #[test]
    fn ast_function_def_has_generic_params() {
        let source = r#"
function deep_copy<T>(value: T) -> T {
  $rust_function
}
"#;
        let function = first_function(parse_and_lower(source));

        assert_eq!(function.generic_params.len(), 1);
        assert_eq!(function.generic_params[0].name.as_str(), "T");
    }

    #[test]
    fn ast_preserves_parameter_defaults_and_call_labels() {
        let source = r#"
function Search(query: string, max_results: int = 10) -> int {
  Search(query = "cats", max_results = 5)
}
"#;
        let function = first_function(parse_and_lower(source));

        assert!(function.params[0].default.is_none());
        let default_id = function.params[1]
            .default
            .expect("expected default expression id");
        assert!(matches!(
            function.defaults.expr(default_id),
            Expr::Literal(_)
        ));

        let Some(FunctionBodyDef::Expr(body, _source_map)) = &function.body else {
            panic!("expected expression body");
        };
        let call_id = body.root_expr.expect("expected body root expression");
        let Expr::Block {
            tail_expr: Some(tail),
            ..
        } = &body.exprs[call_id]
        else {
            panic!("expected block root");
        };
        let Expr::Call { args, .. } = &body.exprs[*tail] else {
            panic!("expected call tail expression");
        };

        assert_eq!(
            args[0].label.as_ref().map(smol_str::SmolStr::as_str),
            Some("query")
        );
        assert_eq!(
            args[1].label.as_ref().map(smol_str::SmolStr::as_str),
            Some("max_results")
        );
    }

    #[test]
    fn ast_tagged_template_body_marked_synthetic() {
        // The desugared closure body of a tagged template is compiler-generated,
        // so every node it allocates is recorded in the source map's synthetic
        // sets — while the user-written tag expr and `${…}` interp expressions
        // (reused from `segments`) stay non-synthetic. This is what lets inlay
        // hints skip the `__tt_*` accumulators / `.push(...)` calls robustly,
        // independent of their (incidental) empty spans / type annotations.
        use crate::ast::{TemplateSegment, TemplateTag};
        let source = r#"
function Demo(items: string[]) -> string {
  sql`a ${1} ${for (let x in items)}${x},${endfor}`
}
"#;
        let function = first_function(parse_and_lower(source));
        let Some(FunctionBodyDef::Expr(body, source_map)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("expected body root expression");
        let Expr::Block {
            tail_expr: Some(tail),
            ..
        } = &body.exprs[root]
        else {
            panic!("expected block root");
        };
        let Expr::Template {
            tag: TemplateTag::Custom { tag, body: tbody },
            segments,
        } = &body.exprs[*tail]
        else {
            panic!("expected Custom Template tail");
        };

        // The elaborated closure body block and all of its statements are synthetic.
        assert!(
            source_map.is_synthetic_expr(*tbody),
            "tagged-template body block should be marked synthetic"
        );
        let Expr::Block { stmts, .. } = &body.exprs[*tbody] else {
            panic!("tagged body should be a block");
        };
        assert!(
            !stmts.is_empty() && stmts.iter().all(|s| source_map.is_synthetic_stmt(*s)),
            "every statement in the tagged-template body should be marked synthetic"
        );

        // The tag expr and the user's `${…}` interp expression stay non-synthetic.
        assert!(
            !source_map.is_synthetic_expr(*tag),
            "user-written tag expr must not be marked synthetic"
        );
        let interp = segments
            .iter()
            .find_map(|s| match s {
                TemplateSegment::Interp(e) => Some(*e),
                _ => None,
            })
            .expect("expected a top-level ${…} interp segment");
        assert!(
            !source_map.is_synthetic_expr(interp),
            "user ${{…}} interpolation expression must stay non-synthetic"
        );
    }

    #[test]
    fn ast_tagged_template_lowers_to_template_expr() {
        // BEP-049 §10. `tag`...`` lowers to a first-class `Expr::Template`
        // with `TemplateTag::Custom`, PRESERVING segment structure (text /
        // interp / for / if). The tag itself lowers as an ordinary expression
        // (here the bare path `sql`).
        use crate::ast::{TemplateIfBranch, TemplateSegment, TemplateTag};
        let source = r#"
function Demo(items: string[]) -> string {
  sql`a ${1} ${for (let x in items)}${x},${endfor}${if (true)}w${else}e${endif}`
}
"#;
        let function = first_function(parse_and_lower(source));
        let Some(FunctionBodyDef::Expr(body, _source_map)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("expected body root expression");
        let Expr::Block {
            tail_expr: Some(tail),
            ..
        } = &body.exprs[root]
        else {
            panic!("expected block root, got {:?}", body.exprs[root]);
        };
        let Expr::Template { tag, segments } = &body.exprs[*tail] else {
            panic!("expected Template tail, got {:?}", body.exprs[*tail]);
        };

        // A tagged template carries `TemplateTag::Custom`, whose tag expr
        // lowers to the bare path `sql` (TIR validates it resolves to a
        // `//baml:tagged_string` fn; lowering only handles it structurally).
        let TemplateTag::Custom { tag, .. } = tag else {
            panic!("expected Custom tag, got {tag:?}");
        };
        assert!(
            matches!(&body.exprs[*tag], Expr::Path(p) if p.len() == 1 && p[0].as_str() == "sql"),
            "tag should lower to Path([sql]), got {:?}",
            body.exprs[*tag]
        );

        // Top-level segments include a leading Text, an Interp, a For block
        // and an If chain (whitespace Text segments interleave them).
        assert!(
            matches!(segments.first(), Some(TemplateSegment::Text(_))),
            "first segment should be literal text, got {:?}",
            segments.first()
        );
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, TemplateSegment::Interp(_))),
            "expected a top-level Interp segment"
        );

        let TemplateSegment::For { body: for_body, .. } = segments
            .iter()
            .find(|s| matches!(s, TemplateSegment::For { .. }))
            .expect("expected a For segment")
        else {
            unreachable!()
        };
        assert!(
            for_body
                .iter()
                .any(|s| matches!(s, TemplateSegment::Interp(_))),
            "for body should contain the ${{x}} interpolation, got {for_body:?}"
        );

        let TemplateSegment::If {
            branches,
            else_body,
        } = segments
            .iter()
            .find(|s| matches!(s, TemplateSegment::If { .. }))
            .expect("expected an If segment")
        else {
            unreachable!()
        };
        assert_eq!(branches.len(), 1, "expected a single if-branch");
        let TemplateIfBranch {
            body: then_body, ..
        } = &branches[0];
        assert!(
            then_body
                .iter()
                .any(|s| matches!(s, TemplateSegment::Text(_))),
            "then-branch should contain text"
        );
        assert!(else_body.is_some(), "if should carry an else body");
    }

    #[test]
    fn ast_does_not_treat_upcast_target_as_method_type_args() {
        let source = r#"
function main(m: MultiFormat) -> string {
  return m.as<Converter<int>>.convert()
}
"#;
        let function = first_function(parse_and_lower(source));
        let Some(FunctionBodyDef::Expr(body, _source_map)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("expected body root expression");
        let Expr::Block { stmts, .. } = &body.exprs[root] else {
            panic!("expected block root expression");
        };
        let return_expr = match &body.stmts[stmts[0]] {
            Stmt::Return(Some(expr_id)) => *expr_id,
            other => panic!("expected return statement, got {other:?}"),
        };
        let Expr::Call {
            callee, type_args, ..
        } = &body.exprs[return_expr]
        else {
            panic!("expected return expression to be a call");
        };
        assert!(
            type_args.is_empty(),
            ".as<Interface> target must not be lowered as method type args"
        );

        let Expr::MemberAccess { base, member } = &body.exprs[*callee] else {
            panic!("expected call callee to be member access");
        };
        assert_eq!(member.as_str(), "convert");
        let Expr::Upcast { target, .. } = &body.exprs[*base] else {
            panic!("expected member receiver to be an upcast expression");
        };
        assert_eq!(
            target.to_string(),
            "Converter<int>",
            "upcast target itself should still be preserved"
        );
    }

    #[test]
    fn ast_preserves_out_of_body_implements_for_external_class_target() {
        let source = r#"
interface ToJson {
  function to_json(self) -> string
}

implements ToJson for Dog {
  function to_json(self) -> string {
    return "dog"
  }
}
"#;
        let items = parse_and_lower(source);
        let imp = items
            .iter()
            .find_map(|item| match item {
                Item::ImplementsFor(imp) => Some(imp),
                _ => None,
            })
            .expect("external class target should remain an ImplementsFor item");

        assert_eq!(imp.interface_target.to_string(), "ToJson");
        assert_eq!(imp.for_target.to_string(), "Dog");
    }

    #[test]
    fn ast_does_not_merge_qualified_out_of_body_implements_into_local_class() {
        let source = r#"
interface ToJson {
  function to_json(self) -> string
}

class Dog {
  name string
}

implements ToJson for other.Dog {
  function to_json(self) -> string {
    return "dog"
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .iter()
            .find_map(|item| match item {
                Item::Class(class) if class.name.as_str() == "Dog" => Some(class),
                _ => None,
            })
            .expect("expected local Dog class");
        assert!(
            class.implements.is_empty(),
            "qualified target other.Dog must not merge into local Dog"
        );

        let imp = items
            .iter()
            .find_map(|item| match item {
                Item::ImplementsFor(imp) => Some(imp),
                _ => None,
            })
            .expect("qualified target should remain an ImplementsFor item");
        assert_eq!(imp.for_target.to_string(), "other.Dog");
    }

    #[test]
    fn ast_does_not_merge_generic_out_of_body_implements_into_local_class() {
        let source = r#"
interface ToJson {
  function to_json(self) -> string
}

class Dog<T> {
  value T
}

implements ToJson for Dog<int> {
  function to_json(self) -> string {
    return "dog"
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .iter()
            .find_map(|item| match item {
                Item::Class(class) if class.name.as_str() == "Dog" => Some(class),
                _ => None,
            })
            .expect("expected local Dog class");
        assert!(
            class.implements.is_empty(),
            "generic target Dog<int> must not merge into generic class Dog<T>"
        );

        let imp = items
            .iter()
            .find_map(|item| match item {
                Item::ImplementsFor(imp) => Some(imp),
                _ => None,
            })
            .expect("generic target should remain an ImplementsFor item");
        assert_eq!(imp.for_target.to_string(), "Dog<int>");
    }

    #[test]
    fn ast_preserves_interface_generic_param_bounds() {
        let source = r#"
interface Named {
  name string
}

interface Box<T extends Named, E> {
  value T
}
"#;
        let items = parse_and_lower(source);
        let interface = items
            .iter()
            .find_map(|item| match item {
                Item::Interface(interface) if interface.name.as_str() == "Box" => Some(interface),
                _ => None,
            })
            .expect("expected Box interface");

        assert_eq!(
            rendered_generic_params(&interface.generic_params),
            vec!["T extends Named", "E"]
        );
    }

    /// `T extends A & B` is a conjunction — every bound must survive lowering.
    #[test]
    fn ast_preserves_every_bound_in_a_generic_param_intersection() {
        let source = r#"
interface Named {
  name string
}

interface Sized {
  size int
}

interface Box<T extends Named & Sized, E> {
  value T
}

function pack<U extends Named & Sized>(value: U) -> U {
  value
}
"#;
        let items = parse_and_lower(source);
        let interface = items
            .iter()
            .find_map(|item| match item {
                Item::Interface(interface) if interface.name.as_str() == "Box" => Some(interface),
                _ => None,
            })
            .expect("expected Box interface");
        assert_eq!(
            rendered_generic_params(&interface.generic_params),
            vec!["T extends Named & Sized", "E"]
        );

        let function = items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name.as_str() == "pack" => Some(function),
                _ => None,
            })
            .expect("expected pack function");
        assert_eq!(
            rendered_generic_params(&function.generic_params),
            vec!["U extends Named & Sized"]
        );
    }

    #[test]
    fn ast_preserves_required_interface_method_generic_param_bounds() {
        let source = r#"
interface Named {
  name string
}

interface Mapper {
  function map<T extends Named, E>(self, value: T, extra: E) -> T
}
"#;
        let items = parse_and_lower(source);
        let interface = items
            .iter()
            .find_map(|item| match item {
                Item::Interface(interface) if interface.name.as_str() == "Mapper" => {
                    Some(interface)
                }
                _ => None,
            })
            .expect("expected Mapper interface");
        let method = interface
            .required_methods
            .first()
            .expect("expected required method");

        assert_eq!(
            rendered_generic_params(&method.generic_params),
            vec!["T extends Named", "E"]
        );
    }

    #[test]
    fn ast_default_indices_survive_recovered_parameter() {
        let source = r#"
function Broken(: int = 1, value: int = 2) -> int {
  value
}
"#;
        let root = {
            let tokens = lex_lossless(source, FileId::new(0));
            let (green, _errors) = parse_file(&tokens);
            SyntaxNode::new_root(green)
        };
        let (items, _diags, _env_var_refs) = lower_file(&root);
        let function = first_function(items);

        assert_eq!(function.params.len(), 1);
        assert_eq!(function.params[0].name.as_str(), "value");
        let default_id = function.params[0]
            .default
            .expect("expected valid param default to survive recovery");
        assert!(matches!(
            function.defaults.expr(default_id),
            Expr::Literal(_)
        ));
    }

    #[test]
    fn constructorless_recovered_object_lowers_to_missing() {
        let source = r#"
function Broken() -> int {
  (1)<int> { x: 2 }
}
"#;
        let tokens = lex_lossless(source, FileId::new(0));
        let (green, _errors) = parse_file(&tokens);

        let root = SyntaxNode::new_root(green);
        assert!(
            root.descendants()
                .any(|node| node.kind() == baml_compiler_syntax::SyntaxKind::OBJECT_LITERAL),
            "this regression must exercise the parser's identifier-less object CST"
        );
        let (items, diags, _env_var_refs) = lower_file(&root);
        let function = first_function(items);
        let Some(crate::ast::FunctionBodyDef::Expr(body, _)) = function.body else {
            panic!("expected expression body")
        };

        assert!(
            body.exprs
                .iter()
                .any(|(_, expr)| matches!(expr, Expr::Missing)),
            "constructor-less recovery must lower to Expr::Missing"
        );
        assert!(
            diags.iter().any(|diag| matches!(
                diag,
                crate::LoweringDiagnostic::MissingObjectConstructor { .. }
            )),
            "the recovery must remain visible as a lowering diagnostic"
        );
    }

    #[test]
    fn removed_hash_string_does_not_lower_to_a_string_literal() {
        let source = r##"
function legacy() -> string {
  #"value"#
}
"##;
        let tokens = lex_lossless(source, FileId::new(0));
        let (green, errors) = parse_file(&tokens);
        assert!(!errors.is_empty(), "removed hash strings must fail parsing");

        let root = SyntaxNode::new_root(green);
        assert!(
            root.descendants()
                .any(|node| node.kind() == SyntaxKind::RAW_STRING_LITERAL),
            "the parser must retain the hash string CST for error recovery"
        );
        let (items, _diags, _env_var_refs) = lower_file(&root);
        let function = first_function(items);
        let Some(FunctionBodyDef::Expr(body, _)) = function.body else {
            panic!("expected expression body")
        };

        assert!(
            body.exprs
                .iter()
                .any(|(_, expr)| matches!(expr, Expr::Missing)),
            "removed hash strings must lower only to Expr::Missing"
        );
        assert!(
            !body.exprs.iter().any(
                |(_, expr)| matches!(expr, Expr::Literal(crate::ast::Literal::String(value)) if value == "value")
            ),
            "removed hash strings must never become semantic string literals"
        );
    }

    #[test]
    fn ast_default_indices_skip_missing_name_slots() {
        let source = r#"
function Broken(: int, b: string = "x") -> string {
  b
}
"#;
        let root = {
            let tokens = lex_lossless(source, FileId::new(0));
            let (green, _errors) = parse_file(&tokens);
            SyntaxNode::new_root(green)
        };
        let (items, diags, _env_var_refs) = lower_file(&root);
        let function = first_function(items);

        assert!(
            diags
                .iter()
                .any(|diag| matches!(diag, crate::LoweringDiagnostic::MissingParamName { .. })),
            "lower_param should report the recovered missing name"
        );
        assert_eq!(
            function.params.len(),
            1,
            "lower_params_with_defaults should filter out the missing-name slot"
        );
        assert_eq!(function.params[0].name.as_str(), "b");
        assert_eq!(
            function.defaults.exprs.exprs.len(),
            1,
            "lower_expr_body::lower_default_expr_nodes should only lower b's default"
        );

        let default_id = function.params[0]
            .default
            .expect("expected b's default to use the lowered params index");
        assert_eq!(
            function.defaults.expr(default_id),
            &Expr::Literal(crate::ast::Literal::String("x".to_string()))
        );
    }

    #[test]
    fn ast_lowers_method_block_attributes() {
        let source = r#"
class Response {
  @@internal.uses(engine_ctx)
  function text(self) -> string throws baml.errors.Io {
    $rust_io_function
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| match item {
                Item::Class(class) => Some(class),
                _ => None,
            })
            .expect("expected ClassDef");
        let method = class.methods.first().expect("expected method");

        assert_eq!(method.attributes.len(), 1);
        assert_eq!(method.attributes[0].name.as_str(), "internal.uses");
        assert_eq!(method.attributes[0].args.len(), 1);
        assert_eq!(method.attributes[0].args[0].value, "engine_ctx");
        let throws = method.throws.as_ref().expect("expected throws contract");
        assert_eq!(
            throws.kind,
            TypeExprKind::Path {
                segments: vec![
                    baml_base::Name::new("baml"),
                    baml_base::Name::new("errors"),
                    baml_base::Name::new("Io"),
                ],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![]
            }
        );
    }

    #[test]
    fn ast_lowers_keyword_named_class_field() {
        let source = r#"
class InterfaceTwo {
  interface string
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| match item {
                Item::Class(class) => Some(class),
                _ => None,
            })
            .expect("expected ClassDef");

        let field = class.fields.first().expect("expected field");
        assert_eq!(field.name.as_str(), "interface");
        assert_eq!(field.type_expr.to_string(), "string");
    }

    #[test]
    fn ast_lowers_keyword_named_class_method_and_member_call() {
        let source = r#"
class TypeValue {
  function implements(self) -> string {
    "ok"
  }
}

function Foo(t: TypeValue) -> string {
  t.implements()
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .iter()
            .find_map(|item| match item {
                Item::Class(class) => Some(class),
                _ => None,
            })
            .expect("expected ClassDef");
        assert!(
            !class.methods.is_empty(),
            "expected keyword-named method, got class {class:#?}"
        );
        assert_eq!(class.methods[0].name.as_str(), "implements");

        let function = items
            .into_iter()
            .find_map(|item| match item {
                Item::Function(function) => Some(function),
                _ => None,
            })
            .expect("expected FunctionDef");
        let Some(FunctionBodyDef::Expr(body, _source_map)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("expected body root expression");
        let Expr::Block { tail_expr, .. } = &body.exprs[root] else {
            panic!("expected block root expression");
        };
        let tail_expr = tail_expr.expect("expected tail expression");
        let Expr::Call { callee, .. } = &body.exprs[tail_expr] else {
            panic!("expected call expression");
        };
        let Expr::MemberAccess { member, .. } = &body.exprs[*callee] else {
            panic!("expected member access callee");
        };
        assert_eq!(member.as_str(), "implements");
    }

    #[test]
    fn ast_lowers_required_interface_method_throws() {
        let source = r#"
interface Response {
  function text(self) -> string throws baml.errors.Io
}
"#;
        let items = parse_and_lower(source);
        let interface = items
            .into_iter()
            .find_map(|item| match item {
                Item::Interface(interface) => Some(interface),
                _ => None,
            })
            .expect("expected InterfaceDef");
        let method = interface
            .required_methods
            .first()
            .expect("expected required method");

        let throws = method.throws.as_ref().expect("expected throws contract");
        assert_eq!(
            throws.kind,
            TypeExprKind::Path {
                segments: vec![
                    baml_base::Name::new("baml"),
                    baml_base::Name::new("errors"),
                    baml_base::Name::new("Io"),
                ],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![]
            }
        );
    }

    // ── 4.1/4.2: Parser produces GENERIC_PARAM_LIST / GENERIC_PARAM CST nodes ──

    #[test]
    fn parser_produces_generic_param_list_for_class_with_single_type_param() {
        let source = r#"
class Array<T> {
  function at(self, index: int) -> T {
    $rust_function
  }
}
"#;
        let root = parse(source);

        // Verify GENERIC_PARAM_LIST node exists in the tree
        let param_list = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::GENERIC_PARAM_LIST)
            .expect("expected GENERIC_PARAM_LIST node");

        // Verify it contains exactly one GENERIC_PARAM child
        let params: Vec<_> = param_list
            .children()
            .filter(|n| n.kind() == SyntaxKind::GENERIC_PARAM)
            .collect();
        assert_eq!(params.len(), 1, "expected one GENERIC_PARAM");

        // Verify the param name is "T"
        let param_name = params[0]
            .children_with_tokens()
            .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD)
            .expect("expected WORD token in GENERIC_PARAM")
            .text()
            .to_string();
        assert_eq!(param_name, "T");
    }

    #[test]
    fn parser_produces_two_generic_params_for_map_class() {
        let source = r#"
class Map<K, V> {
  function has(self, key: K) -> bool {
    $rust_function
  }
}
"#;
        let root = parse(source);

        let param_list = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::GENERIC_PARAM_LIST)
            .expect("expected GENERIC_PARAM_LIST node");

        let params: Vec<_> = param_list
            .children()
            .filter(|n| n.kind() == SyntaxKind::GENERIC_PARAM)
            .collect();
        assert_eq!(params.len(), 2, "expected two GENERIC_PARAM nodes");

        let names: Vec<String> = params
            .iter()
            .map(|p| {
                p.children_with_tokens()
                    .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::WORD)
                    .expect("expected WORD token")
                    .text()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["K", "V"]);
    }

    #[test]
    fn parser_does_not_produce_generic_param_list_for_non_generic_class() {
        let source = r#"
class User {
  name string
}
"#;
        let root = parse(source);

        let param_list = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::GENERIC_PARAM_LIST);
        assert!(
            param_list.is_none(),
            "expected no GENERIC_PARAM_LIST for non-generic class"
        );
    }

    // ── 4.3: AST ClassDef.generic_params is populated from CST ───────────────

    #[test]
    fn ast_class_def_has_one_generic_param() {
        let source = r#"
class Array<T> {
  function at(self, index: int) -> T {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef");

        assert_eq!(class.generic_params.len(), 1);
        assert_eq!(class.generic_params[0].name.as_str(), "T");
    }

    #[test]
    fn ast_class_def_has_two_generic_params() {
        let source = r#"
class Map<K, V> {
  function has(self, key: K) -> bool {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef");

        assert_eq!(class.generic_params.len(), 2);
        assert_eq!(class.generic_params[0].name.as_str(), "K");
        assert_eq!(class.generic_params[1].name.as_str(), "V");
    }

    #[test]
    fn ast_class_def_has_empty_generic_params_for_non_generic_class() {
        let source = r#"
class User {
  name string
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef");

        assert!(class.generic_params.is_empty());
    }

    // ── 4.4: FunctionBodyDef::Builtin is produced for $rust_function ─────────

    #[test]
    fn function_body_rust_function_produces_builtin_vm() {
        let source = r#"
class Array<T> {
  function at(self, index: int) -> T {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef");

        let method = class.methods.first().expect("expected a method");
        match &method.body {
            Some(FunctionBodyDef::Builtin(BuiltinKind::Vm)) => {}
            other => panic!("expected FunctionBodyDef::Builtin(Vm), got {other:?}"),
        }
    }

    #[test]
    fn function_body_rust_io_function_produces_builtin_io() {
        let source = r#"
function get(key: string) -> string? {
  $rust_io_function
}
"#;
        let items = parse_and_lower(source);
        let func = items
            .into_iter()
            .find_map(|item| {
                if let Item::Function(f) = item {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("expected a FunctionDef");

        match &func.body {
            Some(FunctionBodyDef::Builtin(BuiltinKind::Io)) => {}
            other => panic!("expected FunctionBodyDef::Builtin(Io), got {other:?}"),
        }
    }

    #[test]
    fn regular_expr_body_is_not_builtin() {
        let source = r#"
function add(a: int, b: int) -> int {
  a + b
}
"#;
        let items = parse_and_lower(source);
        let func = items
            .into_iter()
            .find_map(|item| {
                if let Item::Function(f) = item {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("expected a FunctionDef");

        match &func.body {
            Some(FunctionBodyDef::Expr(_, _)) => {}
            other => panic!("expected FunctionBodyDef::Expr, got {other:?}"),
        }
    }

    // ── 4.5: TypeExprKind::Rust is produced for $rust_type field type ────────────

    #[test]
    fn field_with_rust_type_produces_type_expr_rust() {
        let source = r#"
class Media {
  _data $rust_type
}
"#;
        let items = parse_and_lower(source);
        let class = items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef");

        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "_data")
            .expect("expected _data field");

        match &field.type_expr.kind {
            TypeExprKind::Rust { .. } => {}
            other => panic!("expected TypeExprKind::Rust, got {other:?}"),
        }
    }

    // ── Roundtrip: parse representative stub content without panics ───────────

    #[test]
    fn roundtrip_no_panic_on_array_stub() {
        // Use explicit return types to avoid parser errors on void functions.
        // The stub content uses -> void for methods with no return value.
        let source = r#"
class Array<T> {
  function length(self) -> int {
    $rust_function
  }

  function at(self, index: int) -> T {
    $rust_function
  }

  function push(self, item: T) -> int {
    $rust_function
  }

  function concat(self, other: T[]) -> T[] {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1);
        if let Item::Class(c) = &items[0] {
            assert_eq!(c.name.as_str(), "Array");
            assert_eq!(c.generic_params.len(), 1);
            assert_eq!(c.generic_params[0].name.as_str(), "T");
            // 4 user-defined stubs + 2 auto-derived (`to_json`, `from_json`).
            let stub_methods: Vec<_> = c
                .methods
                .iter()
                .filter(|m| m.metadata.origin != crate::ast::FunctionOrigin::AutoDerive)
                .collect();
            assert_eq!(stub_methods.len(), 4);
            for method in &stub_methods {
                assert!(
                    matches!(
                        &method.body,
                        Some(FunctionBodyDef::Builtin(BuiltinKind::Vm))
                    ),
                    "method {} should be Builtin(Vm)",
                    method.name
                );
            }
        } else {
            panic!("expected Item::Class");
        }
    }

    #[test]
    fn roundtrip_no_panic_on_map_stub() {
        let source = r#"
class Map<K, V> {
  function length(self) -> int {
    $rust_function
  }

  function has(self, key: K) -> bool {
    $rust_function
  }

  function keys(self) -> K[] {
    $rust_function
  }

  function values(self) -> V[] {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1);
        if let Item::Class(c) = &items[0] {
            assert_eq!(c.name.as_str(), "Map");
            assert_eq!(c.generic_params.len(), 2);
        } else {
            panic!("expected Item::Class");
        }
    }

    #[test]
    fn roundtrip_no_panic_on_media_stub_with_rust_type() {
        let source = r#"
class Media {
  _data $rust_type

  function url(self) -> string {
    $rust_function
  }

  function base64(self) -> string {
    $rust_function
  }
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1);
        if let Item::Class(c) = &items[0] {
            assert_eq!(c.name.as_str(), "Media");
            assert!(c.generic_params.is_empty());
            let data_field = c.fields.iter().find(|f| f.name.as_str() == "_data");
            assert!(data_field.is_some(), "expected _data field");
            assert!(
                matches!(
                    &data_field.unwrap().type_expr.kind,
                    TypeExprKind::Rust { .. }
                ),
                "_data field should have TypeExprKind::Rust"
            );
        } else {
            panic!("expected Item::Class");
        }
    }

    #[test]
    fn function_throws_clause_lowers_to_never_type() {
        let source = r#"
function f() -> int throws never {
  return 1
}
"#;
        let func = first_function(parse_and_lower(source));
        let throws = func
            .throws
            .expect("expected throws clause to be lowered into FunctionDef.throws");
        assert!(
            matches!(throws.kind, TypeExprKind::Never { .. }),
            "expected throws type to lower as TypeExprKind::Never, got {:?}",
            throws.kind
        );
    }

    #[test]
    fn throw_statement_and_expression_are_lowered() {
        let source = r#"
function f() -> int {
  throw "boom"
}

function g() -> int {
  return throw 1
}
"#;
        let items = parse_and_lower(source);
        let mut funcs = items.into_iter().filter_map(|item| {
            if let Item::Function(f) = item {
                Some(f)
            } else {
                None
            }
        });

        let f = funcs.next().expect("expected first function");
        if let Some(FunctionBodyDef::Expr(body, _)) = &f.body {
            let root = body.root_expr.expect("expected root expr");
            let Expr::Block { stmts, .. } = &body.exprs[root] else {
                panic!("expected block root expression");
            };
            let first_stmt = &body.stmts[stmts[0]];
            assert!(
                matches!(first_stmt, Stmt::Throw { .. }),
                "expected first statement to be Stmt::Throw, got {first_stmt:?}"
            );
        } else {
            panic!("expected expression body for f");
        }

        let g = funcs.next().expect("expected second function");
        if let Some(FunctionBodyDef::Expr(body, _)) = &g.body {
            let root = body.root_expr.expect("expected root expr");
            let Expr::Block { stmts, .. } = &body.exprs[root] else {
                panic!("expected block root expression");
            };
            let first_stmt = &body.stmts[stmts[0]];
            let Stmt::Return(Some(ret_expr)) = first_stmt else {
                panic!("expected `return throw ...` statement");
            };
            assert!(
                matches!(&body.exprs[*ret_expr], Expr::Throw { .. }),
                "expected return expression to be Expr::Throw, got {:?}",
                body.exprs[*ret_expr]
            );
        } else {
            panic!("expected expression body for g");
        }
    }

    #[test]
    fn throw_call_catch_binds_catch_to_payload_expression() {
        let source = r#"
function make_err() -> int {
  return 1
}

function f() -> int {
  return throw make_err() catch (e) {
    _ => 0
  }
}
"#;
        let items = parse_and_lower(source);
        let f = items
            .into_iter()
            .filter_map(|item| {
                if let Item::Function(func) = item {
                    Some(func)
                } else {
                    None
                }
            })
            .find(|func| func.name.as_str() == "f")
            .expect("expected function f");

        if let Some(FunctionBodyDef::Expr(body, sm)) = &f.body {
            let root = body.root_expr.expect("expected root expr");
            let Expr::Block { stmts, .. } = &body.exprs[root] else {
                panic!("expected block root expression");
            };
            let ret_expr = match &body.stmts[stmts[0]] {
                Stmt::Return(Some(expr_id)) => *expr_id,
                other => panic!("expected return statement, got {other:?}"),
            };

            let (catch_base, catch_clauses) = match &body.exprs[ret_expr] {
                Expr::Catch { base, clauses } => (*base, clauses),
                other => panic!("expected return expression to be Expr::Catch, got {other:?}"),
            };

            let thrown_value = match &body.exprs[catch_base] {
                Expr::Throw { value } => *value,
                other => panic!("expected catch base to be Expr::Throw, got {other:?}"),
            };
            assert!(
                matches!(&body.exprs[thrown_value], Expr::Call { .. }),
                "expected throw payload to be call expression"
            );

            assert_eq!(catch_clauses.len(), 1);
            let first_arm = catch_clauses[0].arms[0];
            let arm_span = sm.catch_arm_span(first_arm);
            assert!(
                !arm_span.is_empty(),
                "expected non-empty catch arm span in source map"
            );
        } else {
            panic!("expected expression body for f");
        }
    }

    // ── Phase 1: retry_policy produces Item::Let with LetOrigin::RetryPolicy ──

    // ── Postfix type expression tests ────────────────────────────────────────

    fn first_type_alias(items: Vec<Item>) -> crate::ast::TypeAliasDef {
        items
            .into_iter()
            .find_map(|item| {
                if let Item::TypeAlias(ta) = item {
                    Some(ta)
                } else {
                    None
                }
            })
            .expect("expected a TypeAliasDef")
    }

    #[test]
    fn type_expr_simple_optional() {
        let ta = first_type_alias(parse_and_lower("type T = int?\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(Optional(Int))
        );
    }

    #[test]
    fn type_expr_simple_array() {
        let ta = first_type_alias(parse_and_lower("type T = int[]\n"));
        assert_eq!(strip_spans(&ta.type_expr.unwrap()), type_expr!(List(Int)));
    }

    #[test]
    fn type_expr_array_optional() {
        // int[]? = Optional(List(Int))
        let ta = first_type_alias(parse_and_lower("type T = int[]?\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(Optional(List(Int)))
        );
    }

    #[test]
    fn type_expr_optional_in_array() {
        // string?[] = List(Optional(String))
        let ta = first_type_alias(parse_and_lower("type T = string?[]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(Optional(String)))
        );
    }

    #[test]
    fn type_expr_optional_array_optional() {
        // string?[]? = Optional(List(Optional(String)))
        let ta = first_type_alias(parse_and_lower("type T = string?[]?\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(Optional(List(Optional(String))))
        );
    }

    #[test]
    fn type_expr_nested_int_array() {
        // int[][] = List(List(Int))
        let ta = first_type_alias(parse_and_lower("type T = int[][]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(List(Int)))
        );
    }

    #[test]
    fn type_expr_triple_nested_array() {
        // int[][][] = List(List(List(Int)))
        let ta = first_type_alias(parse_and_lower("type T = int[][][]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(List(List(Int))))
        );
    }

    #[test]
    fn function_type_throws_preserves_omission_vs_explicit_never() {
        let omitted = first_type_alias(parse_and_lower(
            "type Omitted = (cb: (value: int) -> string) -> void\n",
        ));
        let explicit = first_type_alias(parse_and_lower(
            "type Explicit = (cb: (value: int) -> string throws never) -> void\n",
        ));

        let omitted_outer = omitted.type_expr.expect("expected type alias body");
        let TypeExprKind::Function { params, .. } = &omitted_outer.kind else {
            panic!("expected outer function type for omitted case");
        };
        let TypeExprKind::Function { throws, .. } = &params[0].ty.kind else {
            panic!("expected inner function type for omitted case");
        };
        assert!(
            throws.is_none(),
            "expected omitted nested throws to stay None in raw AST, got {throws:?}"
        );

        let explicit_outer = explicit.type_expr.expect("expected type alias body");
        let TypeExprKind::Function { params, .. } = &explicit_outer.kind else {
            panic!("expected outer function type for explicit case");
        };
        let TypeExprKind::Function { throws, .. } = &params[0].ty.kind else {
            panic!("expected inner function type for explicit case");
        };
        assert!(
            matches!(
                throws.as_deref().map(|t| &t.kind),
                Some(TypeExprKind::Never { .. })
            ),
            "expected explicit nested throws never to be preserved, got {throws:?}"
        );
    }

    #[test]
    fn type_expr_paren_union_array() {
        // (int | string)[] = List(Union(Int, String))
        let ta = first_type_alias(parse_and_lower("type T = (int | string)[]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(Union((Int), (String))))
        );
    }

    #[test]
    fn type_expr_nested_union_array() {
        // (int | bool)[][] = List(List(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(List(Union((Int), (Bool)))))
        );
    }

    #[test]
    fn type_expr_nested_union_array_opt() {
        // (int | bool)[][]? = Optional(List(List(Union(Int, Bool))))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]?\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(Optional(List(List(Union((Int), (Bool))))))
        );
    }

    #[test]
    fn type_expr_opt_union_in_array() {
        // (int | bool)?[] = List(Optional(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)?[]\n"));
        assert_eq!(
            strip_spans(&ta.type_expr.unwrap()),
            type_expr!(List(Optional(Union((Int), (Bool)))))
        );
    }

    // ── Phase 1: retry_policy produces Item::Let with LetOrigin::RetryPolicy ──

    #[test]
    fn retry_policy_produces_let_item_with_retry_policy_origin() {
        // Renamed behavior: retry_policy blocks are removed; retry composes
        // at the client boundary (ai.Retry).
        let source = r#"
retry_policy MyRetry {
  max_retries 3
}
"#;
        let (items, diags) = parse_and_lower_with_diagnostics(source);
        assert!(items.is_empty(), "retry_policy lowers to no items");
        assert!(
            diags.iter().any(|diag| matches!(
                diag,
                crate::LoweringDiagnostic::RetryPolicyRemoved { name, .. } if name == "MyRetry"
            )),
            "expected RetryPolicyRemoved, got: {diags:#?}"
        );
    }

    #[test]
    fn quoted_string_literals_decode_escape_sequences() {
        let source = r#"
function main() -> string {
  "\n"
}
"#;

        let function = first_function(parse_and_lower(source));
        let Some(FunctionBodyDef::Expr(body, _)) = &function.body else {
            panic!("expected expression body");
        };

        let root = body.root_expr.expect("expected root expr");
        let Expr::Block { tail_expr, .. } = &body.exprs[root] else {
            panic!("expected block root expression");
        };
        let tail = tail_expr.expect("expected tail expression");

        assert_eq!(
            &body.exprs[tail],
            &Expr::Literal(crate::ast::Literal::String("\n".to_string()))
        );
    }

    // ── Type attribute tests ─────────────────────────────────────────────────

    fn first_class(items: Vec<Item>) -> crate::ast::ClassDef {
        items
            .into_iter()
            .find_map(|item| {
                if let Item::Class(c) = item {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("expected a ClassDef")
    }

    #[test]
    fn type_attr_before_field_attr_parses_as_type_attribute() {
        // @stream.done is a type attribute, @alias("bar") is a field attribute.
        // When @stream.done comes first, the parser should nest it inside TYPE_EXPR.
        let source = r#"
class Foo {
  foo Fizz @stream.done @alias("bar")
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "foo")
            .expect("expected field 'foo'");

        // Field attribute: @alias("bar")
        assert_eq!(
            field.attributes.len(),
            1,
            "expected 1 field attribute, got {:?}",
            field.attributes
        );
        assert_eq!(field.attributes[0].name.as_str(), "alias");

        // Type attribute: @stream.done should be on the TypeExpr
        let type_expr = &field.type_expr;
        let type_attrs = type_expr.attrs();
        assert_eq!(
            type_attrs.len(),
            1,
            "expected 1 type attribute, got {type_attrs:?}"
        );
        assert_eq!(type_attrs[0].name.as_str(), "stream.done");
    }

    #[test]
    fn type_attr_after_field_attr_parses_as_type_attribute() {
        // THE FIX: @alias("bar") before @stream.done now works correctly.
        // Both attrs are consumed inside TYPE_EXPR, then disambiguation
        // hoists @alias to FieldDef and keeps @stream.done on TypeExpr.
        let source = r#"
class Foo {
  foo Fizz @alias("bar") @stream.done
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "foo")
            .expect("expected field 'foo'");

        // Field attribute: @alias("bar") — hoisted from TypeExpr to FieldDef
        assert_eq!(
            field.attributes.len(),
            1,
            "expected 1 field attribute, got {:?}",
            field.attributes
        );
        assert_eq!(field.attributes[0].name.as_str(), "alias");

        // Type attribute: @stream.done stays on the TypeExpr
        assert_eq!(
            strip_spans(&field.type_expr),
            type_expr!(Path("Fizz", Attr("stream.done")))
        );
    }

    #[test]
    fn type_attrs_on_optional_type() {
        let source = r#"
class Foo {
  bar int? @stream.done
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "bar")
            .expect("expected field 'bar'");

        let type_expr = &field.type_expr;
        // Type should be Optional(Int)
        assert!(
            matches!(type_expr.kind, TypeExprKind::Optional { .. }),
            "expected Optional type, got {type_expr:?}",
        );
        // @stream.done should be a type attribute
        let type_attrs = type_expr.attrs();
        assert_eq!(
            type_attrs.len(),
            1,
            "expected 1 type attribute, got {type_attrs:?}"
        );
        assert_eq!(type_attrs[0].name.as_str(), "stream.done");
    }

    #[test]
    fn type_attrs_on_array_type() {
        let source = r#"
class Foo {
  items string[] @stream.done
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "items")
            .expect("expected field 'items'");

        let type_expr = &field.type_expr;
        assert!(
            matches!(type_expr.kind, TypeExprKind::List { .. }),
            "expected List type, got {type_expr:?}",
        );
        let type_attrs = type_expr.attrs();
        assert_eq!(
            type_attrs.len(),
            1,
            "expected 1 type attribute, got {type_attrs:?}"
        );
        assert_eq!(type_attrs[0].name.as_str(), "stream.done");
        // Type attribute: @stream.done stays on the TypeExpr
        assert_eq!(
            strip_spans(&field.type_expr),
            type_expr!(WithAttrs((List(String)), Attr("stream.done")))
        );
    }

    // ── Attribute disambiguation sanity checks ──────────────────────────────
    //
    // Comprehensive coverage lives in baml_tests/projects/attr_disambiguation/.
    // These unit tests verify the core AST-level mechanics:
    //  1. The bug fix (field-before-type ordering)
    //  2. Union trailing attr → hoisted to FieldDef
    //  3. Nested field attr → validation error

    /// Helper: parse BAML source, lower to AST, and also return field-attr validation diagnostics.
    fn parse_lower_validate(
        source: &str,
    ) -> (Vec<Item>, Vec<(std::string::String, text_size::TextRange)>) {
        let root = parse(source);
        let (items, diags, _env_var_refs) = lower_file(&root);
        // Separate out field-attr-in-type-position diagnostics from other diagnostics.
        let mut field_attr_errors = Vec::new();
        let mut other_diags = Vec::new();
        for d in diags {
            match d {
                crate::lowering_diagnostic::LoweringDiagnostic::FieldAttributeInTypePosition {
                    attr_name,
                    span,
                } => {
                    field_attr_errors.push((attr_name, span));
                }
                other => other_diags.push(other),
            }
        }
        assert!(
            other_diags.is_empty(),
            "expected no non-field-attr diagnostics, got: {other_diags:#?}"
        );
        (items, field_attr_errors)
    }

    #[test]
    fn field_attr_before_type_attr_disambiguated_correctly() {
        // The core bug: @alias before @stream.done used to misclassify @stream.done.
        let source = r#"
class C {
  f Foo @alias("x") @stream.done
}
"#;
        let (items, diags) = parse_lower_validate(source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
        let class = first_class(items);
        let field = &class.fields[0];
        assert_eq!(field.attributes.len(), 1);
        assert_eq!(field.attributes[0].name.as_str(), "alias");
        let te = &field.type_expr;
        assert_eq!(te.attrs().len(), 1);
        assert_eq!(te.attrs()[0].name.as_str(), "stream.done");
    }

    #[test]
    fn custom_schema_attr_is_hoisted_but_stream_attr_stays_on_type() {
        let source = r#"
class C {
  f string @custom("read-back") @stream.done
}
"#;
        let (items, diags) = parse_lower_validate(source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
        let class = first_class(items);
        let field = &class.fields[0];
        assert_eq!(field.attributes.len(), 1);
        assert_eq!(field.attributes[0].name.as_str(), "custom");
        assert_eq!(field.type_expr.attrs().len(), 1);
        assert_eq!(field.type_expr.attrs()[0].name.as_str(), "stream.done");
    }

    #[test]
    fn union_trailing_field_attr_hoisted_to_field() {
        // A | B | C @alias("x") → @alias hoisted to FieldDef, Union has no attrs.
        let source = r#"
class C {
  f A | B | C @alias("x")
}
"#;
        let (items, diags) = parse_lower_validate(source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
        let class = first_class(items);
        let field = &class.fields[0];
        assert_eq!(field.attributes.len(), 1);
        assert_eq!(field.attributes[0].name.as_str(), "alias");
        assert!(matches!(
            &field.type_expr.kind,
            TypeExprKind::Union { attrs, .. } if attrs.is_empty()
        ));
    }

    #[test]
    fn field_attr_in_nested_position_produces_diagnostic() {
        // (Foo @alias("x"))[] → @alias inside parens is an error.
        let source = r#"
class C {
  f (Foo @alias("x"))[]
}
"#;
        let (_, diags) = parse_lower_validate(source);
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].0, "alias");
    }

    #[test]
    fn type_attr_on_inner_union_member_stays_on_member() {
        // (A | B @stream.done) | C → @stream.done should apply to B specifically,
        // not to the inner union (A | B).
        let source = r#"
class C {
  f (A | B @stream.done) | C
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = &class.fields[0];
        assert_eq!(
            strip_spans(&field.type_expr),
            type_expr!(Union(
                (Union((Path("A")), (Path("B", Attr("stream.done"))))),
                (Path("C"))
            ))
        );
    }

    #[test]
    fn paren_union_trailing_type_attr_stays_on_last_member() {
        // (A | B | C @stream.done) → no trailing hoisting inside type expressions,
        // so @stream.done stays on C, not on the inner union.
        let source = r#"
class C {
  f (A | B | C @stream.done)
}
"#;
        let (items, diags) = parse_lower_validate(source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
        let class = first_class(items);
        let field = &class.fields[0];

        assert_eq!(
            strip_spans(&field.type_expr),
            type_expr!(Union(
                (Path("A")),
                (Path("B")),
                (Path("C", Attr("stream.done")))
            ))
        );
    }

    #[test]
    fn paren_union_trailing_field_attr_produces_diagnostic() {
        // (A | B | C @alias("x")) → @alias is a field attr inside parens,
        // should produce a diagnostic (can't be hoisted from nested position).
        let source = r#"
class C {
  f (A | B | C @alias("x"))
}
"#;
        let (_, diags) = parse_lower_validate(source);
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].0, "alias");
    }

    // ─── BEP-049: backtick string literal lowering ────────────────────────────

    fn extract_first_string_literal(items: Vec<Item>) -> String {
        let function = first_function(items);
        let Some(FunctionBodyDef::Expr(body, _sm)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("expected root expr");
        // Body is wrapped in a Block; find the tail or first statement string.
        let candidate = match &body.exprs[root] {
            Expr::Block {
                tail_expr: Some(tail),
                ..
            } => *tail,
            Expr::Block { stmts, .. } => match &body.stmts[stmts[0]] {
                Stmt::Expr(expr_id) => *expr_id,
                Stmt::Let {
                    initializer: Some(init),
                    ..
                } => *init,
                other => panic!("unexpected stmt: {other:?}"),
            },
            _ => root,
        };
        // An untagged backtick lowers to `Expr::Template`; its desugared
        // realization (a string literal, for pure-text templates) lives in
        // `TemplateTag::Default { elaborated }`.
        let candidate = match &body.exprs[candidate] {
            Expr::Template {
                tag: crate::ast::TemplateTag::Default { elaborated },
                ..
            } => *elaborated,
            _ => candidate,
        };
        match &body.exprs[candidate] {
            Expr::Literal(baml_base::Literal::String(s)) => s.clone(),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn backtick_one_liner_lowers_to_string_literal() {
        let source = "
function Demo() -> string {
    `hello world`
}
";
        let items = parse_and_lower(source);
        assert_eq!(extract_first_string_literal(items), "hello world");
    }

    #[test]
    fn backtick_decodes_standard_escapes() {
        let source = r#"
function Demo() -> string {
    `line\nbreak`
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(extract_first_string_literal(items), "line\nbreak");
    }

    #[test]
    fn backtick_escapes_backtick_and_dollar() {
        let source = r#"
function Demo() -> string {
    `a\`b\${name}c`
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(extract_first_string_literal(items), "a`b${name}c");
    }

    #[test]
    fn backtick_multiline_dedents() {
        let source = "
function Demo() -> string {
    `
        line one
        line two
    `
}
";
        let items = parse_and_lower(source);
        assert_eq!(extract_first_string_literal(items), "line one\nline two");
    }

    #[test]
    fn backtick_multi_tick_ladder_preserves_inner_ticks() {
        let source = "
function Demo() -> string {
    ``inline `code` here``
}
";
        let items = parse_and_lower(source);
        assert_eq!(extract_first_string_literal(items), "inline `code` here");
    }

    #[test]
    fn backtick_interpolation_lowers_to_concat_chain() {
        // BEP §11: `Hello, ${name}!` is an untagged `Expr::Template` whose
        // `Default { elaborated }` realization is the left-folded Binary Add
        // chain ("Hello, " + name.to_string()) + "!" over the segments.
        let source = "
function Demo(name: string) -> string {
    `Hello, ${name}!`
}
";
        let items = parse_and_lower(source);
        let function = first_function(items);
        let Some(FunctionBodyDef::Expr(body, _)) = &function.body else {
            panic!("expected expression body");
        };
        let root = body.root_expr.expect("root");
        let Expr::Block {
            tail_expr: Some(tail),
            ..
        } = &body.exprs[root]
        else {
            panic!("expected Block at root, got {:?}", body.exprs[root]);
        };
        let Expr::Template {
            tag: crate::ast::TemplateTag::Default { elaborated },
            ..
        } = &body.exprs[*tail]
        else {
            panic!(
                "expected untagged Template at tail, got {:?}",
                body.exprs[*tail]
            );
        };
        let Expr::Binary { op, lhs, rhs } = &body.exprs[*elaborated] else {
            panic!(
                "expected Binary at elaborated root, got {:?}",
                body.exprs[*elaborated]
            );
        };
        assert!(matches!(op, crate::ast::BinaryOp::Add));
        assert!(matches!(
            &body.exprs[*rhs],
            Expr::Literal(baml_base::Literal::String(s)) if s == "!"
        ));
        let Expr::Binary {
            op: op2, lhs: lhs2, ..
        } = &body.exprs[*lhs]
        else {
            panic!("expected nested Binary on lhs");
        };
        assert!(matches!(op2, crate::ast::BinaryOp::Add));
        assert!(matches!(
            &body.exprs[*lhs2],
            Expr::Literal(baml_base::Literal::String(s)) if s == "Hello, "
        ));
    }

    #[test]
    fn property_syntax_is_structural_ast_data() {
        let items = parse_and_lower(
            r#"
class Config { name string }
function build(name: string) -> unknown {
  let shorthand_map = { name };
  let explicit_map = { "name": name };
  let shorthand_object = Config { name };
  let explicit_object = Config { name: name };
  [shorthand_map, explicit_map, shorthand_object, explicit_object]
}
"#,
        );
        let function = first_function(items);
        let Some(FunctionBodyDef::Expr(body, _)) = &function.body else {
            panic!("expected expression body");
        };

        let map_syntax: Vec<_> = body
            .exprs
            .iter()
            .filter_map(|(_, expr)| match expr {
                Expr::Map { entries } => entries.first().map(|entry| entry.syntax),
                _ => None,
            })
            .collect();
        let object_syntax: Vec<_> = body
            .exprs
            .iter()
            .filter_map(|(_, expr)| match expr {
                Expr::Object { fields, .. } => fields.first().map(|field| field.syntax),
                _ => None,
            })
            .collect();

        assert_eq!(
            map_syntax,
            vec![
                crate::ast::PropertySyntax::Shorthand,
                crate::ast::PropertySyntax::Explicit,
            ]
        );
        assert_eq!(
            object_syntax,
            vec![
                crate::ast::PropertySyntax::Shorthand,
                crate::ast::PropertySyntax::Explicit,
            ]
        );
    }
}

#[cfg(test)]
mod traverse_coverage_tests {
    use crate::{
        ast::{Expr, FunctionBodyDef, Item},
        traverse::BodyNode,
    };

    /// Every expression and statement a lambda-free body allocates must be
    /// reachable from its root. A child this walker forgets would be silently
    /// dropped by every analysis built on it — an unwalked `throw` simply
    /// vanishes from the function's effect set.
    #[test]
    fn every_allocated_node_is_reachable_from_the_root() {
        let sources = [
            r#"function f(a: int, b: int) -> int throws string {
  let m = { "k": a + b }
  let arr = [a, b, m["k"]]
  for (let x in arr) { if (x > 0) { throw "pos" } }
  let i = 0
  while (i < 3) { i = i + 1 }
  match (a) { 1 => { throw "one" }, _ if b > 0 => { b }, _ => { 0 } }
}"#,
            r#"function g(o: int?, cb: () -> int) -> int throws never {
  defer { let z = 1 }
  let v = o?.to_string()
  let c = cb() catch (e) { _ => 0 }
  return c
}"#,
            r#"function h(xs: int[]) -> int throws never {
  if let [first, ..rest] = xs { return first }
  return 0
}"#,
            // Backtick templates carry their expressions twice — once in
            // `segments`, once in the desugared tag payload — from the same
            // `ExprId`s. Both must be walked, and neither may be walked twice.
            r#"function t(a: string, n: int) -> string throws never {
  let plain = `hi ${a} there`
  let looped = `${for (let i in [1, 2])}x${a}${endfor}`
  let branched = `${if (n > 0)}pos${else}neg${endif}`
  return plain + looped + branched
}"#,
            // BEP-066 hides ordinary expression nodes inside type arguments,
            // type bindings, and patterns. Canonical traversal must still see
            // every one exactly once.
            r#"function runtime_edges(t: reflect.Type, value: int) -> int throws never {
  type T = unreflect(t)
  let called = identity<unreflect(t)>(value)
  let tested = value is unreflect(t)
  match (value) {
    unreflect(t) => called,
    _ => if (tested) { value } else { 0 }
  }
}"#,
        ];
        for source in sources {
            for item in super::tests::parse_and_lower(source) {
                let Item::Function(f) = item else { continue };
                let Some(FunctionBodyDef::Expr(body, _)) = &f.body else {
                    continue;
                };
                // Skip bodies containing lambdas: their nodes live in a nested
                // arena, so arena membership and reachability legitimately differ.
                if body.exprs.iter().any(|(_, e)| matches!(e, Expr::Lambda(_))) {
                    continue;
                }
                let Some(root) = body.root_expr else { continue };
                let reached: std::collections::HashSet<BodyNode> =
                    body.reachable_excluding_lambdas(root).into_iter().collect();

                let missed_exprs: Vec<_> = body
                    .exprs
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|id| !reached.contains(&BodyNode::Expr(*id)))
                    .collect();
                let missed_stmts: Vec<_> = body
                    .stmts
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|id| !reached.contains(&BodyNode::Stmt(*id)))
                    .collect();

                // The arena is a DAG (templates share ids between their two
                // representations), so the walk must also not repeat itself:
                // callers that push per visit would report duplicates.
                let walked = body.reachable_excluding_lambdas(root);
                assert_eq!(
                    walked.len(),
                    reached.len(),
                    "`{}` visited {} nodes for {} unique — the walk must de-duplicate",
                    f.name,
                    walked.len(),
                    reached.len(),
                );

                assert!(
                    missed_exprs.is_empty() && missed_stmts.is_empty(),
                    "unreachable nodes in `{}`:\n  exprs: {:?}\n  stmts: {:?}\nsource:\n{source}",
                    f.name,
                    missed_exprs
                        .iter()
                        .map(|id| (*id, &body.exprs[*id]))
                        .collect::<Vec<_>>(),
                    missed_stmts
                        .iter()
                        .map(|id| (*id, &body.stmts[*id]))
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
}

#[cfg(test)]
mod testset_nesting_tests {
    use crate::ast::{Expr, FunctionBodyDef, Item, Stmt};

    /// Count `<collector>.register_test` / `register_test_set` calls in a body.
    ///
    /// Lambda bodies share this arena, so the flat scan already covers the
    /// registration lambdas the test/testset wrappers introduce.
    fn count_registrations(body: &crate::ast::ExprBody) -> usize {
        let mut n = 0;
        for (_, expr) in body.exprs.iter() {
            if let Expr::Call { callee, .. } = expr {
                if let Expr::MemberAccess { member, .. } = &body.exprs[*callee]
                    && matches!(member.as_str(), "register_test" | "register_test_set")
                {
                    n += 1;
                }
                if let Expr::Path(segments) = &body.exprs[*callee]
                    && segments.last().is_some_and(|s| {
                        matches!(s.as_str(), "register_test_at" | "register_test_set_at")
                    })
                {
                    n += 1;
                }
            }
        }
        n
    }

    fn count_missing_stmts(body: &crate::ast::ExprBody) -> usize {
        body.stmts
            .iter()
            .filter(|(_, s)| matches!(s, Stmt::Missing))
            .count()
    }

    /// A `test` nested inside a `testset` registers; a `test` nested inside a
    /// `test` does not.
    ///
    /// The distinction is carried entirely by `LoweringContext::testset_collector_var`:
    /// a testset body sets it, a test body clears it. Both intents are currently
    /// expressed by *which constructor* builds the body's context, so anything
    /// that changes how those bodies are lowered has to reproduce both — set and
    /// clear. Getting only the "set" half right makes `test` silently nest.
    #[test]
    fn a_test_inside_a_test_does_not_register() {
        let items = super::tests::parse_and_lower(
            r#"testset "Outer" {
  test "Middle" {
    test "Inner" {
      assert.is_true(true)
    }
  }
}"#,
        );

        let init = items
            .iter()
            .find_map(|item| match item {
                Item::Function(f) if f.name.as_str().starts_with("$init_test") => Some(f),
                _ => None,
            })
            .expect("expected a synthesized $init_test function");
        let Some(FunctionBodyDef::Expr(body, _)) = &init.body else {
            panic!("expected an expression body for $init_test");
        };

        // Outer testset + Middle test = 2. `Inner` sits in a test body, where
        // the collector is cleared, so it lowers to `Stmt::Missing` instead.
        //
        // This pins the *registration* behaviour, not the diagnostic story:
        // dropping `Inner` silently is a separate known bug (see the `// BUG:`
        // note on the `TEST_EXPR_DEF` arm in `lower_expr_body.rs`).
        assert_eq!(
            count_registrations(body),
            2,
            "expected exactly the testset and the test it contains to register"
        );
        assert!(
            count_missing_stmts(body) >= 1,
            "expected the doubly-nested `test` to lower to `Stmt::Missing`"
        );
    }
}

#[cfg(test)]
mod lambda_arena_tests {
    use crate::ast::{Expr, FunctionBodyDef, Item, Stmt};

    /// A lambda's body is allocated in the enclosing function's arena, and is
    /// *not* reachable from that function's root.
    ///
    /// This is why effect analyses cannot scan the arena flatly: a `throw`
    /// written inside a lambda is a sibling of the function's own statements,
    /// indistinguishable from one the function wrote itself.
    #[test]
    fn a_lambdas_throw_is_in_the_enclosing_arena_but_not_reachable_from_its_root() {
        let items = super::tests::parse_and_lower(
            r#"function defines(value: int) -> int throws never {
  let risky = (n: int) -> int {
    throw "boom"
  }
  return value
}"#,
        );
        let Some(Item::Function(f)) = items.into_iter().next() else {
            panic!("expected a function item");
        };
        let Some(FunctionBodyDef::Expr(body, _)) = &f.body else {
            panic!("expected an expression body");
        };

        let throw_stmts: Vec<_> = body
            .stmts
            .iter()
            .filter(|(_, s)| matches!(s, Stmt::Throw { .. }))
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            throw_stmts.len(),
            1,
            "the lambda's `throw` must live in the enclosing function's arena"
        );

        let root = body.root_expr.expect("root");
        let reached = body.reachable_excluding_lambdas(root);
        assert!(
            !reached.contains(&crate::traverse::BodyNode::Stmt(throw_stmts[0])),
            "a structural walk that stops at lambdas must not reach the lambda's `throw`"
        );

        // And the lambda really does own it.
        let lambda_root = body
            .exprs
            .iter()
            .find_map(|(_, e)| match e {
                Expr::Lambda(l) => l.body,
                _ => None,
            })
            .expect("lambda body");
        assert!(
            body.reachable_excluding_lambdas(lambda_root)
                .contains(&crate::traverse::BodyNode::Stmt(throw_stmts[0])),
            "walking from the lambda's own root must reach its `throw`"
        );
    }
}
