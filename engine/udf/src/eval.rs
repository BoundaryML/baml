//! Manual Rust evaluator for UDFs defined in YAML.
//!
//! ## Problem with `if` branches
//! Consider the Jinja expression:  ```jinja
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

use baml_types::BamlMap;
use indexmap::IndexSet;

use crate::config::{Constant, Function, OutputExpression, UDFConfig};

pub fn match_and_compute_row<'src>(
    udf: &'src UDFConfig,
    row: serde_json::Value,
    all_names: &IndexSet<&'src str>,
) -> anyhow::Result<FunctionResults<'src>> {
    Ok(match find_function_for_row(udf, &row)? {
        Some(result) => eval_function(result, row, all_names),
        None => FunctionResults {
            compile_errors: BamlMap::new(),
            defined: BamlMap::new(),
            not_defined: all_names.iter().copied().collect(),
        },
    })
}

fn eval_function<'src>(
    ev: MatchedFunction<'src>,
    row: serde_json::Value,
    all_names: &IndexSet<&'src str>,
) -> FunctionResults<'src> {
    let (defined, compile_errors) = eval_existing_returns(ev, serde_json::to_value(row).unwrap());

    let not_defined: Vec<_> = all_names
        .iter()
        .copied()
        .filter(|&key| !(defined.contains_key(key) || compile_errors.contains_key(key)))
        .collect();

    FunctionResults {
        compile_errors,
        defined,
        not_defined,
    }
}

/// Adds `date_between` filter to the environment.
fn get_env<'s>() -> minijinja::Environment<'s> {
    let mut env = internal_baml_core::ir::jinja_helpers::get_env();

    env.add_filter("date_between", date_between);
    env
}

fn parse_date(date: &str) -> chrono::ParseResult<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
}

// TODO: (Jesus) use real date type
fn date_between(date: String, begin: String, end: String) -> Result<bool, minijinja::Error> {
    // TODO: (Jesus) handle parse errors?
    // NOTE: (Jesus) This will be parsing `begin` and `end` dates for all rows and `date` for all date
    // comparisons.
    let date = parse_date(&date).unwrap();
    let begin = parse_date(&begin).unwrap();
    let end = parse_date(&end).unwrap();

    Ok(date >= begin && date <= end)
}

// TODO: before sql:
// - 1 single jinja expression
// - 1 statement per return

#[derive(Debug)]
pub struct DefinedResult {
    pub missing_values: Vec<String>,
    pub result: Result<f64, minijinja::Error>,
}

#[derive(Debug)]
pub struct FunctionResults<'udf> {
    // NOTE: (Jesus) compile_errors does not fit very well here, specially when considering
    // matching multiple rows. I'd want to compile the expressions and keep here a set of outputs
    // that have known errors but have the errors pop up elsewhere.
    /// return expressions that failed to compile
    pub compile_errors: BamlMap<&'udf str, minijinja::Error>,
    pub defined: BamlMap<&'udf str, DefinedResult>,
    /// List of accessors (e.g `raw.output_tokens.count`) that were not defined when running the
    /// functions.
    pub not_defined: Vec<&'udf str>,
}

// src/rust_version.rs -> return Option<float> per return, also return None when
// src/all_jinja.rs
// src/clickhouse.rs

#[derive(Debug, Default)]
struct MatchedFunction<'a> {
    // NOTE: (Jesus) If required, a reference to the original provider + override map could be used.
    // Override map is required because many overrides can be merged. Otheriwise we could have just
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

#[cfg(test)]
mod tests {

    use crate::{config::gather_all_outputs, read_udf_config};

    use super::*;

    fn load_sample_udf() -> UDFConfig {
        use anyhow::Context;
        read_udf_config("./sample-prices.yaml")
            .context("load sample UDF config")
            .unwrap()
    }

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

        let match_res = mock.map(|mock| {
            match_and_compute_row(&udf, serde_json::to_value(mock).unwrap(), &all_names).unwrap()
        });

