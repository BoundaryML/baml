//! Manual Rust evaluator for UDFs defined in YAML.
//!
//! ## Problem with `if` branches
//! Consider the Jinja expression:
//! ```jinja
//! raw.output_tokens_details.cached_tokens if raw.output_token_details else 1
//! ```
//!
//! Analysis from `minijinja` yields that the following paths are not statically known (they are not `set` variables):
//! ```notrust
//! raw.output_tokens_details
//! raw.output_tokens_details.cached_tokens
//! ```
//!
//! We currently only have the last piece of information: we don't know how they are used, since
//! we're not doing any manual AST analysis. We find out that `raw` does not have the field
//! `output_tokens_details`.  Since we have to set `cached_tokens` to zero, we conservatively set
//! `raw.output_tokens_details` to a map, ending with `raw.output_tokens_details = { cached_tokens
//! =  0 }`. This makes `raw.output_tokens_details` not an empty map, and thus the `if
//! raw.output_token_details` branch yields the wrong value.
//!

mod path_trie;

use baml_types::BamlMap;
use indexmap::IndexSet;
use path_trie::PathTrie;
use serde::Serialize;

use crate::{
    config::{Constant, Function, OutputExpression, UDFConfig},
    get_env,
};

pub fn match_and_compute_row<'src>(
    udf: &'src UDFConfig,
    row: serde_json::Value,
    context: &mut CompileContext<'src>,
) -> anyhow::Result<FunctionResults<'src>> {
    Ok(match find_function_for_row(udf, &row)? {
        Some(result) => eval_function(result, row, context),
        None => FunctionResults {
            has_compile_errors: Vec::new(),
            defined: BamlMap::new(),
            not_defined: context.outputs.iter().copied().collect(),
        },
    })
}

fn eval_function<'src>(
    ev: MatchedFunction<'src>,
    row: serde_json::Value,
    context: &mut CompileContext<'src>,
) -> FunctionResults<'src> {
    let (defined, compile_errors) = eval_existing_returns(ev, row);

    let not_defined: Vec<_> = context
        .outputs
        .iter()
        .copied()
        .filter(|&key| !(defined.contains_key(key) || compile_errors.contains_key(key)))
        .collect();

    let has_compile_errors = compile_errors.iter().map(|x| *x.0).collect();

    context.compile_errors.extend(compile_errors);

    FunctionResults {
        has_compile_errors,
        defined,
        not_defined,
    }
}

#[derive(Debug)]
pub struct DefinedResult {
    pub missing_values: Vec<String>,
    pub result: Result<f64, minijinja::Error>,
}

#[derive(Debug)]
pub struct CompileContext<'udf> {
    /// For returns that have compile errors, the errors that we could find.
    pub compile_errors: BamlMap<&'udf str, minijinja::Error>,
    pub env: minijinja::Environment<'udf>,
    pub outputs: &'udf IndexSet<&'udf str>,
}

impl<'udf> CompileContext<'udf> {
    /// `outputs` can be gathered using [`crate::config::gather_all_outputs`]
    pub fn with_outputs(outputs: &'udf IndexSet<&'udf str>) -> Self {
        Self {
            env: {
                let mut env = get_env();
                env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
                env
            },
            compile_errors: Default::default(),
            outputs,
        }
    }
}

/// There was a compile error, and it has been registered to the specified return
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct RegisteredCompilerError;

impl<'udf> CompileContext<'udf> {
    pub fn compile_expression_for_return<'env, 'src>(
        &'env mut self,
        ret_name: &'udf str,
        source: &'src str,
    ) -> Result<minijinja::Expression<'env, 'src>, RegisteredCompilerError>
    where
        'udf: 'src,
    {
        self.env.compile_expression(source).map_err(|e| {
            self.compile_errors.insert(ret_name, e);
            RegisteredCompilerError
        })
    }
}

#[derive(Debug, Default)]
pub struct FunctionResults<'udf> {
    /// Set of outputs that have compile errors.
    pub has_compile_errors: Vec<&'udf str>,
    pub defined: BamlMap<&'udf str, DefinedResult>,
    /// List of accessors (e.g `raw.output_tokens.count`) that were not defined when running the
    /// functions.
    pub not_defined: Vec<&'udf str>,
}

