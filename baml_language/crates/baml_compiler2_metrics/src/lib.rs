//! Code complexity metrics for BAML functions, classes, and interfaces.
//!
//! Structural metrics are computed on the AST rather than MIR so they reflect
//! the code as written: MIR lowering adds branches the author never wrote
//! (`for` desugaring, catch dispatch, narrowing). Token metrics read the CST,
//! where comments are distinct. Call and coupling metrics read type
//! inference, which resolves what each call and member access names.
//!
//! Function metrics:
//!
//! - Cyclomatic (McCabe): `1 +` one per decision point: `if` / `if let`
//!   (including each `else if`), `while`, `while let`, `for`, each `match` arm
//!   past the first, each `match` guard, each `catch` arm, `&&`, `||`, `??`,
//!   each optional access (`?.`), `let ... else`, and template `if` branches
//!   and `for` loops.
//! - Cognitive (Sonar white paper, adapted): `+1 + nesting` for `if`,
//!   `if let`, `match`, loops, `catch`, and `let ... else`; `+1` flat for
//!   `else` and `else if`; `+1` per run of like boolean operators; `+1` for
//!   `break` / `continue`; `+1` when the function is in a recursion cycle. A
//!   `match` counts once regardless of arm count. Lambda bodies are one
//!   nesting level deeper but earn no increment of their own. `?.` and `??`
//!   are free.
//! - `NPath`: see [`npath`](crate::npath).
//! - Halstead and the physical line counts: see [`tokens`](crate::tokens).
//! - Logical lines: statements, plus block tail expressions.
//! - Exits: each `return` and `throw` outside nested lambdas, plus normal
//!   completion.
//! - ABC (Fitzpatrick): assignments are `let` initializers and (compound)
//!   assignments; branches are calls; conditions are comparisons, `else`,
//!   each `match` arm, `catch` and each handler, type and pattern tests,
//!   `?.` / `??`, and bare values used as conditions.
//! - Fan-in and fan-out: distinct callers among the analyzed functions, and
//!   distinct callees anywhere; self-calls excluded.
//!
//! LLM functions have no hand-written body; their prompt template is measured
//! instead, with the same rules as any other template.

use std::collections::HashMap;

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
use serde::Serialize;
use text_size::TextRange;

