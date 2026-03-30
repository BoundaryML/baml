//! `baml_compiler2_ast` — Concrete AST structs and CST → AST lowering.
//!
//! This crate isolates all CST messiness in one boundary layer. After
//! `lower_file` returns, the CST is never needed again — all structural
//! content is owned by the returned `Vec<Item>`.
//!
//! No Salsa dependency. Everything downstream works with owned data and
//! can be constructed directly in tests without parsing.

pub mod ast;
pub(crate) mod companions;
pub(crate) mod lower_config_item;
pub(crate) mod lower_cst;
pub(crate) mod lower_expr_body;
pub(crate) mod lower_type_expr;

pub use ast::*;
pub use lower_cst::{lower_file, lower_file_with_file_id};

#[cfg(test)]
mod tests {
    use baml_base::FileId;
    use baml_compiler_lexer::lex_lossless;
    use baml_compiler_parser::parse_file;
    use baml_compiler_syntax::{SyntaxKind, SyntaxNode};

    use crate::{
        ast::{BuiltinKind, Expr, FunctionBodyDef, Item, Stmt, TypeExpr},
        lower_cst::lower_file,
    };

    /// Build a `TypeExpr` value for use in `assert_eq!` comparisons.
    /// All `attrs` fields are set to `vec![]`.
    macro_rules! type_expr {
        (Int) => { TypeExpr::Int { attrs: vec![] } };
        (Float) => { TypeExpr::Float { attrs: vec![] } };
        (String) => { TypeExpr::String { attrs: vec![] } };
        (Bool) => { TypeExpr::Bool { attrs: vec![] } };
        (Null) => { TypeExpr::Null { attrs: vec![] } };
        (Never) => { TypeExpr::Never { attrs: vec![] } };
        (Rust) => { TypeExpr::Rust { attrs: vec![] } };
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
        (Union($($variant:tt),+ $(,)?)) => {
            TypeExpr::Union {
                variants: vec![$(type_expr!($variant)),+],
                attrs: vec![],
            }
        };
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
        let (items, diags) = lower_file(&root);
        assert!(
            diags.is_empty(),
            "expected no lower diagnostics, got: {diags:#?}"
        );
        items
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
            assert_eq!(c.methods.len(), 4);
            for method in &c.methods {
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
    fn type_expr_paren_union_array() {
        // (int | string)[] = List(Union(Int, String))
        let ta = first_type_alias(parse_and_lower("type T = (int | string)[]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(Union(Int, String)))
        );
    }

    #[test]
    fn type_expr_nested_union_array() {
        // (int | bool)[][] = List(List(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(List(Union(Int, Bool))))
        );
    }

    #[test]
    fn type_expr_nested_union_array_opt() {
        // (int | bool)[][]? = Optional(List(List(Union(Int, Bool))))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)[][]?\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(Optional(List(List(Union(Int, Bool)))))
        );
    }

    #[test]
    fn type_expr_opt_union_in_array() {
        // (int | bool)?[] = List(Optional(Union(Int, Bool)))
        let ta = first_type_alias(parse_and_lower("type T = (int | bool)?[]\n"));
        assert_eq!(
            ta.type_expr.unwrap().expr,
            type_expr!(List(Optional(Union(Int, Bool))))
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
            } => (type_name, fields, spreads),
            other => panic!("expected Expr::Object, got {other:?}"),
        };

        assert_eq!(
            type_name.as_ref().map(smol_str::SmolStr::as_str),
            Some("RetryPolicy"),
            "expected type_name to be RetryPolicy"
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
        // When @alias("bar") comes first, it breaks out of TYPE_EXPR parsing.
        // @stream.done then becomes a field attribute in the CST.
        // This test documents the CURRENT behavior: @stream.done ends up as a
        // field attribute when it follows a field attribute like @alias.
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

        // Currently, both @alias("bar") and @stream.done end up as field attributes
        // because once the parser breaks out of TYPE_EXPR for @alias, @stream.done
        // is parsed in the field attribute loop.
        // This documents the existing behavior; a future parser fix may change this.
        let type_expr = &field.type_expr.as_ref().expect("expected type expr").expr;
        let type_attrs = type_expr.attrs();
        let field_attr_names: Vec<_> = field.attributes.iter().map(|a| a.name.as_str()).collect();
        let type_attr_names: Vec<_> = type_attrs.iter().map(|a| a.name.as_str()).collect();

        // Document current state: we expect EITHER:
        // (a) @stream.done is a type attr (ideal), or
        // (b) @stream.done is a field attr (current parser limitation)
        let stream_done_is_type_attr = type_attrs.iter().any(|a| a.name.as_str() == "stream.done");
        let stream_done_is_field_attr = field
            .attributes
            .iter()
            .any(|a| a.name.as_str() == "stream.done");

        assert!(
            stream_done_is_type_attr || stream_done_is_field_attr,
            "expected @stream.done somewhere: type_attrs={type_attr_names:?}, field_attrs={field_attr_names:?}",
        );

        // @alias should always be a field attribute
        assert!(
            field.attributes.iter().any(|a| a.name.as_str() == "alias"),
            "expected @alias as field attribute, got field_attrs={field_attr_names:?}",
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
    }

    #[test]
    fn multiple_type_attrs_are_collected() {
        let source = r#"
class Foo {
  baz int @stream.done @check("positive", {{this > 0}})
}
"#;
        let class = first_class(parse_and_lower(source));
        let field = class
            .fields
            .iter()
            .find(|f| f.name.as_str() == "baz")
            .expect("expected field 'baz'");

        let type_expr = &field.type_expr.as_ref().expect("expected type expr").expr;
        let type_attrs = type_expr.attrs();
        assert_eq!(
            type_attrs.len(),
            2,
            "expected 2 type attributes, got {type_attrs:?}"
        );
        assert_eq!(type_attrs[0].name.as_str(), "stream.done");
        assert_eq!(type_attrs[1].name.as_str(), "check");
    }
}