#[derive(Debug, Default)]
struct MatchedFunction<'a> {
    // NOTE: (Jesus) If required, a reference to the original provider + override map could be used.
    // Override map is required because many overrides can be merged. Otherwise we could have just
    // a reference to the provider and an optional reference to *the* selected override. This
    // would remove a substantial amount of copies, taking into account that right now we're
    // cloning *all* of the strings.
    // Summary:
    // - Original provider hashmap can be reference.
    // - HashMap can be Cow at least (ref + optional ref would be best, but I don't think
    // that's possible)
    // - Constant & OutputExpression can be references! Instead of &str they would be
    // minijinja::Expression because those are handles too.
    constants: BamlMap<&'a str, Constant>,
    returns: BamlMap<&'a str, OutputExpression>,
}

/// Evaluate selected outputs of function
fn eval_existing_returns<'src>(
    ev: MatchedFunction<'src>,
    mut serialized_data: serde_json::Value,
) -> (
    BamlMap<&'src str, DefinedResult>,
    BamlMap<&'src str, minijinja::Error>,
) {
    let mut env = get_env();

    // semi strict: fail for everything undefined, except when using `if x.y`, which evaluates
    // the `false` branch.
    env.set_undefined_behavior(minijinja::UndefinedBehavior::SemiStrict);

    let mut result_map = BamlMap::new();
    let mut errors_map = BamlMap::new();

    {
        let env_map = serialized_data
            .as_object_mut()
            .expect("serialized data context must come in the form of a dictionary");

        // add constants to env
        for (name, value) in ev.constants {
            env_map.insert(name.into(), serde_json::to_value(value.0).unwrap());
        }
    }

    // we'll exploit the fact that `returns` uses IndexMap multiple times here.

    // compile all expressions to a vec, since we're going to do two passes over them. We'll
    // know which is which because of IndexMap's consistent iteration order.
    let compiled_expressions: Vec<_> = ev
        .returns
        .values()
        .map(|src| env.compile_expression(&src.0))
        .collect();

    // build a trie of all the variable paths that are not directly known (e.g raw.outputs.x.y) for all expressions,
    // registering the return that uses them.
    let mut path_trie = PathTrie::default();
    // somewhere to store the strings that the trie is going to reference.
    let mut string_stash = Vec::new();
    let mut ranges = Vec::with_capacity(compiled_expressions.len());

    // store all the undefined strings into a stash, since otherwise we can't fullfill the lifetime
    // requirement for `PathTrie`.
    for expr in &compiled_expressions {
        let start = string_stash.len();

        if let Ok(e) = expr {
            string_stash.extend(e.undeclared_variables(true));
        }

        ranges.push(start..string_stash.len());
    }
    for (return_index, range) in ranges.into_iter().enumerate() {
        let ret_order = path_trie::ReturnIterationOrder(return_index);

        for und in &string_stash[range] {
            {
                path_trie.insert(und, ret_order);
            };
        }
    }

    let mut undefined_lists: Vec<_> = std::iter::repeat_with(Vec::new)
        .take(compiled_expressions.len())
        .collect();

    path_trie::zero_undefined_values(&path_trie, &mut serialized_data, &mut undefined_lists);

    // execute the expressions.
    for ((&name, expr), missing_values) in ev
        .returns
        .keys()
        .zip(compiled_expressions)
        .zip(undefined_lists)
    {
        // when testing, make sure that the entries have consistent order.
        #[cfg(test)]
        let missing_values = {
            let mut m = missing_values;
            m.sort();
            m
        };
        let expr = match expr {
            Ok(e) => e,
            Err(err) => {
                errors_map.insert(name, err);
                continue;
            }
        };

        eval_return(
            &serialized_data,
            &mut result_map,
            name,
            missing_values,
            &expr,
        );
    }
    (result_map, errors_map)
}