mod calls;
mod npath;
mod structure;
mod tokens;
mod types;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMetrics {
    pub functions: Vec<FunctionMetrics>,
    pub types: Vec<TypeMetrics>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FnKind {
    Expr,
    Llm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// `Owner.method` for methods, the bare name for top-level functions.
    pub name: Name,
    pub file: SourceFile,
    pub span: TextRange,
    pub kind: FnKind,
    pub loc: Loc,
    /// Declared parameters, excluding the ones injected into LLM functions.
    pub params: u32,
    pub max_nesting: u32,
    pub cyclomatic: u32,
    pub cognitive: u32,
    /// Saturates at `u64::MAX`.
    pub npath: u64,
    pub exits: u32,
    pub abc: Abc,
    pub halstead: Halstead,
    pub calls: Calls,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Loc {
    /// Every line the function spans.
    pub sloc: u32,
    /// Lines holding code.
    pub ploc: u32,
    /// Statements.
    pub lloc: u32,
    /// Lines holding a comment, including trailing comments after code.
    pub cloc: u32,
    /// Lines holding neither code nor a comment.
    pub blank: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Abc {
    pub assignments: u32,
    pub branches: u32,
    pub conditions: u32,
}

impl Abc {
    /// `sqrt(A^2 + B^2 + C^2)`.
    pub fn magnitude(&self) -> f64 {
        let [a, b, c] = [self.assignments, self.branches, self.conditions].map(f64::from);
        (a * a + b * b + c * c).sqrt()
    }
}

/// Halstead's base counts; every other Halstead measure derives from them.
/// Formulas follow rust-code-analysis. A measure whose denominator is zero is
/// `0.0`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Halstead {
    /// `n1`
    pub distinct_operators: u32,
    /// `N1`
    pub operators: u32,
    /// `n2`
    pub distinct_operands: u32,
    /// `N2`
    pub operands: u32,
}

impl Halstead {
    /// `n = n1 + n2`
    pub fn vocabulary(&self) -> f64 {
        f64::from(self.distinct_operators) + f64::from(self.distinct_operands)
    }

    /// `N = N1 + N2`
    pub fn length(&self) -> f64 {
        f64::from(self.operators) + f64::from(self.operands)
    }

    /// `n1 log2 n1 + n2 log2 n2`
    pub fn estimated_length(&self) -> f64 {
        let term = |n: u32| {
            let n = f64::from(n);
            if n > 0.0 { n * n.log2() } else { 0.0 }
        };
        term(self.distinct_operators) + term(self.distinct_operands)
    }

    pub fn purity_ratio(&self) -> f64 {
        ratio(self.estimated_length(), self.length())
    }

    /// `N log2 n`
    pub fn volume(&self) -> f64 {
        let vocabulary = self.vocabulary();
        if vocabulary > 0.0 {
            self.length() * vocabulary.log2()
        } else {
            0.0
        }
    }

    /// `(n1 / 2) * (N2 / n2)`
    pub fn difficulty(&self) -> f64 {
        f64::from(self.distinct_operators) / 2.0
            * ratio(f64::from(self.operands), f64::from(self.distinct_operands))
    }

    pub fn level(&self) -> f64 {
        ratio(1.0, self.difficulty())
    }

    pub fn effort(&self) -> f64 {
        self.difficulty() * self.volume()
    }

    /// Seconds, at Stroud's 18 mental discriminations per second.
    pub fn time(&self) -> f64 {
        self.effort() / 18.0
    }

    /// Delivered bugs: `E^(2/3) / 3000`.
    pub fn bugs(&self) -> f64 {
        self.effort().powf(2.0 / 3.0) / 3000.0
    }
}

fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Calls {
    pub fan_in: u32,
    pub fan_out: u32,
    /// Calls itself, directly or through other analyzed functions.
    pub recursive: bool,
}

/// The three common Maintainability Index variants, as in rust-code-analysis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Maintainability {
    /// `171 - 5.2 ln V - 0.23 G - 16.2 ln SLOC`
    pub original: f64,
    /// The SEI variant, which rewards comments.
    pub sei: f64,
    /// The original rescaled to 0..100 and floored at 0, as Visual Studio
    /// reports it.
    pub visual_studio: f64,
}

impl FunctionMetrics {
    pub fn maintainability(&self) -> Maintainability {
        let volume = self.halstead.volume();
        let cyclomatic = f64::from(self.cyclomatic);
        let sloc = f64::from(self.loc.sloc);
        let comments = ratio(f64::from(self.loc.cloc), sloc);
        let original = 171.0 - 5.2 * volume.ln() - 0.23 * cyclomatic - 16.2 * sloc.ln();
        let sei = 171.0 - 5.2 * volume.log2() - 0.23 * cyclomatic - 16.2 * sloc.log2()
            + 50.0 * (comments * 2.4).sqrt().sin();
        Maintainability {
            original,
            sei,
            visual_studio: (original * 100.0 / 171.0).max(0.0),
        }
    }

