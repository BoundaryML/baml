//! Converts UDF config -> full Jinja expression to be run in a single row.
//!
//! # Note on missing features
//! The "missing data is set to zero" feature and the "missing data is reported as missing" feature
//! are purposefully out of scope for this implementation, at least for the time being.

use baml_types::BamlMap;
use minijinja::machinery::ast;

use crate::{
    config::{Constant, Function, OutputExpression},
    eval::CompileContext,
    IntrusiveStack,
};

pub fn compile_returns_to_jinja<'udf>(
    udf: &'udf crate::config::UDFConfig,
    ctx: &mut CompileContext<'udf>,
) -> (Vec<String>, BamlMap<&'udf str, minijinja::Error>) {
    let (strings, maps): (Vec<_>, Vec<_>) = ctx
        .outputs
        .iter()
        .map(|out| compile_return_to_jinja(out, &udf.global_constants, &udf.functions))
        .unzip();

    let maps = maps.into_iter().fold(BamlMap::new(), |mut acc, errors| {
        acc.extend(errors);
        acc
    });

    (strings, maps)
}

#[cfg(test)]
mod tests {

    use crate::yaml2jinja::compile_returns_to_jinja;
    use crate::{
        config::gather_all_outputs,
        eval::{eval_return, CompileContext, FunctionResults},
        tests::{data, load_sample_udf},
    };

    #[test]
    fn match_override_tree() {
        let udf = load_sample_udf();

        let mock_data = [
            data::openai(),
            data::anthropic(),
            data::gemini(),
            data::none_match(),
        ];

        let mut results: [_; 4] = std::array::from_fn(|_| FunctionResults::default());

        let outputs = gather_all_outputs(&udf);

        let mut ctx = CompileContext::with_outputs(&outputs);

        let (return_exprs, jinja_errors) = compile_returns_to_jinja(&udf, &mut ctx);

        assert!(
            jinja_errors.is_empty(),
            "Encountered parse errors: {jinja_errors:?}"
        );

        for (out, result) in ctx.outputs.iter().zip(return_exprs) {
            match ctx.compile_expression_for_return(out, &result) {
                Err(_) => {
                    for result_map in results.iter_mut() {
                        result_map.has_compile_errors.push(out);
                    }
                }
                Ok(expr) => {
                    for (result_map, data) in results.iter_mut().zip(mock_data.iter()) {
                        let result = expr.eval(data);

                        if result.as_ref().is_ok_and(minijinja::Value::is_undefined) {
                            result_map.not_defined.push(out);
                        } else {
                            let missing_values = Vec::new();
                            eval_return(data, &mut result_map.defined, out, missing_values, &expr);
                        }
                    }
                }
            }
        }

        let mut eval_cctx = CompileContext::with_outputs(&outputs);
        let eval_results: Box<_> = mock_data
            .iter()
            .map(|data| {
                crate::eval::match_and_compute_row(
                    &udf,
                    serde_json::to_value(data).unwrap(),
                    &mut eval_cctx,
                )
                .unwrap()
            })
            .collect();

        for (eval_res, res) in eval_results.iter().zip(&results) {
            assert_eq!(&eval_res.not_defined, &res.not_defined);
            for (k, eval_v) in &eval_res.defined {
                let v = res
                    .defined
                    .get(k)
                    .expect("should have the same available computations");

                // NOTE: not testing for equal missing values because YAML->Jinja does not support
                // support them.
                assert_eq!(eval_v.result.as_ref().unwrap(), v.result.as_ref().unwrap());
            }
        }

        insta::assert_debug_snapshot!(results);
    }
}

#[derive(Clone, Copy)]
enum ReturnStatus<'a> {
    Undefined,
    Defined(&'a OutputExpression),
}

fn search_intrusive_stack<'a, T, B>(
    mut int: &'a IntrusiveStack<'a, T>,
    mut search_fn: impl FnMut(&'a T) -> Option<B>,
) -> Option<B> {
    loop {
        if let Some(ok) = search_fn(&int.cur) {
            return Some(ok);
        }

        int = int.prev?;
    }
}
type IntrusiveMapStack<'a, 'src> = IntrusiveStack<'a, &'src BamlMap<String, Constant>>;

/// Compiles a return to a jinja expression that will run on an input and will match it to yield
/// the correct result.
fn compile_return_to_jinja<'a>(
    return_name: &str,
    globals: &BamlMap<String, Constant>,
    functions: &'a [Function],
) -> (String, BamlMap<&'a str, minijinja::Error>) {
    let stack_top = IntrusiveStack {
        prev: None,
        cur: globals,
    };
    let mut jinja_errors = BamlMap::new();
    let mut result = compile_override_match_open(
        return_name,
        ReturnStatus::Undefined,
        Some(&stack_top),
        functions,
        &mut jinja_errors,
    );

    // result = `X if cond1 else Y if cond2 else ... xN if condN else`
    // So it's missing `undefined` since it's not defined if it does not match.

    result += " undefined";

    (result, jinja_errors)
}

