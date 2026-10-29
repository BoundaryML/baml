//! Code complexity metrics for BAML functions and files.
//!
//! Structural metrics are computed on the AST rather than MIR so they reflect
//! the code as written: MIR lowering adds branches the author never wrote
//! (`for` desugaring, catch dispatch, narrowing). Line counts read the CST,
//! where comments are distinct. Call metrics read type inference, which
//! resolves what each call names.
//!
//! Function metrics:
//!
//! - Lines: lines holding code, not counting blank or comment-only lines.
//! - Cyclomatic (`McCabe` 1976, with `&&` / `||` as in Myers 1977): `1 +` one
//!   per decision point: `if` / `if let` (including each `else if`), `while`,
//!   `while let`, `for`, each `match` arm past the first, each `match` guard,
//!   each `catch` arm, `&&`, `||`, `??`, each optional access (`?.`),
//!   `let ... else`, and template `if` branches and `for` loops.
//! - Cognitive (Campbell, `SonarSource` 2017): `+1 + nesting` for `if`,
//!   `if let`, `match`, loops, `catch`, and `let ... else`; `+1` flat for
//!   `else` and `else if`; `+1` per run of like boolean operators; `+1` when
//!   the function is in a recursion cycle. A `match` counts once regardless of
//!   arm count. Lambda bodies are one nesting level deeper but earn no
//!   increment of their own. `?.` and `??` are free, and so are `break` and
//!   `continue`, which the spec only counts when they jump to a label.
//! - Nesting: the most nesting structures open at once, lambdas included.
//! - `NPath` (Nejmeh 1988): see the `npath` module.
//! - Exits: each `return` and `throw` outside nested lambdas, plus normal
//!   completion.
//! - Fan-in and fan-out: distinct callers among the analyzed functions, and
//!   distinct callees anywhere; self-calls excluded. A call through an
//!   interface reaches every implementation.
//!
//! File metrics are the code lines outside test blocks, the number of `test`
//! blocks, and the code lines inside `test` and `testset` blocks.
//!
//! LLM functions have no hand-written body; their prompt template is measured
//! instead, with the same rules as any other template.

use baml_base::{Name, SourceFile, line_index::LineIndex};
use baml_compiler2_hir::{
    body::{FunctionBody, function_body},
    item_data::{
        ImplSubjectData, MethodOwner, class_data, file_functions, function_data, function_llm_meta,
        function_source_map, impl_block_data, impl_enclosing_class, interface_data,
        llm_prompt_segments, method_owner,
    },
    loc::{DeclRef, FunctionLoc},
};
use num_bigint::BigUint;
use text_size::TextRange;

