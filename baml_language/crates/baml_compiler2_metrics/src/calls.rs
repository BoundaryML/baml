//! The functions each function calls, and the call graph built from them.

use std::collections::{HashMap, HashSet};

use baml_base::{Name, SourceRoot};
use baml_compiler2_ast::{BodyNode, Expr, ExprBody, ExprId, TemplateSegment};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    item_data::{function_data, impl_block_data},
    loc::{DeclRef, FunctionLoc, InterfaceLoc},
};
use baml_compiler2_hir_ty::{
    extern_loc::FunctionRef,
    impls::impls_naming_interface,
    infer::{InferenceResult, MemberResolution, MethodCallee, infer_body},
};

/// Every function one function calls, wherever declared.
pub(crate) type Callees<'db> = HashSet<FunctionRef<'db>>;

/// The calls in an ordinary function's body, lambdas included.
pub(crate) fn body_callees<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
    body: &ExprBody,
) -> Callees<'db> {
    let inference = infer_body(db, BodyOwnerId::Function(function));
    let viewer = function.file(db).source_root(db);
    collect(
        db,
        viewer,
        inference,
        body,
        body.exprs.iter().map(|(id, _)| id),
    )
}

/// The calls in an LLM function's prompt, read through its `@spec` companion,
/// whose body the prompt's `ExprId`s index.
pub(crate) fn prompt_callees<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    spec: FunctionLoc<'db>,
    body: &ExprBody,
    segments: &[TemplateSegment],
) -> Callees<'db> {
    let mut seeds = Vec::new();
    segment_nodes(segments, &mut seeds);
    let mut exprs = HashSet::new();
    let mut children = Vec::new();
    while let Some(node) = seeds.pop() {
        match node {
            BodyNode::Expr(id) => exprs.extend(
                body.reachable_excluding_lambdas(id)
                    .into_iter()
                    .filter_map(|node| match node {
                        BodyNode::Expr(id) => Some(id),
                        BodyNode::Stmt(_) => None,
                    }),
            ),
            BodyNode::Stmt(id) => {
                children.clear();
                body.stmt_children(id, &mut children);
                seeds.extend(children.iter().copied());
            }
        }
    }
    let inference = infer_body(db, BodyOwnerId::Function(spec));
    let viewer = spec.file(db).source_root(db);
    collect(db, viewer, inference, body, exprs.into_iter())
}

fn segment_nodes(segments: &[TemplateSegment], out: &mut Vec<BodyNode>) {
    for segment in segments {
        match segment {
            TemplateSegment::Text(_) => {}
            TemplateSegment::Interp(expr) => out.push(BodyNode::Expr(*expr)),
            TemplateSegment::Effect { stmts, .. } => {
                out.extend(stmts.iter().copied().map(BodyNode::Stmt));
            }
            TemplateSegment::For {
                collection, body, ..
            } => {
                out.push(BodyNode::Expr(*collection));
                segment_nodes(body, out);
            }
            TemplateSegment::CStyleFor {
                init,
                cond,
                step,
                body,
            } => {
                out.push(BodyNode::Stmt(*init));
                out.push(BodyNode::Expr(*cond));
                out.extend(step.map(BodyNode::Stmt));
                segment_nodes(body, out);
            }
            TemplateSegment::If {
                branches,
                else_body,
            } => {
                for branch in branches {
                    out.push(BodyNode::Expr(branch.condition));
                    segment_nodes(&branch.body, out);
                }
                if let Some(else_body) = else_body {
                    segment_nodes(else_body, out);
                }
            }
        }
    }
}

fn collect<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    inference: &InferenceResult<'db>,
    body: &ExprBody,
    exprs: impl Iterator<Item = ExprId>,
) -> Callees<'db> {
    let mut callees = Callees::default();
    for id in exprs {
        if let Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } = &body.exprs[id]
            && let Some(resolution) = inference.callee_resolution(*callee)
        {
            callees.extend(resolution.callable(db));
            if let MemberResolution::Method {
                callee:
                    MethodCallee::Virtual {
                        interface: DeclRef::Source(interface),
                        method,
                    },
                ..
            } = resolution
            {
                callees.extend(
                    implementations(db, viewer, *interface, method)
                        .into_iter()
                        .map(DeclRef::Source),
                );
            }
        }
    }
    callees
}

/// Every method that can answer a virtual call to `interface.method`: the
/// implementations of it in the impl blocks `viewer` can see. Linking a
/// virtual call to all of them is class hierarchy analysis; the interface's
/// own declaration, which also covers an adopted default body, is the
/// resolved callee itself.
fn implementations<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: SourceRoot,
    interface: InterfaceLoc<'db>,
    method: &Name,
) -> Vec<FunctionLoc<'db>> {
    impls_naming_interface(db, viewer, interface)
        .iter()
        .flat_map(|&block| impl_block_data(db, block).methods.iter().copied())
        .filter(|&func| function_data(db, func).name == *method)
        .collect()
}

/// Call-graph facts for one function.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Links {
    pub fan_in: u32,
    pub recursive: bool,
}

/// Fan-in and recursion for each of `functions`, from the calls among them.
///
/// A function is recursive when it calls itself or sits in a larger cycle,
/// which is Sonar's rule: every function in a recursion cycle earns the
/// increment.
pub(crate) fn link<'db>(functions: &[(FunctionLoc<'db>, &Callees<'db>)]) -> Vec<Links> {
    let index: HashMap<FunctionLoc<'db>, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, (loc, _))| (*loc, i))
        .collect();
    let edges: Vec<Vec<usize>> = functions
        .iter()
        .map(|(_, callees)| {
            let mut targets: Vec<usize> = callees
                .iter()
                .filter_map(|callee| match callee {
                    DeclRef::Source(loc) => index.get(loc).copied(),
                    DeclRef::External(_) => None,
                })
                .collect();
            targets.sort_unstable();
            targets
        })
        .collect();

    let mut links = vec![Links::default(); functions.len()];
    for (caller, targets) in edges.iter().enumerate() {
        for &callee in targets {
            if callee == caller {
                links[caller].recursive = true;
            } else {
                links[callee].fan_in += 1;
            }
        }
    }
    for component in baml_compiler2_hir_ty::globals::strongly_connected_components(&edges) {
        if component.len() > 1 {
            for member in component {
                links[member].recursive = true;
            }
        }
    }
    links
}
