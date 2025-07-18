use baml_types::BamlMap;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

/// Collects all outputs declared in the configuration file, regardless of which functions declare
/// them.
pub fn gather_all_outputs<'a>(udf: &'a UDFConfig) -> IndexSet<&'a str> {
    // DFS on tree of override sets.
    // Order doesn't matter since we just want to gather everything.

    let mut dfs_stack = Vec::new();
    let mut dfs_top = &udf.functions;

    let mut set = IndexSet::new();

    loop {
        set.extend(dfs_top.into_iter().flat_map(extract_outputs));
        dfs_stack.extend(dfs_top.into_iter().map(|func| &func.overrides));

        dfs_top = match dfs_stack.pop() {
            Some(next) => next,
            None => break,
        }
    }

    return set;

    fn extract_outputs<'a>(func: &'a Function) -> impl Iterator<Item = &'a str> {
        func.returns.keys().map(AsRef::as_ref)
    }
}

// NOTE: (Jesus) Newtype'd to make simple pre-processing additions (e.g 1/100K -> 1e-5) easy
/// Arbitrary constant provided to the UDF by the config file.
#[repr(transparent)]
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(transparent)]
pub struct Constant(pub minijinja::Value);

// NOTE: (Jesus) would be optimal to compile all the expressions while deserializing, but that is not
// easy to do with serde.
// minijinja::Environment can hold templates by name, but adding a name just to cache them could
// negate the effect of caching. If we want to pre-compile them, it would be nice to have them in
// bookkeep whether the expression is raw or has a compiled
// template. Perhaps index_or_compile(&mut Environment)?

/// Raw expression to be executed by Jinja for filtering.
#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MatchExpression(pub String);

// NOTE: (Jesus) Can we use references for source? Is it worth it, knowing that we're going to
// directly pass the strings to compile_expression() anyway?

/// Raw Jinja template which will be executed for the inputs that match.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(transparent)]
pub struct OutputExpression(pub String);

#[derive(Debug, Deserialize, Serialize)]
pub struct UDFConfig {
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(rename = "constants")]
    #[serde(default)]
    pub(crate) global_constants: BamlMap<String, Constant>,
    pub(crate) functions: Vec<Function>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Function {
    #[serde(rename = "match")]
    pub(crate) match_expr: MatchExpression,
    #[serde(default)]
    pub(crate) constants: BamlMap<String, Constant>,
    #[serde(default)]
    pub(crate) returns: BamlMap<String, OutputExpression>,
    #[serde(default)]
    pub(crate) overrides: Vec<Function>,
}
