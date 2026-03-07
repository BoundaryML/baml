//! `baml_compiler2_ast` — Concrete AST structs and CST → AST lowering.
//!
//! This crate isolates all CST messiness in one boundary layer. After
//! `lower_file` returns, the CST is never needed again — all structural
//! content is owned by the returned `Vec<Item>`.
//!
//! No Salsa dependency. Everything downstream works with owned data and
//! can be constructed directly in tests without parsing.

pub mod ast;
pub(crate) mod lower_cst;
pub(crate) mod lower_expr_body;
pub(crate) mod lower_type_expr;

pub use ast::*;
pub use lower_cst::lower_file;

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
            TypeExpr::Path(vec![
                baml_base::Name::new("baml"),
                baml_base::Name::new("errors"),
                baml_base::Name::new("Io"),
            ])
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
            .filter_map(|e| e.into_token())
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
                    .filter_map(|e| e.into_token())
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
                TypeExpr::Rust => {}
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
                    Some(TypeExpr::Rust)
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
            matches!(throws.expr, TypeExpr::Never),
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
}