    /// Henry and Kafura's information flow: `SLOC * (fan_in * fan_out)^2`.
    pub fn henry_kafura(&self) -> u64 {
        let flow = u64::from(self.calls.fan_in) * u64::from(self.calls.fan_out);
        u64::from(self.loc.sloc).saturating_mul(flow.saturating_mul(flow))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Class,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMetrics {
    pub name: Name,
    pub file: SourceFile,
    pub kind: TypeKind,
    pub fields: u32,
    /// Number of methods, including those of its `implements` blocks.
    pub nom: u32,
    /// Weighted methods per class: the sum of its methods' cyclomatic numbers.
    pub wmc: u32,
    /// Response for a class: its methods plus every distinct function they call.
    pub rfc: u32,
    /// Coupling between objects: distinct other types whose fields, variants,
    /// or methods its methods use.
    pub cbo: u32,
    /// Lack of cohesion (LCOM4); `None` for interfaces, which hold no state.
    pub lcom4: Option<u32>,
    /// Interfaces a class implements, or an interface requires.
    pub interfaces_implemented: u32,
}

/// Metrics for every function, method, class, and interface in `files`.
///
/// Compiler-generated functions (`Foo@spec` companions, `$init_test`
/// registrations), builtins, and bodiless declarations are skipped. Fan-in and
/// recursion only see calls among the functions of `files`.
pub fn project_metrics(db: &dyn baml_compiler2_hir::Db, files: &[SourceFile]) -> ProjectMetrics {
    let mut functions = Vec::new();
    let mut all_uses = Vec::new();
    for &file in files {
        let tree = baml_compiler_parser::syntax_tree(db, file);
        let lines = LineIndex::new(file.text(db));
        let file_fns = file_functions(db, file);
        for &function in file_fns {
            let data = function_data(db, function);
            if data.name.contains('@') || baml_compiler2_ast::is_init_test_function(&data.name) {
                continue;
            }
            let Some((kind, structure, npath, uses)) = analyze(db, file_fns, function, &data.name)
            else {
                continue;
            };
            let span = function_source_map(db, function).span;
            let params = data
                .params
                .iter()
                .filter(|p| {
                    kind == FnKind::Expr || !baml_compiler2_ast::is_injected_llm_param(&p.name)
                })
                .count();
            let fan_out = uses
                .callees
                .iter()
                .filter(|&&callee| callee != DeclRef::Source(function))
                .count();
            functions.push(FunctionMetrics {
                name: qualified_name(db, function, &data.name),
                file,
                span,
                kind,
                loc: Loc {
                    lloc: structure.lloc,
                    ..tokens::loc(&tree, &lines, span)
                },
                params: count(params),
                max_nesting: structure.max_nesting,
                cyclomatic: 1 + structure.decisions,
                cognitive: structure.cognitive,
                npath,
                exits: 1 + structure.explicit_exits,
                abc: structure.abc,
                halstead: tokens::halstead(&tree, span),
                calls: Calls {
                    fan_in: 0,
                    fan_out: count(fan_out),
                    recursive: false,
                },
            });
            all_uses.push((function, uses));
        }
    }

    let graph: Vec<_> = all_uses.iter().map(|(loc, uses)| (*loc, uses)).collect();
    for (metrics, links) in functions.iter_mut().zip(calls::link(&graph)) {
        metrics.calls.fan_in = links.fan_in;
        metrics.calls.recursive = links.recursive;
        metrics.cognitive += u32::from(links.recursive);
    }

    let facts: HashMap<_, _> = all_uses
        .iter()
        .zip(&functions)
        .map(|((loc, uses), metrics)| {
            (
                *loc,
                types::MethodFacts {
                    cyclomatic: metrics.cyclomatic,
                    uses,
                },
            )
        })
        .collect();
    let types = types::type_metrics(db, files, &facts);

    ProjectMetrics { functions, types }
}

type Analysis<'db> = (FnKind, structure::Structure, u64, calls::Uses<'db>);

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
            FnKind::Llm,
            structure::walk_template(body, segments),
            npath::npath_template(body, segments),
            calls::prompt_uses(db, spec, body, segments),
        ));
    }
    let FunctionBody::Expr(body) = &*function_body(db, function) else {
        return None;
    };
    let root = body.root_expr?;
    Some((
        FnKind::Expr,
        structure::walk_expr(body, root),
        npath::npath_expr(body, root),
        calls::body_uses(db, function, body),
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