// wraps code such that we end up with a ternary if-chain in the final jinja expression
// Named `_open` because it is designed to be recursive: the expression ends in an open `else`
// NOTE: could make better use of a `mut String` to avoid allocations if necessary.
fn compile_override_match_open<'a>(
    return_name: &str,
    status: ReturnStatus<'a>,
    map_stack: Option<&IntrusiveMapStack>,
    functions: &'a [Function],
    jinja_errors: &mut BamlMap<&'a str, minijinja::Error>,
) -> String {
    let mut result = String::new();
    for func in functions {
        let status_for_func = func
            .returns
            .get(return_name)
            .map(ReturnStatus::Defined)
            .unwrap_or(status);

        let map_stack_for_func = IntrusiveMapStack {
            prev: map_stack,
            cur: &func.constants,
        };

        result += "(";

        // wrap overrides first, which will have "X if condA else Y if condB else" (and end
        // in open 'else')
        if !func.overrides.is_empty() {
            result += &compile_override_match_open(
                return_name,
                status_for_func,
                Some(&map_stack_for_func),
                &func.overrides,
                jinja_errors,
            );
        }

        match status_for_func {
            ReturnStatus::Undefined => result += "undefined",
            ReturnStatus::Defined(expr) => {
                match minijinja::machinery::parse_expr(&expr.0) {
                    Ok(ast) => {
                        // NOTE: (Jesus) It would've been easier if we could just walk the instructions...
                        // Then we would just replace Lookup() whenever required :]
                        // We also can't get from ast::Expr to Expression because Expression::new
                        // (used by Environment::compile_expression) is not exported...
                        let rebuilt_expr =
                            rebuild_with_known_constants(&ast, Some(&map_stack_for_func));
                        result += &rebuilt_expr;
                    }
                    Err(e) => {
                        jinja_errors.insert(&expr.0, e);
                        result += "undefined";
                    }
                }
            }
        };

        result += ")";

        result += " if (";
        result += &func.match_expr.0;
        result += ") else ";
    }

    result
}

pub trait SearchBy<'a, K, V> {
    fn search(&'a self, name: K) -> Option<&'a V>;
}

impl<'a, 'k, K, V, Q> SearchBy<'a, &'k Q, V> for BamlMap<K, V>
where
    Q: indexmap::Equivalent<K> + std::hash::Hash + ?Sized,
    V: Sized,
{
    fn search(&'a self, name: &'k Q) -> Option<&'a V> {
        self.get(name)
    }
}