mod calls;
mod lines;
mod npath;
mod structure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMetrics {
    pub functions: Vec<FunctionMetrics>,
    pub files: Vec<FileMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// `Owner.method` for methods, the bare name for top-level functions.
    pub name: Name,
    pub file: SourceFile,
    pub span: TextRange,
    pub lines: u32,
    /// Declared parameters, excluding the ones injected into LLM functions.
    pub params: u32,
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub nesting: u32,
    pub npath: BigUint,
    pub exits: u32,
    pub fan_in: u32,
    pub fan_out: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetrics {
    pub file: SourceFile,
    /// Code lines outside test blocks.
    pub lines: u32,
    /// `test` blocks.
    pub tests: u32,
    /// Code lines inside `test` and `testset` blocks.
    pub test_lines: u32,
}

/// Metrics for every function and method in `files`, and for each file.
///
/// Compiler-generated functions (`Foo@spec` companions, `$init_test`
/// registrations), builtins, and bodiless declarations are skipped. Fan-in and
/// recursion only see calls among the functions of `files`.
pub fn project_metrics(db: &dyn baml_compiler2_hir::Db, files: &[SourceFile]) -> ProjectMetrics {
    let mut functions = Vec::new();
    let mut file_metrics = Vec::new();
    let mut all_callees = Vec::new();
    for &file in files {
        let tree = baml_compiler_parser::syntax_tree(db, file);
        let lines = LineIndex::new(file.text(db));
        let (code, tests, test_lines) = lines::file_lines(&tree, &lines);
        file_metrics.push(FileMetrics {
            file,
            lines: code,
            tests,
            test_lines,
        });
        let file_fns = file_functions(db, file);
        for &function in file_fns {
            let data = function_data(db, function);
            if data.name.contains('@') || baml_compiler2_ast::is_init_test_function(&data.name) {
                continue;
            }
            let llm = function_llm_meta(db, function).is_some();
            let Some((structure, npath, callees)) = analyze(db, file_fns, function, &data.name)
            else {
                continue;
            };
            let span = function_source_map(db, function).span;
            let params = data
                .params
                .iter()
                .filter(|p| !llm || !baml_compiler2_ast::is_injected_llm_param(&p.name))
                .count();
            let fan_out = callees
                .iter()
                .filter(|&&callee| callee != DeclRef::Source(function))
                .count();
            functions.push(FunctionMetrics {
                name: qualified_name(db, function, &data.name),
                file,
                span,
                lines: count(lines::code_lines(&tree, &lines, span).len()),
                params: count(params),
                cyclomatic: 1 + structure.decisions,
                cognitive: structure.cognitive,
                nesting: structure.max_nesting,
                npath,
                exits: 1 + structure.explicit_exits,
                fan_in: 0,
                fan_out: count(fan_out),
            });
            all_callees.push((function, callees));
        }
    }

    let graph: Vec<_> = all_callees
        .iter()
        .map(|(loc, callees)| (*loc, callees))
        .collect();
    for (metrics, links) in functions.iter_mut().zip(calls::link(&graph)) {
        metrics.fan_in = links.fan_in;
        metrics.cognitive += u32::from(links.recursive);
    }

    ProjectMetrics {
        functions,
        files: file_metrics,
    }
}

type Analysis<'db> = (structure::Structure, BigUint, calls::Callees<'db>);

/// The body-derived metrics of one function, or `None` when it has no body to
/// measure.
fn analyze<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    file_fns: &[FunctionLoc<'db>],
    function: FunctionLoc<'db>,
    name: &Name,
) -> Option<Analysis<'db>> {
    if function_llm_meta(db, function).is_some() {
        let segments = llm_prompt_segments(db, function).as_ref()?;
        let spec = spec_companion(db, file_fns, function, name)?;
        let FunctionBody::Expr(body) = &*function_body(db, spec) else {
            return None;
        };
        return Some((
            structure::walk_template(body, segments),
            npath::npath_template(body, segments),
            calls::prompt_callees(db, spec, body, segments),
        ));
    }
    let FunctionBody::Expr(body) = &*function_body(db, function) else {
        return None;
    };
    let root = body.root_expr?;
    Some((
        structure::walk_expr(body, root),
        npath::npath_expr(body, root),
        calls::body_callees(db, function, body),
    ))
}

/// The `@spec` companion lowering synthesized for LLM function `function`:
/// the function of the same owner named `<name>@spec`.
fn spec_companion<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    file_fns: &[FunctionLoc<'db>],
    function: FunctionLoc<'db>,
    name: &Name,
) -> Option<FunctionLoc<'db>> {
    let spec_name = format!("{name}@spec");
    let owner = method_owner(db, function);
    file_fns.iter().copied().find(|&candidate| {
        function_data(db, candidate).name == spec_name && method_owner(db, candidate) == owner
    })
}

fn qualified_name<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
    name: &Name,
) -> Name {
    let owner = match method_owner(db, function) {
        None => return name.clone(),
        Some(MethodOwner::Class(class)) => class_data(db, class).name.to_string(),
        Some(MethodOwner::Interface(interface)) => interface_data(db, interface).name.to_string(),
        Some(MethodOwner::Impl(block)) => match impl_enclosing_class(db, block) {
            Some(class) => class_data(db, class).name.to_string(),
            None => {
                let data = impl_block_data(db, block);
                match data.subject {
                    ImplSubjectData::Free { for_target, .. } => {
                        data.type_refs.display(for_target).to_string()
                    }
                    ImplSubjectData::InClass { class, .. } => {
                        class_data(db, class).name.to_string()
                    }
                }
            }
        },
    };
    Name::new(format!("{owner}.{name}"))
}

pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
