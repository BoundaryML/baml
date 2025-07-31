//! Converts UDF config -> full Jinja expression to be run in a single row.
//!
//! # Note on missing features
//! The "missing data is set to zero" feature and the "missing data is reported as missing" feature
//! are purposefully out of scope for this implementation, at least for the time being.

use std::collections::HashMap;

use baml_types::BamlMap;
use minijinja::machinery::ast;

use crate::{
    config::{Constant, Function, OutputExpression},
    eval::CompileContext,
    HashByPtr, IntrusiveStack,
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

pub fn rebuild_with_known_constants<'search, 'src>(
    expr: &ast::Expr<'src>,
    constants: &'search dyn SearchBy<'search, &'src str, Constant>,
) -> String {
    let flattened = PreOrderTraversal::from_root(expr, |arg| match arg {
        ast::CallArg::Pos(expr)
        | ast::CallArg::Kwarg(_, expr)
        | ast::CallArg::PosSplat(expr)
        | ast::CallArg::KwargSplat(expr) => Some(expr),
    });

    rebuild_from_flattened(&flattened, constants)
}

#[cfg(test)]
mod tests {

    use crate::{
        config::gather_all_outputs,
        eval::{eval_return, CompileContext, FunctionResults},
        tests::{data, load_sample_udf},
        yaml2jinja::compile_returns_to_jinja,
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
                        let rebuilt_expr = rebuild_with_known_constants(&ast, &map_stack_for_func);
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

// NOTE: could use a simpler generic set with search(self, name: K) -> Option<V>.
/// Trait for searching a map-like structure by key. Used to implement searching for
/// IntrusiveStack.
pub trait SearchBy<'a, K, V> {
    fn search(&'a self, name: K) -> Option<&'a V>;
}

#[derive(Clone, Copy)]
pub struct NullSearch;

impl<'a, K, V> SearchBy<'a, K, V> for NullSearch {
    fn search(&'a self, _name: K) -> Option<&'a V> {
        None
    }
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

/// Contains a pre-order traversal of a Jinja AST, in linear form. This allows for easy (and fast!) traversal in
/// either pre-order or post-order (iterate in reverse).
#[derive(Clone, Debug)]
pub struct PreOrderTraversal<'ast, 'src>(pub Vec<&'ast ast::Expr<'src>>);

impl<'ast, 'src> PreOrderTraversal<'ast, 'src> {
    pub fn from_root(
        ast: &'ast ast::Expr<'src>,
        mut arg_filter: impl FnMut(&'ast ast::CallArg<'src>) -> Option<&'ast ast::Expr<'src>>,
    ) -> Self {
        Self::from_root_dyn_impl(ast, &mut arg_filter)
    }

    pub fn preorder(&self) -> impl Iterator<Item = &'ast ast::Expr<'src>> + '_ {
        self.0.iter().copied()
    }

    pub fn preorder_mut<'s>(
        &'s mut self,
    ) -> impl Iterator<Item = &'s mut &'ast ast::Expr<'src>> + 's {
        self.0.iter_mut()
    }

    pub fn postorder(&self) -> impl Iterator<Item = &'ast ast::Expr<'src>> + '_ {
        self.0.iter().rev().copied()
    }

    pub fn postorder_mut<'s>(
        &'s mut self,
    ) -> impl Iterator<Item = &'s mut &'ast ast::Expr<'src>> + 's {
        self.0.iter_mut().rev()
    }

    // NOTE: (Jesus) this function uses `dyn` to avoid explosion of code due to monomorphization.
    // Not concerned with performance difference, since the objective of using this is to do this expensive
    // traversal once and do the rest by iterating over the Vec.
    fn from_root_dyn_impl(
        ast: &'ast ast::Expr<'src>,
        mut arg_filter: &mut dyn FnMut(&'ast ast::CallArg<'src>) -> Option<&'ast ast::Expr<'src>>,
    ) -> Self {
        let mut stack = Vec::new();
        let mut top = ast;

        let mut result = Vec::new();

        loop {
            result.push(top);

            use ast::Expr::*;
            top = match top {
                Var(_) | Const(_) => match stack.pop() {
                    Some(expr) => expr,
                    None => break,
                },
                Slice(spanned) => {
                    stack.extend(spanned.step.as_ref());
                    stack.extend(spanned.stop.as_ref());
                    stack.extend(spanned.start.as_ref());
                    match stack.pop() {
                        Some(expr) => expr,
                        None => break,
                    }
                }
                UnaryOp(spanned) => &spanned.expr,
                BinOp(spanned) => {
                    stack.push(&spanned.right);
                    &spanned.left
                }
                IfExpr(spanned) => {
                    stack.extend(spanned.false_expr.as_ref());
                    stack.push(&spanned.true_expr);
                    &spanned.test_expr
                }
                Filter(spanned) => {
                    stack.extend(spanned.args.iter().filter_map(&mut arg_filter));
                    spanned
                        .expr
                        .as_ref()
                        .expect("filter expression must have a child expression")
                }
                Test(spanned) => {
                    stack.extend(spanned.args.iter().filter_map(&mut arg_filter));
                    &spanned.expr
                }
                GetAttr(spanned) => &spanned.expr,
                GetItem(spanned) => {
                    stack.push(&spanned.subscript_expr);
                    &spanned.expr
                }
                Call(spanned) => {
                    stack.extend(spanned.args.iter().filter_map(&mut arg_filter));
                    &spanned.expr
                }
                List(spanned) => {
                    stack.extend(&spanned.items);
                    match stack.pop() {
                        Some(expr) => expr,
                        None => break,
                    }
                }
                Map(spanned) => {
                    stack.extend(&spanned.values);
                    stack.extend(&spanned.keys);
                    match stack.pop() {
                        Some(expr) => expr,
                        None => break,
                    }
                }
            };
        }

        Self(result)
    }
}

impl<'ast, 'src> core::ops::DerefMut for PreOrderTraversal<'ast, 'src> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'ast, 'src> core::ops::Deref for PreOrderTraversal<'ast, 'src> {
    type Target = [&'ast ast::Expr<'src>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn rebuild_from_flattened<'search, 'src>(
    flattened: &PreOrderTraversal<'_, 'src>,
    constants: &'search dyn SearchBy<'search, &'src str, Constant>,
) -> String {
    // NOTE: using a hashmap to allow for arbitrary order of access.
    let mut result_map = HashMap::new();

    fn extract<'ast, 'src>(
        result_map: &mut HashMap<HashByPtr<'ast, ast::Expr<'src>>, String>,
        key: &'ast ast::Expr<'src>,
    ) -> String {
        result_map
            .remove(&HashByPtr(key))
            .expect("should have compiled")
    }

    fn rebuild_args<'search, 'ast, 'src>(
        result_map: &'search mut HashMap<HashByPtr<'ast, ast::Expr<'src>>, String>,
        args: &'ast [ast::CallArg<'src>],
    ) -> String {
        args.iter()
            .map(|arg| match arg {
                ast::CallArg::Pos(expr) => extract(result_map, expr),
                ast::CallArg::Kwarg(id, expr) => {
                    format!("{id}={}", extract(result_map, expr))
                }
                ast::CallArg::PosSplat(expr) => format!("*{}", extract(result_map, expr)),
                ast::CallArg::KwargSplat(expr) => format!("**{}", extract(result_map, expr)),
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    for expr in flattened.postorder() {
        use ast::Expr::*;
        let expr_result = match expr {
            Var(spanned) => match constants.search(spanned.id) {
                Some(Constant(value)) => value.to_string(),
                None => spanned.id.to_string(),
            },
            Const(spanned) => format!("{:?}", spanned.value),
            Slice(spanned) => {
                let start = spanned
                    .start
                    .as_ref()
                    .map_or_else(String::new, |e| extract(&mut result_map, e));

                let stop = spanned
                    .stop
                    .as_ref()
                    .map_or_else(String::new, |e| extract(&mut result_map, e));

                let step = spanned
                    .step
                    .as_ref()
                    .map_or_else(String::new, |e| format!(":{}", extract(&mut result_map, e)));

                let inner = extract(&mut result_map, &spanned.expr);

                format!("{inner}[{start}:{stop}{step}]")
            }
            UnaryOp(spanned) => {
                let op_char = match spanned.op {
                    ast::UnaryOpKind::Not => '!',
                    ast::UnaryOpKind::Neg => '-',
                };

                let inner = extract(&mut result_map, &spanned.expr);

                format!("({op_char}{inner})")
            }
            BinOp(spanned) => {
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

                let left = extract(&mut result_map, &spanned.left);
                let right = extract(&mut result_map, &spanned.right);

                format!("({left} {op_str} {right})")
            }
            IfExpr(spanned) => {
                let test = extract(&mut result_map, &spanned.test_expr);
                let true_branch = extract(&mut result_map, &spanned.true_expr);
                let false_branch = spanned.false_expr.as_ref().map_or_else(
                    || "undefined".to_string(),
                    |ex| extract(&mut result_map, ex),
                );

                format!("({true_branch} if {test} else {false_branch})")
            }
            Filter(spanned) => {
                let args = rebuild_args(&mut result_map, &spanned.args);

                let expr = spanned
                    .expr
                    .as_ref()
                    .expect("filter expression must have a child expression");

                let inner = extract(&mut result_map, expr);

                format!("({inner} | {name}{args})", name = spanned.name)
            }
            Test(spanned) => {
                let has_args = !spanned.args.is_empty();
                let args = rebuild_args(&mut result_map, &spanned.args);
                let expr = extract(&mut result_map, &spanned.expr);

                if has_args {
                    format!("({expr} is {name}({args}))", name = spanned.name)
                } else {
                    format!("({expr} is {name})", name = spanned.name)
                }
            }
            GetAttr(spanned) => {
                let inner = extract(&mut result_map, &spanned.expr);
                format!("{inner}.{name}", name = spanned.name)
            }
            GetItem(spanned) => {
                let inner = extract(&mut result_map, &spanned.expr);
                let index = extract(&mut result_map, &spanned.subscript_expr);
                format!("{inner}[{index}]")
            }
            Call(spanned) => {
                let args = rebuild_args(&mut result_map, &spanned.args);
                let inner = extract(&mut result_map, &spanned.expr);
                format!("{inner}({args})")
            }
            List(spanned) => {
                let args = spanned
                    .items
                    .iter()
                    .map(|e| extract(&mut result_map, e))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("[{args}]")
            }
            Map(spanned) => {
                let pairs = spanned
                    .keys
                    .iter()
                    .zip(&spanned.values)
                    .map(|(k, v)| (extract(&mut result_map, k), extract(&mut result_map, v)))
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{{{pairs}}}")
            }
        };

        result_map.insert(HashByPtr(expr), expr_result);
    }

    assert_eq!(
        result_map.len(),
        1,
        "All other expressions but root should have been consumed",
    );

    extract(&mut result_map, flattened[0])
}