// This is going to copy-paste on each different `T`. It's okay as it's very little code.
/// Evaluate a compiled expression for a single return, and add it onto the defined result map if
/// it can extract an `f64` out of it, or if it has a runtime error.
pub fn eval_return<'udf, 'src, T: Serialize>(
    serialized_row: &T,
    result_map: &mut BamlMap<&'udf str, DefinedResult>,
    name: &'udf str,
    missing_values: Vec<String>,
    expr: &minijinja::Expression<'_, 'src>,
) {
    let result = expr
        .eval(serialized_row)
        .map_or_else(|e| Ok(Err(e)), |v| f64::try_from(v).map(Ok));

    match result {
        Ok(res) => {
            result_map.insert(
                name,
                DefinedResult {
                    missing_values,
                    result: res,
                },
            );
        }
        Err(e) => {
            // NOTE: (Jesus) By ignoring the result, we are purposefully not registering the return
            // as defined, i.e we're setting the result value to undefined. We could use our own
            // error variant/ anyhow for this too, but I think emitting a warn is the desirable
            // output anyway.
            log::warn!("function result is not coercible to floating point: {e}");
        }
    }
}

fn find_function_for_row<'c>(
    config: &'c UDFConfig,
    row: &serde_json::Value,
) -> anyhow::Result<Option<MatchedFunction<'c>>> {
    use anyhow::Context;
    let mut env = get_env();

    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);

    fn eval_match(
        src: &str,
        env: &minijinja::Environment,
        row: &serde_json::Value,
    ) -> anyhow::Result<bool> {
        let expr = env
            .compile_expression(src)
            .context("compiling match expression")?;

        let result = match expr.eval(row) {
            Ok(res) => res.is_true(),
            Err(e) => {
                // ignore error & assume false
                log::warn!("error when evaluating match expression: {}", e);
                false
            }
        };

        Ok(result)
    }

    fn find_child<'a>(
        children: &'a [Function],
        env: &minijinja::Environment,
        row: &serde_json::Value,
    ) -> anyhow::Result<Option<&'a Function>> {
        children
            .iter()
            .find_map(|func| {
                eval_match(&func.match_expr.0, env, row)
                    .map(|x| x.then_some(func))
                    .transpose()
            })
            .transpose()
    }

    // we'll use a BFS traversal where we first examine all the immediate children, then follow
    // with the rest if there are more.

    let Some(mut cur) = find_child(&config.functions, &env, row)? else {
        return Ok(None);
    };

    let mut eval = MatchedFunction {
        constants: {
            let s = config
                .global_constants
                .iter()
                .map(|(a, b)| (a.as_ref(), b.clone()));
            let s = s.chain(cur.constants.iter().map(|(a, b)| (a.as_ref(), b.clone())));
            s.collect()
        },
        returns: cur
            .returns
            .iter()
            .map(|(k, v)| (k.as_ref(), v.clone()))
            .collect(),
    };

    // keep iterating until we have no more matches for overrides.
    loop {
        let next_child = find_child(&cur.overrides, &env, row)?;
        match next_child {
            None => break,
            Some(child) => {
                eval.constants
                    .extend(child.constants.iter().map(|(a, b)| (a.as_ref(), b.clone())));
                eval.returns
                    .extend(child.returns.iter().map(|(a, b)| (a.as_ref(), b.clone())));
                cur = child;
            }
        }
    }

    Ok(Some(eval))
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        config::gather_all_outputs,
        tests::{data, load_sample_udf},
    };

    #[test]
    fn parse_yaml_file() {
        let deser = load_sample_udf();

        insta::assert_yaml_snapshot!(deser);
    }

    #[test]
    fn find_all_outputs() {
        let udf = load_sample_udf();

        let result = gather_all_outputs(&udf);

        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn compute_jinja_matches() {
        let udf = load_sample_udf();

        let mock = [
            // this one has everything
            data::openai(),
            // this one doesn't have one of the returns (since only openai defines it)
            data::anthropic(),
            // this one won't have any returns, so both should be undefined.
            data::none_match(),
            // this one will have returns, but they should have detected zeroes.
            data::anthropic_with_bad_raw(),
        ];

        let all_names = gather_all_outputs(&udf);

        let mut context = CompileContext::with_outputs(&all_names);

        let match_res = mock.map(|mock| {
            match_and_compute_row(&udf, serde_json::to_value(mock).unwrap(), &mut context).unwrap()
        });

        insta::assert_debug_snapshot!(match_res);
    }

    #[test]
    fn match_functions() {
        let udf = load_sample_udf();

        let mock = [
            data::openai(),
            data::gemini(),
            data::anthropic(),
            data::none_match(),
        ];

        let results = mock
            .map(|resp| find_function_for_row(&udf, &serde_json::to_value(resp).unwrap()).unwrap());

        insta::assert_debug_snapshot!(results);
    }
}
