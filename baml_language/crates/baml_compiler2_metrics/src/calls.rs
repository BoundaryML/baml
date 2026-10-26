//! What each function's code resolves to (calls, field reads, coupled types),
//! and the call graph built from it.

use std::collections::{HashMap, HashSet};

use baml_base::{Name, SourceRoot};
use baml_compiler2_ast::{BodyNode, Expr, ExprBody, ExprId, TemplateSegment};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    item_data::{MethodOwner, function_data, impl_block_data, impl_enclosing_class, method_owner},
    loc::{DeclRef, FunctionLoc, InterfaceLoc},
};
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef, FunctionRef, InterfaceRef},
    impls::impls_naming_interface,
    infer::{InferenceResult, MemberResolution, MethodCallee, infer_body},
};

/// A declared type one function's code reaches into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TypeRef<'db> {
    Class(ClassRef<'db>),
    Enum(EnumRef<'db>),
    Interface(InterfaceRef<'db>),
}

/// What one function's code resolves to.
#[derive(Debug, Default)]
pub(crate) struct Uses<'db> {
    /// Every function it calls, wherever declared.
    pub callees: HashSet<FunctionRef<'db>>,
    /// Every class field it reads or writes.
    pub fields: HashSet<(ClassRef<'db>, Name)>,
    /// Every type whose fields, variants, or methods it uses.
    pub coupled: HashSet<TypeRef<'db>>,
}

/// The uses in an ordinary function's body, lambdas included.
pub(crate) fn body_uses<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
    body: &ExprBody,
) -> Uses<'db> {
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

/// The uses in an LLM function's prompt, read through its `@spec` companion,
/// whose body the prompt's `ExprId`s index.
pub(crate) fn prompt_uses<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    spec: FunctionLoc<'db>,
    body: &ExprBody,
    segments: &[TemplateSegment],
) -> Uses<'db> {
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
) -> Uses<'db> {
    let mut uses = Uses::default();
    for id in exprs {
        if let Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } = &body.exprs[id]
            && let Some(resolution) = inference.callee_resolution(*callee)
        {
            uses.callees.extend(resolution.callable(db));
            if let MemberResolution::Method {
                callee:
                    MethodCallee::Virtual {
                        interface: DeclRef::Source(interface),
                        method,
                    },
                ..
            } = resolution
            {
                uses.callees.extend(
                    implementations(db, viewer, *interface, method)
                        .into_iter()
                        .map(DeclRef::Source),
                );
            }
        }
        if let Some(resolution) = inference.member_resolutions.get(&id) {
            note(db, &mut uses, resolution);
        }
        if let Some(path) = inference.path_resolutions.get(&id) {
            for segment in &path.segments {
                if let Some(resolution) = &segment.resolution {
                    note(db, &mut uses, resolution);
                }
            }
        }
    }
    uses
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

fn note<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    uses: &mut Uses<'db>,
    resolution: &MemberResolution<'db>,
) {
    match resolution {
        MemberResolution::Field { class, field } => {
            uses.coupled.insert(TypeRef::Class(*class));
            uses.fields.insert((*class, field.clone()));
        }
        MemberResolution::Variant { enum_loc, .. } => {
            uses.coupled.insert(TypeRef::Enum(*enum_loc));
        }
        MemberResolution::InterfaceVirtualField { interface, .. }
        | MemberResolution::Method {
            callee: MethodCallee::Virtual { interface, .. },
            ..
        } => {
            uses.coupled.insert(TypeRef::Interface(*interface));
        }
        MemberResolution::Method { callee, .. } => {
            if let Some(DeclRef::Source(func)) = callee.callable(db)
                && let Some(owner) = owner_type(db, func)
            {
                uses.coupled.insert(owner);
            }
        }
        MemberResolution::Free { .. } => {}
    }
}

/// The type that declares `method`: its class, its interface, or the class an
/// in-body `implements` block belongs to.
pub(crate) fn owner_type<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    method: FunctionLoc<'db>,
) -> Option<TypeRef<'db>> {
    match method_owner(db, method)? {
        MethodOwner::Class(class) => Some(TypeRef::Class(DeclRef::Source(class))),
        MethodOwner::Interface(interface) => Some(TypeRef::Interface(DeclRef::Source(interface))),
        MethodOwner::Impl(block) => {
            impl_enclosing_class(db, block).map(|class| TypeRef::Class(DeclRef::Source(class)))
        }
    }
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
pub(crate) fn link<'db>(functions: &[(FunctionLoc<'db>, &Uses<'db>)]) -> Vec<Links> {
    let index: HashMap<FunctionLoc<'db>, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, (loc, _))| (*loc, i))
        .collect();
    let edges: Vec<Vec<usize>> = functions
        .iter()
        .map(|(_, uses)| {
            let mut targets: Vec<usize> = uses
                .callees
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
