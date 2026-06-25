//! `baml_compiler2_ast` — Concrete AST structs and CST → AST lowering.
//!
//! This crate isolates all CST messiness in one boundary layer. After
//! `lower_file` returns, the CST is never needed again — all structural
//! content is owned by the returned `Vec<Item>`.
//!
//! No Salsa dependency. Everything downstream works with owned data and
//! can be constructed directly in tests without parsing.

pub mod ast;
pub(crate) mod auto_derive_json;
pub mod borsh_helpers;
pub(crate) mod companions;
pub(crate) mod disambiguate;
pub mod docstring;
pub(crate) mod lower_config_item;
pub(crate) mod lower_cst;
pub(crate) mod lower_expr_body;
pub(crate) mod lower_type_expr;
pub mod lowering_diagnostic;

pub use ast::*;
/// Decode common escape sequences in a quoted string literal body.
///
/// Re-exported from [`baml_base::escape::unescape_string_literal`] so existing
/// callers don't need to change their import path.
pub use baml_base::escape::unescape_string_literal;
pub use companions::llm_parse as llm_parse_companion;
pub use disambiguate::is_field_attr;
pub use docstring::extract_docstring;
pub use lower_cst::{
    lower_file, lower_file_with_path, synthesize_llm_builtin_call, synthesize_llm_make_stream_call,
};
pub use lower_expr_body::EnvVarRef;
pub use lowering_diagnostic::LoweringDiagnostic;

/// The BEP-044 `default` receiver keyword. Inside an `implements` block,
/// `default.method(...)` invokes the interface's *default* method body,
/// deliberately bypassing the class's override. It is a **contextual** keyword:
/// the lexer produces an ordinary identifier, and TIR/MIR recognize it by name
/// at the root of a path — so a local named `default` shadows it. This constant
/// is the single source of truth for that spelling; prefer the
/// `is_default_receiver_root` helpers over comparing the literal string.
pub const DEFAULT_RECEIVER_KEYWORD: &str = "default";

/// Parse the digit body of a `bigint` literal into a [`num_bigint::BigInt`].
///
/// The lexer (`baml_compiler_lexer`) guarantees one-or-more ASCII decimal
/// digits, so a parse failure here indicates the CST has been corrupted.
/// Callers should pass the digit-only string (with the trailing `n` suffix
/// already stripped).
pub fn parse_bigint_literal_digits(digits: &str) -> num_bigint::BigInt {
    num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10).unwrap_or_else(|| {
        unreachable!("CST bigint_literal returned non-decimal digits: {digits:?}")
    })
}