impl<'a, K, V, T> SearchBy<'a, K, V> for IntrusiveStack<'a, T>
where
    T: SearchBy<'a, K, V>,
    K: Copy,
{
    fn search(&'a self, name: K) -> Option<&'a V> {
        search_intrusive_stack(self, |map| map.search(name))
    }
}

impl<'a, K, V, T> SearchBy<'a, K, V> for &'a T
where
    T: SearchBy<'a, K, V>,
{
    fn search(&'a self, name: K) -> Option<&'a V> {
        SearchBy::search(*self, name)
    }
}

fn search_constant<'a, 'k>(
    map_stack: Option<&'a dyn SearchBy<'a, &'k str, Constant>>,
    name: &'k str,
) -> Option<&'a Constant> {
    map_stack.and_then(|map| map.search(name))
}

pub fn rebuild_with_known_constants<'search, 'src>(
    expr: &ast::Expr<'src>,
    map_stack: Option<&'search dyn SearchBy<'search, &'src str, Constant>>,
) -> String {
    fn rebuild_args<'search, 'src>(
        map_stack: Option<&'search dyn SearchBy<'search, &'src str, Constant>>,
        args: &Vec<minijinja::machinery::ast::CallArg<'src>>,
    ) -> String {
        args.iter()
            .map(|arg| match arg {
                ast::CallArg::Pos(expr) => rebuild_with_known_constants(expr, map_stack),
                ast::CallArg::Kwarg(id, expr) => {
                    format!("{id}={}", rebuild_with_known_constants(expr, map_stack))
                }
                ast::CallArg::PosSplat(expr) => {
                    format!("*{}", rebuild_with_known_constants(expr, map_stack))
                }
                ast::CallArg::KwargSplat(expr) => {
                    format!("**{}", rebuild_with_known_constants(expr, map_stack))
                }
            })
            .collect::<Vec<String>>()
            .join(",")
    }

    match expr {
        ast::Expr::Var(spanned) => match search_constant(map_stack, spanned.id) {
            Some(Constant(value)) => format!("{value}"),
            None => spanned.id.to_owned(),
        },
        ast::Expr::Const(spanned) => format!("{:?}", spanned.value),
        ast::Expr::Slice(spanned) => {
            let start = spanned
                .start
                .as_ref()
                .map(|e| rebuild_with_known_constants(e, map_stack))
                .unwrap_or_else(String::new);
            let stop = spanned
                .stop
                .as_ref()
                .map(|e| rebuild_with_known_constants(e, map_stack))
                .unwrap_or_else(String::new);
            let step = spanned
                .step
                .as_ref()
                .map(|e| format!(":{}", rebuild_with_known_constants(e, map_stack)))
                .unwrap_or_else(String::new);

            format!(
                "{}[{start}:{stop}{step}]",
                rebuild_with_known_constants(&spanned.expr, map_stack)
            )
        }
        ast::Expr::UnaryOp(spanned) => {
            let op_char = match spanned.op {
                ast::UnaryOpKind::Not => '!',
                ast::UnaryOpKind::Neg => '-',
            };

            format!(
                "({}{})",
                op_char,
                rebuild_with_known_constants(&spanned.expr, map_stack)
            )
        }
        ast::Expr::BinOp(spanned) => {
            let op_str = match spanned.op {
                ast::BinOpKind::Eq => "==",
                ast::BinOpKind::Ne => "!=",
                ast::BinOpKind::Lt => "<",
                ast::BinOpKind::Lte => "<=",
                ast::BinOpKind::Gt => ">",
                ast::BinOpKind::Gte => ">=",
                ast::BinOpKind::ScAnd => "&&",
                ast::BinOpKind::ScOr => "||",
                ast::BinOpKind::Add | ast::BinOpKind::Concat => "+",
                ast::BinOpKind::Sub => "-",
                ast::BinOpKind::Mul => "*",
                ast::BinOpKind::Div => "/",
                ast::BinOpKind::FloorDiv => "//",
                ast::BinOpKind::Rem => "%",
                ast::BinOpKind::Pow => "**",
                ast::BinOpKind::In => "in",
            };

            let left = rebuild_with_known_constants(&spanned.left, map_stack);
            let right = rebuild_with_known_constants(&spanned.right, map_stack);

            format!("({left} {op_str} {right})")
        }
        ast::Expr::IfExpr(spanned) => {
            let test = rebuild_with_known_constants(&spanned.test_expr, map_stack);
            let true_branch = rebuild_with_known_constants(&spanned.true_expr, map_stack);
            let false_branch = spanned
                .false_expr
                .as_ref()
                .map(|ex| rebuild_with_known_constants(ex, map_stack))
                .unwrap_or_else(|| "undefined".into());

            format!("({true_branch} if {test} else {false_branch})")
        }
        ast::Expr::Filter(spanned) => {
            let args = rebuild_args(map_stack, &spanned.args);

            let expr = rebuild_with_known_constants(
                spanned
                    .expr
                    .as_ref()
                    .expect("filter expression must have a child expression"),
                map_stack,
            );

            format!("({expr} | {name}{args})", name = spanned.name)
        }
        ast::Expr::Test(spanned) => {
            let has_args = !spanned.args.is_empty();

            let expr = rebuild_with_known_constants(&spanned.expr, map_stack);
            let name = spanned.name;

            if has_args {
                let args = rebuild_args(map_stack, &spanned.args);

                format!("({expr} is {name}({args}))")
            } else {
                format!("({expr} is {name})")
            }
        }
        ast::Expr::GetAttr(spanned) => {
            format!(
                "{inner}.{name}",
                inner = rebuild_with_known_constants(&spanned.expr, map_stack),
                name = spanned.name
            )
        }
        ast::Expr::GetItem(spanned) => {
            format!(
                "{inner}[{index}]",
                inner = rebuild_with_known_constants(&spanned.expr, map_stack),
                index = rebuild_with_known_constants(&spanned.subscript_expr, map_stack),
            )
        }
        ast::Expr::Call(spanned) => {
            format!(
                "{inner}({args})",
                inner = rebuild_with_known_constants(&spanned.expr, map_stack),
                args = rebuild_args(map_stack, &spanned.args),
            )
        }
        ast::Expr::List(spanned) => {
            let args = spanned
                .items
                .iter()
                .map(|e| rebuild_with_known_constants(e, map_stack))
                .collect::<Vec<_>>()
                .join(", ");

            format!("[{args}]")
        }
        ast::Expr::Map(spanned) => {
            let keys = spanned
                .keys
                .iter()
                .map(|e| rebuild_with_known_constants(e, map_stack));
            let values = spanned
                .values
                .iter()
                .map(|e| rebuild_with_known_constants(e, map_stack));

            let pairs = keys
                .zip(values)
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(", ");

            format!("{{{pairs}}}")
        }
    }
}