        insta::assert_debug_snapshot!(match_res);
    }

    #[test]
    fn exec_jinja_matches() {
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

    mod data {

        use serde::Serialize;
        use serde_json::{json, Map};

        type Dict = Map<String, serde_json::Value>;

        pub fn gemini() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "client-c".into(),
                    provider: "gemini".into(),
                    base_url: Some("https://generativelanguage.googleapis.com".into()),
                    options: json!({
                        "model": "gemini-pro"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                },
                response: DbHttpResponseMetadata {
                    status: 200,
                    error_message: None,
                    headers: json!({
                        "content-type": "application/json"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("gemini-pro".into()),
                },
                options: json!({
                    "model": "gemini-pro"
                })
                .as_object()
                .unwrap()
                .clone(),
                date: "2025-06-10".into(),
                raw: json!({
                    "usageMetadata": {
                        "promptTokenCount": 900,
                        "cachedTokenCount": 100,
                        "candidatesTokenCount": 400
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            }
        }

        pub fn anthropic() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "client-b".into(),
                    provider: "anthropic".into(),
                    base_url: Some("https://api.anthropic.com".into()),
                    options: json!({
                        "model": "claude-3-opus"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                },
                response: DbHttpResponseMetadata {
                    status: 200,
                    error_message: None,
                    headers: json!({
                        "content-type": "application/json"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("claude-3-opus".into()),
                },
                options: json!({
                    "model": "claude-3-opus"
                })
                .as_object()
                .unwrap()
                .clone(),
                date: "2025-06-01".into(),
                raw: json!({
                    "usage": {
                        "input_tokens": 1200,
                        "cached_tokens": 150,
                        "output_tokens": 600
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            }
        }

        pub fn none_match() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "unknown-client".into(),
                    provider: "llama-corp".into(), // Not openai, anthropic, or gemini
                    base_url: Some("https://api.unknown-llm.com".into()),
                    options: json!({
                        "model": "llama-9000"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                },
                response: DbHttpResponseMetadata {
                    status: 500, // Doesn't match any known expression like "status == 200"
                    error_message: Some("Internal server error".into()),
                    headers: json!({
                        "content-type": "application/json"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("llama-9000".into()),
                },
                options: json!({
                    "model": "llama-9000"
                })
                .as_object()
                .unwrap()
                .clone(),
                date: "2025-07-01".into(),
                raw: json!({
                    "error": {
                        "message": "Model not found",
                        "code": 404
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            }
        }

        pub fn anthropic_with_bad_raw() -> DbHttpMetadata {
            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "client-x".into(),
                    provider: "anthropic".into(), // ✅ matches provider expression
                    base_url: Some("https://api.anthropic.com".into()),
                    options: json!({
                        "model": "claude-3-haiku"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                },
                response: DbHttpResponseMetadata {
                    status: 200,
                    error_message: None,
                    headers: json!({
                        "content-type": "application/json"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("claude-3-haiku".into()),
                },
                options: json!({
                    "model": "claude-3-haiku"
                })
                .as_object()
                .unwrap()
                .clone(),
                date: "2025-06-12".into(),
                raw: json!({
                    // ❌ Missing `input_tokens`, `cached_tokens`, `output_tokens`
                    "meta": {
                        "token_usage": {
                            "prompt": 123,
                            "completion": 456
                        }
                    },
                    "data": {
                        "some_other_field": true
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            }
        }

        pub fn openai() -> DbHttpMetadata {
            use serde_json::json;

            DbHttpMetadata {
                client: DbHttpClientDetails {
                    name: "client-a".into(),
                    provider: "openai".into(),
                    base_url: Some("https://api.openai.com".into()),
                    options: json!({
                        "model": "gpt-4-turbo",
                        "stream": false,
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                },
                response: DbHttpResponseMetadata {
                    status: 200,
                    error_message: None,
                    headers: json!({
                        "content-type": "application/json",
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                    model: Some("gpt-4-turbo".into()),
                },
                options: json!({
                    "model": "gpt-4-turbo",
                    "stream": false,
                })
                .as_object()
                .unwrap()
                .clone(),
                date: "2025-05-15".into(),
                raw: json!({
                    "usage": {
                        "input_tokens": 1000,
                        "output_tokens": 500,
                        "input_tokens_details": {
                            "cached_tokens": 100
                        },
                        "output_tokens_details": {
                            "reasoning_tokens": 400
                        }
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            }
        }

        #[derive(Debug, Serialize)]
        struct DbHttpClientDetails {
            ///  BAML client name
            name: String,
            ///  Provider name (openai, anthropic, etc.)
            provider: String,
            ///  API base URL
            base_url: Option<String>,
            ///   Request parameters
            options: Dict,
        }

        #[derive(Debug, Serialize)]
        pub struct DbHttpResponseMetadata {
            /// HTTP status code
            status: i32,
            /// Error message if failed
            error_message: Option<String>,
            /// Response headers
            headers: Dict,
            /// Extracted model name
            model: Option<String>,
        }

        #[derive(Debug, Serialize)]
        pub struct DbHttpMetadata {
            client: DbHttpClientDetails,
            response: DbHttpResponseMetadata,
            /// Request parameters (e.g., options['model'])
            options: Dict,
            // NOTE: (Jesus) do we need a more granular type ?
            date: String,
            /// Full response body parsed as JSON
            raw: Dict,
        }
    }
}

// TODO: move inner functions to internal module? Like eval or something. Specially the trie for
// static analysis :]
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
        // TODO: keep values in serde_json::Value?
        for (name, value) in ev.constants {
            env_map.insert(name.into(), serde_json::to_value(value.0).unwrap());
        }
    }

    // we'll exploit the fact that `returns` uses IndexMap multiple times here.

    // compile all expressions to a vec, since we're going to do two passes over them. We'll
    // know which is which because of IndexMap's consistent iteration order.
    // TODO: Gather the compilations elsewhere? Have like a Cow<> but triggering a compilation.
    // Compiler errors should be gathered, and an error state kept, but the errors themselves don't
    // need to be copied to the result.
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

    // store all the undefined strings into a stash.
    // TODO: (Jesus) could group returns here, external to the trie.
    // When submitting a "mark this as zeroed", then we can consult the map.
    for expr in &compiled_expressions {
        let start = string_stash.len();

        if let Ok(e) = expr {
            string_stash.extend(e.undeclared_variables(true));
        }

        ranges.push(start..string_stash.len());
    }
    for (return_index, range) in ranges.into_iter().enumerate() {
        let ret_order = ReturnIterationOrder(return_index);

        for und in &string_stash[range] {
            insert_into_trie(&mut path_trie, und, ret_order);
        }
    }

    let mut undefined_lists: Vec<_> = std::iter::repeat_with(Vec::new)
        .take(compiled_expressions.len())
        .collect();

    // DFS the path trie, in two modes:
    // - tracking mode: starting with the existing map, on each node visited (each `PathTrie` has
    // multiple path-compressed nodes):
    //      - try to interpret the current value as a map. If it fails (e.g it's a number), the
    //      expression will fail when interpreting and we will not consider it for zeroing. Stop DFS
    //      here.
    //      - try to access the map with this node as a key. If it fails, we're going to consider all
    //      the paths as zero. Switch this node to zeroing mode.
    // - zeroing mode: DFS all nodes.
    //      - if node is terminal, add its path [NOTE: could confuse when the access is used in `if` to
    //      check?] to all registered returns.
    //      - if node does not have any children, define it as zero value on the map.
    //      - if node has children, define it as a map. Keep the map reference to track it.
    //
    // Due to Rust restrictions on &mut pointers, the algorithm will use a recursive DFS.

    dfs_tracking(&path_trie, &mut serialized_data, &mut undefined_lists);

    // execute the expressions.
    for ((&name, expr), missing_values) in ev
        .returns
        .keys()
        .zip(compiled_expressions)
        .zip(undefined_lists)
    {
        let expr = match expr {
            Ok(e) => e,
            Err(err) => {
                errors_map.insert(name, err);
                continue;
            }
        };

        let result = expr
            .eval(&serialized_data)
            .map_or_else(|e| Ok(Err(e)), |val| f64::try_from(val).map(Ok));

        // when testing, make sure that the entries have consistent order.
        #[cfg(test)]
        let missing_values = {
            let mut m = missing_values;
            m.sort();
            m
        };

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
                // TODO: (Jesus) what do we do when we don't find floats?
                // Currently ignore them.
                log::warn!("function result is not coercible to floating point: {e}");
            }
        }
    }
    return (result_map, errors_map);

    fn dfs_tracking(
        trie: &PathTrie,
        tracked_value: &mut serde_json::Value,
        zeroed_lists: &mut [Vec<String>],
    ) {
        visit_pieces(
            trie.partial_path.iter().copied().enumerate(),
            trie,
            tracked_value,
            zeroed_lists,
        );

        // NOTE: (Jesus) tail-recursive, because I couldn't get the for-loop version to work.
        // It complained about `as_map` being used in the part that bails out, i.e:
        // ```rs
        // let Some(next) = as_map.get_mut(piece) else {
        // // says it's borrowed here
        //    return dfs_transition_to_zeroing(...);
        // }
        fn visit_pieces<'s, 'l>(
            mut pieces: impl Iterator<Item = (usize, &'l str)>,
            trie: &'l PathTrie<'l>,
            tracked_value: &'s mut serde_json::Value,
            zeroed_lists: &'l mut [Vec<String>],
        ) {
            match pieces.next() {
                None => {
                    // continue DFS
                    for child in &trie.children {
                        dfs_tracking(child, tracked_value, zeroed_lists);
                    }
                }
                Some((piece_index, piece)) => {
                    let Some(as_map) = tracked_value.as_object_mut() else {
                        return;
                    };

                    match as_map.get_mut(piece) {
                        Some(next) => visit_pieces(pieces, trie, next, zeroed_lists),
                        None => dfs_transition_to_zeroing(
                            trie,
                            piece_index,
                            tracked_value,
                            zeroed_lists,
                        ),
                    }
                }
            }
        }
    }

    // TODO: use Map instead of Value in the tracked_value?
    fn dfs_transition_to_zeroing(
        trie: &PathTrie,
        piece_index: usize,
        mut tracked_value: &mut serde_json::Value,
        zeroed_lists: &mut [Vec<String>],
    ) {
        let last_piece_with_children = if trie.children.is_empty() {
            trie.partial_path.len() - 1
        } else {
            trie.partial_path.len()
        };

        // problem:
        // - if I make a map, then the ones that are using it as `if` are wrong (it yields
        // non-zero).
        // - if I make a value, then the ones using as a map are wrong, and I cauld have it in
        // the trie as terminal because there's a usage within an `if` (e.g `if hello.world`)
        //
        // -> node has children -> it's used elsewhere as a map.
        // If it is used within `if`, then there's a problem. Can I just use the AST to find
        // about it? -> not doing that. Annotated at the top what the problem looks like.
        // -> node has no children -> it's safe to set to zero

        for &piece in &trie.partial_path[piece_index..last_piece_with_children] {
            // non-terminal and has at least 1 child.

            let object = tracked_value.as_object_mut().unwrap();

            object.insert(piece.into(), serde_json::Map::new().into());

            tracked_value = object.get_mut(piece).unwrap();
        }

        if let Some(terminal) = trie.terminal_full_path.as_ref() {
            // last needs to be zero.
            tracked_value.as_object_mut().unwrap().insert(
                trie.partial_path.last().copied().unwrap().into(),
                0f64.into(),
            );

            for &index in &terminal.results {
                zeroed_lists[index.0].push(terminal.full_path.into());
            }
        }

        for child in &trie.children {
            dfs_zeroing(child, tracked_value, zeroed_lists);
        }
    }

    #[inline]
    fn dfs_zeroing(
        trie: &PathTrie,
        tracked_value: &mut serde_json::Value,
        zeroed_lists: &mut [Vec<String>],
    ) {
        dfs_transition_to_zeroing(trie, 0, tracked_value, zeroed_lists)
    }

    /// Index from iteration order (which is consistent due to IndexMap) for a certain
    /// return. Used to register
    #[derive(Clone, Copy)]
    struct ReturnIterationOrder(usize);

    struct TrieTerminal<'a> {
        full_path: &'a str,
        // TODO: this could be just a bit set.
        /// The results that are registered for this terminal. If a variable path is found to be zero,
        /// then all of these should have said path added to their "missing value" list.
        results: Vec<ReturnIterationOrder>,
    }

    #[derive(Default)]
    struct PathTrie<'a> {
        partial_path: Vec<&'a str>,
        children: Vec<PathTrie<'a>>,
        terminal_full_path: Option<TrieTerminal<'a>>,
    }

    fn insert_into_trie<'src>(
        root: &mut PathTrie<'src>,
        path: &'src str,
        ret_handle: ReturnIterationOrder,
    ) {
        insert_into_trie_recursive(root, path, path.split('.'), ret_handle)
    }

    // using a tail-recursive approach because that plays better with the borrow checker.
    fn insert_into_trie_recursive<'src>(
        root: &mut PathTrie<'src>,
        path: &'src str,
        mut path_it: impl Iterator<Item = &'src str>,
        ret_handle: ReturnIterationOrder,
    ) {
        // assume everything until `root` is matched.
        let Some(first) = path_it.next() else {
            root.terminal_full_path = Some(match root.terminal_full_path.take() {
                Some(mut terminal) => {
                    terminal.results.push(ret_handle);
                    terminal
                }
                None => TrieTerminal {
                    full_path: path,
                    results: vec![ret_handle],
                },
            });

            return;
        };

        // find a child that matches.
        let matching_child = root
            .children
            .iter_mut()
            .find(|child| child.partial_path[0] == first);

        match matching_child {
            None => {
                // found no children -> insert into the trie.
                root.children.push(PathTrie {
                    partial_path: [first].into_iter().chain(path_it).collect(),
                    children: Vec::new(),
                    terminal_full_path: Some(TrieTerminal {
                        full_path: path,
                        results: vec![ret_handle],
                    }),
                });
                return;
            }
            Some(child) => {
                // match as much of the path as possible.
                for index in 1..child.partial_path.len() {
                    match path_it.next() {
                        None => {
                            // cut the child abcd -> abc {d}, with abc terminal.
                            let partial_path = child.partial_path.drain(0..index).collect();

                            let cutoff_child = std::mem::replace(
                                child,
                                PathTrie {
                                    partial_path,
                                    children: Vec::with_capacity(1),
                                    terminal_full_path: Some(TrieTerminal {
                                        full_path: path,
                                        results: vec![ret_handle],
                                    }),
                                },
                            );

                            // child is now the branch that we inserted.
                            child.children.push(cutoff_child);
                            return;
                        }
                        Some(piece) => {
                            if piece == child.partial_path[index] {
                                // matched, go to next.
                                continue;
                            }

                            // cut the child abcd -> abc {d e}, with abc non-terminal

                            let partial_path = child.partial_path.drain(0..index).collect();
                            let cutoff_child = std::mem::replace(
                                child,
                                PathTrie {
                                    partial_path,
                                    children: Vec::with_capacity(2),
                                    terminal_full_path: None,
                                },
                            );

                            let branch = child;
                            branch.children.push(cutoff_child);
                            branch.children.push(PathTrie {
                                partial_path: [piece].into_iter().chain(path_it).collect(),
                                children: Vec::new(),
                                terminal_full_path: Some(TrieTerminal {
                                    full_path: path,
                                    results: vec![ret_handle],
                                }),
                            });
                            return;
                        }
                    }
                }

                // full path has been matched.
                insert_into_trie_recursive(child, path, path_it, ret_handle)
            }
        }
    }
}

fn find_function_for_row<'c>(
    config: &'c UDFConfig,
    row: &serde_json::Value,
) -> anyhow::Result<Option<MatchedFunction<'c>>> {
    use anyhow::Context;
    let env = get_env();

    fn eval_match(
        src: &str,
        env: &minijinja::Environment,
        row: &serde_json::Value,
    ) -> anyhow::Result<bool> {
        // TODO: (Jesus) cache compiled expressions
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
            // TODO: bind UDFConfig to the source lifetime.
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