/// Parse the raw text of a `BIGINT_LITERAL` token (digits plus trailing
/// lowercase `n` suffix) into a [`num_bigint::BigInt`]. The lexer guarantees
/// the suffix is present, so its absence panics with `unreachable!`.
pub fn parse_bigint_literal_token(text: &str) -> num_bigint::BigInt {
    let digits = text
        .strip_suffix('n')
        .unwrap_or_else(|| unreachable!("BIGINT_LITERAL missing 'n' suffix: {text:?}"));
    parse_bigint_literal_digits(digits)
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
        ast::{BuiltinKind, Expr, FunctionBodyDef, Item, Stmt, TypeExpr},
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
        (Int $(, Attr($a:expr))*) => { TypeExpr::Int { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Bigint $(, Attr($a:expr))*) => { TypeExpr::Bigint { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Float $(, Attr($a:expr))*) => { TypeExpr::Float { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (String $(, Attr($a:expr))*) => { TypeExpr::String { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Bool $(, Attr($a:expr))*) => { TypeExpr::Bool { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Null $(, Attr($a:expr))*) => { TypeExpr::Null { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Never $(, Attr($a:expr))*) => { TypeExpr::Never { attrs: type_expr!(@attrs $(, Attr($a))*) } };
        (Rust $(, Attr($a:expr))*) => { TypeExpr::Rust { attrs: type_expr!(@attrs $(, Attr($a))*) } };

        // ── Path ──
        (Path($name:expr $(, Attr($a:expr))*)) => {
            TypeExpr::Path {
                segments: vec![baml_base::Name::new($name)],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: type_expr!(@attrs $(, Attr($a))*),
            }
        };

        // ── Containers ──
        (Optional($($inner:tt)+)) => {
            TypeExpr::Optional {
                inner: Box::new(type_expr!($($inner)+)),
                attrs: vec![],
            }
        };
        (List($($inner:tt)+)) => {
            TypeExpr::List {
                inner: Box::new(type_expr!($($inner)+)),
                attrs: vec![],
            }
        };

        // ── Union: each variant is wrapped in parens ──
        (Union($(($($variant:tt)+)),+ $(,)?)) => {
            TypeExpr::Union {
                variants: vec![$(type_expr!(($($variant)+))),+],
                attrs: vec![],
            }
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

        match expr {
            TypeExpr::Int { attrs } => TypeExpr::Int {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Bigint { attrs } => TypeExpr::Bigint {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Float { attrs } => TypeExpr::Float {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::String { attrs } => TypeExpr::String {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Bool { attrs } => TypeExpr::Bool {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Null { attrs } => TypeExpr::Null {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Uint8Array { attrs } => TypeExpr::Uint8Array {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Never { attrs } => TypeExpr::Never {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Void { attrs } => TypeExpr::Void {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Rust { attrs } => TypeExpr::Rust {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Path {
                segments,
                generic_args,
                associated_type_bindings,
                attrs,
            } => TypeExpr::Path {
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
            TypeExpr::AssociatedTypeProjection {
                base,
                interface,
                member,
                attrs,
            } => TypeExpr::AssociatedTypeProjection {
                base: Box::new(strip_spans(base)),
                interface: interface
                    .as_ref()
                    .map(|interface| Box::new(strip_spans(interface))),
                member: member.clone(),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
                inner: Box::new(strip_spans(inner)),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::List { inner, attrs } => TypeExpr::List {
                inner: Box::new(strip_spans(inner)),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
                key: Box::new(strip_spans(key)),
                value: Box::new(strip_spans(value)),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Union { variants, attrs } => TypeExpr::Union {
                variants: variants.iter().map(strip_spans).collect(),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Literal { value, attrs } => TypeExpr::Literal {
                value: value.clone(),
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Function {
                params,
                ret,
                throws,
                attrs,
            } => TypeExpr::Function {
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
            TypeExpr::Media { kind, attrs } => TypeExpr::Media {
                kind: *kind,
                attrs: strip_attrs(attrs),
            },
            TypeExpr::BuiltinUnknown { attrs } => TypeExpr::BuiltinUnknown {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Type { attrs } => TypeExpr::Type {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Error { attrs } => TypeExpr::Error {
                attrs: strip_attrs(attrs),
            },
            TypeExpr::Unknown { attrs } => TypeExpr::Unknown {
                attrs: strip_attrs(attrs),
            },
        }
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
    fn parse_and_lower(source: &str) -> Vec<Item> {
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
    fn llm_function_user_client_param_is_reserved() {
        let source = r##"
client<llm> GPT4 {
  provider "openai"
  options {
    model "gpt-4o"
    api_key "test"
  }
}

function Extract(client: string, text: string) -> string {
  client GPT4
  prompt #"{{ text }}"#
}
"##;

        let (items, diags) = parse_and_lower_with_diagnostics(source);
        assert!(
            diags.iter().any(|diag| matches!(
                diag,
                crate::LoweringDiagnostic::ReservedLlmClientParam {
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
        assert_eq!(param_names, vec!["text", "client"]);
        let default_id = function.params[1].default.expect("expected client default");
        let default_expr = &function.defaults.exprs.exprs[default_id.expr()];
        assert!(
            matches!(default_expr, Expr::Path(path) if path.len() == 1 && path[0].as_str() == "GPT4"),
            "expected compiler-injected client default to reference GPT4, got {default_expr:#?}"
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
        assert_eq!(function.generic_params[0].as_str(), "T");
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
            panic!("expected block root, got {:?}", &body.exprs[root]);
        };
        let Expr::Template { tag, segments } = &body.exprs[*tail] else {
            panic!("expected Template tail, got {:?}", &body.exprs[*tail]);
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
            &body.exprs[*tag]
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

        assert_eq!(imp.interface_target.expr.to_string(), "ToJson");
        assert_eq!(imp.for_target.expr.to_string(), "Dog");
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
        assert_eq!(imp.for_target.expr.to_string(), "other.Dog");
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
        assert_eq!(imp.for_target.expr.to_string(), "Dog<int>");
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
            interface
                .generic_params
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>(),
            vec!["T", "E"]
        );
        assert_eq!(interface.generic_param_bounds.len(), 2);
        assert_eq!(
            interface.generic_param_bounds[0]
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("Named")
        );
        assert!(interface.generic_param_bounds[1].is_none());
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
            method
                .generic_params
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>(),
            vec!["T", "E"]
        );
        assert_eq!(method.generic_param_bounds.len(), 2);
        assert_eq!(
            method.generic_param_bounds[0]
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("Named")
        );
        assert!(method.generic_param_bounds[1].is_none());
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
            throws.expr,
            TypeExpr::Path {
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
        assert_eq!(
            field
                .type_expr
                .as_ref()
                .map(|te| te.expr.to_string())
                .as_deref(),
            Some("string")
        );
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
            throws.expr,
            TypeExpr::Path {
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
        assert_eq!(class.generic_params[0].as_str(), "T");
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
        assert_eq!(class.generic_params[0].as_str(), "K");
        assert_eq!(class.generic_params[1].as_str(), "V");
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

    // ── 4.5: TypeExpr::Rust is produced for $rust_type field type ────────────

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

        match &field.type_expr {
            Some(spanned) => match &spanned.expr {
                TypeExpr::Rust { .. } => {}
                other => panic!("expected TypeExpr::Rust, got {other:?}"),
            },
            None => panic!("expected a type expression for _data field"),
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
            assert_eq!(c.generic_params[0].as_str(), "T");
            // 4 user-defined stubs + 2 auto-derived (`to_json`, `from_json`).
            let stub_methods: Vec<_> = c
                .methods
                .iter()
                .filter(|m| m.origin != crate::ast::FunctionOrigin::AutoDerive)
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
                    data_field.unwrap().type_expr.as_ref().map(|te| &te.expr),
                    Some(TypeExpr::Rust { .. })
                ),
                "_data field should have TypeExpr::Rust"
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
            matches!(throws.expr, TypeExpr::Never { .. }),
            "expected throws type to lower as TypeExpr::Never, got {:?}",
            throws.expr
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
        assert_eq!(ta.type_expr.unwrap().expr, type_expr!(Optional(Int)));
    }

    #[test]
    fn type_expr_simple_array() {
        let ta = first_type_alias(parse_and_lower("type T = int[]\n"));
        assert_eq!(ta.type_expr.unwrap().expr, type_expr!(List(Int)));
    }

    #[test]
    fn type_expr_array_optional() {
        // int[]? = Optional(List(Int))
        let ta = first_type_alias(parse_and_lower("type T = int[]?\n"));
        assert_eq!(ta.type_expr.unwrap().expr, type_expr!(Optional(List(Int))));
    }

    #[test]
    fn type_expr_optional_in_array() {
        // string?[] = List(Optional(String))
        let ta = first_type_alias(parse_and_lower("type T = string?[]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(Optional(String)))
        );
    }

    #[test]
    fn type_expr_optional_array_optional() {
        // string?[]? = Optional(List(Optional(String)))
        let ta = first_type_alias(parse_and_lower("type T = string?[]?\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(Optional(List(Optional(String))))
        );
    }

    #[test]
    fn type_expr_nested_int_array() {
        // int[][] = List(List(Int))
        let ta = first_type_alias(parse_and_lower("type T = int[][]\n"));
        assert_eq!(ta.type_expr.unwrap().expr, type_expr!(List(List(Int))));
    }

    #[test]
    fn type_expr_triple_nested_array() {
        // int[][][] = List(List(List(Int)))
        let ta = first_type_alias(parse_and_lower("type T = int[][][]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
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
        let TypeExpr::Function { params, .. } = &omitted_outer.expr else {
            panic!("expected outer function type for omitted case");
        };
        let TypeExpr::Function { throws, .. } = &params[0].ty else {
            panic!("expected inner function type for omitted case");
        };
        assert!(
            throws.is_none(),
            "expected omitted nested throws to stay None in raw AST, got {throws:?}"
        );

        let explicit_outer = explicit.type_expr.expect("expected type alias body");
        let TypeExpr::Function { params, .. } = &explicit_outer.expr else {
            panic!("expected outer function type for explicit case");
        };
        let TypeExpr::Function { throws, .. } = &params[0].ty else {
            panic!("expected inner function type for explicit case");
        };
        assert!(
            matches!(throws.as_deref(), Some(TypeExpr::Never { .. })),
            "expected explicit nested throws never to be preserved, got {throws:?}"
        );
    }

    #[test]
    fn type_expr_paren_union_array() {
        // (int | string)[] = List(Union(Int, String))
        let ta = first_type_alias(parse_and_lower("type T = (int | string)[]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(Union((Int), (String))))
        );
    }

    #[test]
    fn type_expr_nested_union_array() {
        // (int | bool)[][] = List(List(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(List(Union((Int), (Bool)))))
        );
    }

    #[test]
    fn type_expr_nested_union_array_opt() {
        // (int | bool)[][]? = Optional(List(List(Union(Int, Bool))))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]?\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(Optional(List(List(Union((Int), (Bool))))))
        );
    }

    #[test]
    fn type_expr_opt_union_in_array() {
        // (int | bool)?[] = List(Optional(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)?[]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(Optional(Union((Int), (Bool)))))
        );
    }

    // ── Phase 1: retry_policy produces Item::Let with LetOrigin::RetryPolicy ──

    #[test]
    fn retry_policy_produces_let_item_with_retry_policy_origin() {
        use crate::ast::{Expr, Item, LetOrigin, Literal};

        let source = r#"
retry_policy MyRetry {
  max_retries 3
  initial_delay_ms 500
  multiplier 2.0
  max_delay_ms 60000
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1, "expected exactly one item");

        let let_def = match &items[0] {
            Item::Let(ld) => ld,
            other => panic!("expected Item::Let, got {other:?}"),
        };

        assert_eq!(let_def.name.as_str(), "MyRetry");
        assert_eq!(let_def.origin, LetOrigin::RetryPolicy);

        let (body, _source_map) = let_def.initializer.as_ref().expect("expected initializer");

        let root_id = body.root_expr.expect("expected root expr");
        let root_expr = &body.exprs[root_id];

        let (type_name, fields, _) = match root_expr {
            Expr::Object {
                type_name,
                fields,
                spreads,
                ..
            } => (type_name, fields, spreads),
            other => panic!("expected Expr::Object, got {other:?}"),
        };

        assert_eq!(
            type_name.to_string(),
            "baml.llm.RetryPolicy",
            "expected type_name to be baml.llm.RetryPolicy"
        );

        // Check field names
        let field_names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            field_names,
            vec![
                "max_retries",
                "initial_delay_ms",
                "multiplier",
                "max_delay_ms"
            ]
        );

        // Check field values
        let field_exprs: Vec<&Expr> = fields.iter().map(|(_, id)| &body.exprs[*id]).collect();

        assert_eq!(
            field_exprs[0],
            &Expr::Literal(Literal::Int(3)),
            "max_retries should be Int(3)"
        );
        assert_eq!(
            field_exprs[1],
            &Expr::Literal(Literal::Int(500)),
            "initial_delay_ms should be Int(500)"
        );
        assert_eq!(
            field_exprs[2],
            &Expr::Literal(Literal::Float("2.0".to_string())),
            "multiplier should be Float(2.0)"
        );
        assert_eq!(
            field_exprs[3],
            &Expr::Literal(Literal::Int(60000)),
            "max_delay_ms should be Int(60000)"
        );
    }

    #[test]
    fn retry_policy_with_defaults_produces_let_item() {
        use crate::ast::{Item, LetOrigin};

        // A retry_policy with only max_retries set; other fields use defaults
        let source = r#"
retry_policy Simple {
  max_retries 5
}
"#;
        let items = parse_and_lower(source);
        assert_eq!(items.len(), 1);

        let let_def = match &items[0] {
            Item::Let(ld) => ld,
            other => panic!("expected Item::Let, got {other:?}"),
        };

        assert_eq!(let_def.name.as_str(), "Simple");
        assert_eq!(let_def.origin, LetOrigin::RetryPolicy);
        assert!(let_def.initializer.is_some(), "expected an initializer");
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
        let type_expr = &field.type_expr.as_ref().expect("expected type expr").expr;
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
            strip_spans(&field.type_expr.as_ref().expect("expected type expr").expr),
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

        let type_expr = &field.type_expr.as_ref().expect("expected type expr").expr;
        // Type should be Optional(Int)
        assert!(
            matches!(type_expr, TypeExpr::Optional { .. }),
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

        let type_expr = &field.type_expr.as_ref().expect("expected type expr").expr;
        assert!(
            matches!(type_expr, TypeExpr::List { .. }),
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
            strip_spans(&field.type_expr.as_ref().expect("expected type expr").expr),
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
        let te = &field.type_expr.as_ref().unwrap().expr;
        assert_eq!(te.attrs().len(), 1);
        assert_eq!(te.attrs()[0].name.as_str(), "stream.done");
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
            &field.type_expr.as_ref().unwrap().expr,
            TypeExpr::Union { attrs, .. } if attrs.is_empty()
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
            strip_spans(&field.type_expr.as_ref().expect("expected type expr").expr),
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
            strip_spans(&field.type_expr.as_ref().expect("expected type expr").expr),
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
            panic!("expected Block at root, got {:?}", &body.exprs[root]);
        };
        let Expr::Template {
            tag: crate::ast::TemplateTag::Default { elaborated },
            ..
        } = &body.exprs[*tail]
        else {
            panic!(
                "expected untagged Template at tail, got {:?}",
                &body.exprs[*tail]
            );
        };
        let Expr::Binary { op, lhs, rhs } = &body.exprs[*elaborated] else {
            panic!(
                "expected Binary at elaborated root, got {:?}",
                &body.exprs[*elaborated]
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
}
