//! Which heap types the cycle collector tracks.
//!
//! A heap object is an array, a map, a class instance, a function value
//! (its closure's environment) or a captured local's cell. Natively each is
//! behind a counted handle, and a cycle of handles is never freed by its
//! counts alone. The runtime's cycle collector (`bex_aot::cc`) finds such
//! cycles by trial deletion, which costs something on every decrement of a
//! handle it tracks, so only the handles of types that *can* be on a cycle
//! are tracked: the Nim ORC rule.
//!
//! The program is closed (every class instantiation, implementor and
//! lambda is known), so the analysis is exact over types: build the graph
//! whose nodes are the heap types the module mentions and whose edges are
//! "a value of this type can hold a handle to that one" (through fields,
//! elements, map values, `T | null`, union members, interface implementors,
//! and, for a function type, every lambda of that signature's captured
//! cells), and track exactly the types in a strongly connected component
//! with a cycle (or a self-loop). Everything else is plain `Rc`: a value
//! of such a type can never reach itself, so its count alone frees it.
//!
//! The one place the types do not say what a value holds is a thrown
//! value (`Thrown`): it is taken to reach every class.

use std::fmt::Write as _;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    classes::ClassInfo,
    function::{Candidate, LocalKind},
    types::{ClassInst, NativeTy},
};

/// A heap type: what a handle points to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum HeapTy<'db> {
    /// An array, map, class instance or function type.
    Value(NativeTy<'db>),
    /// The cell of a captured local of this type.
    Cell(NativeTy<'db>),
}

/// The decision for one heap type, as the report lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedType {
    /// The type, BAML-flavoured (`user.Node[]`, `cell of int`).
    pub described: String,
    /// Whether its handles are tracked.
    pub tracked: bool,
    /// Why: the cycle it is on, or that none passes through it.
    pub reason: String,
}

/// How many allocation sites of each kind the printer emitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocSites {
    /// Sites that allocate a tracked object (`cc`, `array::new_cc`, ..).
    pub tracked: usize,
    /// Sites that allocate an untracked object.
    pub untracked: usize,
}

/// The memory-management side of a compiled module: every heap type with
/// its decision, and the allocation sites the printer emitted.
#[derive(Debug, Clone, Default)]
pub struct MemoryReport {
    pub types: Vec<TrackedType>,
    pub sites: AllocSites,
}

impl MemoryReport {
    /// Heap types tracked and untracked.
    pub fn counts(&self) -> (usize, usize) {
        let tracked = self.types.iter().filter(|ty| ty.tracked).count();
        (tracked, self.types.len() - tracked)
    }

    /// The report as text: one line per type, then the totals.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for ty in &self.types {
            let mark = if ty.tracked { "tracked  " } else { "untracked" };
            let _ = writeln!(out, "{mark} {}  ({})", ty.described, ty.reason);
        }
        let (tracked, untracked) = self.counts();
        let _ = writeln!(
            out,
            "types: {tracked} tracked, {untracked} untracked; allocation sites: {} tracked, {} untracked",
            self.sites.tracked, self.sites.untracked
        );
        out
    }
}

/// The tracked heap types of a module.
#[derive(Debug, Default)]
pub(crate) struct Tracking<'db> {
    tracked: FxHashSet<HeapTy<'db>>,
    pub(crate) report: MemoryReport,
}

impl<'db> Tracking<'db> {
    /// Whether handles to `ty` (an array, map, class or function type) are
    /// tracked. Any other type is not a heap type and is never tracked.
    pub(crate) fn value(&self, ty: &NativeTy<'db>) -> bool {
        !self.tracked.is_empty() && self.tracked.contains(&HeapTy::Value(ty.clone()))
    }

    /// Whether the cell of a captured local of type `ty` is tracked.
    pub(crate) fn cell(&self, ty: &NativeTy<'db>) -> bool {
        !self.tracked.is_empty() && self.tracked.contains(&HeapTy::Cell(ty.clone()))
    }
}

/// Decide every heap type the module mentions. `describe` renders a type
/// for the report.
pub(crate) fn analyze<'db>(
    classes: &[&ClassInfo<'db>],
    candidates: &[Candidate<'db>],
    describe: &dyn Fn(&NativeTy<'db>) -> String,
) -> Tracking<'db> {
    let fields: FxHashMap<&ClassInst<'db>, &ClassInfo<'db>> =
        classes.iter().map(|info| (&info.instance, *info)).collect();
    let all_classes: Vec<NativeTy<'db>> = classes
        .iter()
        .map(|info| NativeTy::Class(info.instance.clone()))
        .collect();
    // Every lambda's captured cells, by its function type.
    let mut lambdas: FxHashMap<NativeTy<'db>, Vec<NativeTy<'db>>> = FxHashMap::default();
    for candidate in candidates.iter().filter(|candidate| candidate.is_lambda()) {
        let ty = NativeTy::Fn(
            candidate.param_tys().into_iter().cloned().collect(),
            Box::new(candidate.return_ty().clone()),
        );
        lambdas
            .entry(ty)
            .or_default()
            .extend(candidate.captures.iter().cloned());
    }

    // The roots: every type a function, a lambda environment or a class
    // spells, and every captured local's cell.
    let mut roots: Vec<HeapTy<'db>> = Vec::new();
    let add = |ty: &NativeTy<'db>, roots: &mut Vec<HeapTy<'db>>| {
        heap_tys(ty, &all_classes, &mut |heap| roots.push(heap));
    };
    for info in classes {
        add(&NativeTy::Class(info.instance.clone()), &mut roots);
    }
    for candidate in candidates {
        for (index, kind) in candidate.kinds.iter().enumerate() {
            if let LocalKind::Value(ty) = kind {
                add(ty, &mut roots);
                let captured = candidate
                    .body
                    .locals
                    .get(index)
                    .is_some_and(|local| local.is_captured);
                if captured {
                    roots.push(HeapTy::Cell(ty.clone()));
                }
            }
        }
        for ty in &candidate.captures {
            roots.push(HeapTy::Cell(ty.clone()));
            add(ty, &mut roots);
        }
        for ty in candidate.values.iter().flatten().flatten() {
            add(ty, &mut roots);
        }
        for ty in candidate.function_values.values() {
            add(ty, &mut roots);
        }
    }

    // The graph, discovered from the roots.
    let mut index: FxHashMap<HeapTy<'db>, usize> = FxHashMap::default();
    let mut nodes: Vec<HeapTy<'db>> = Vec::new();
    let mut edges: Vec<Vec<usize>> = Vec::new();
    let mut work: Vec<HeapTy<'db>> = roots;
    let mut intern = |heap: HeapTy<'db>,
                      nodes: &mut Vec<HeapTy<'db>>,
                      edges: &mut Vec<Vec<usize>>,
                      work: &mut Vec<HeapTy<'db>>| {
        *index.entry(heap.clone()).or_insert_with(|| {
            nodes.push(heap.clone());
            edges.push(Vec::new());
            work.push(heap);
            nodes.len() - 1
        })
    };
    let pending: Vec<HeapTy<'db>> = std::mem::take(&mut work);
    for heap in pending {
        intern(heap, &mut nodes, &mut edges, &mut work);
    }
    while let Some(heap) = work.pop() {
        let from = intern(heap.clone(), &mut nodes, &mut edges, &mut work);
        let mut targets = Vec::new();
        let mut reach = |ty: &NativeTy<'db>| {
            heap_tys(ty, &all_classes, &mut |target| targets.push(target));
        };
        match &heap {
            HeapTy::Cell(ty) => reach(ty),
            HeapTy::Value(NativeTy::Array(element)) => reach(element),
            HeapTy::Value(NativeTy::Map(key, value)) => {
                reach(key);
                reach(value);
            }
            HeapTy::Value(NativeTy::Class(instance)) => {
                if let Some(info) = fields.get(instance) {
                    for field in &info.fields {
                        reach(&field.ty);
                    }
                }
            }
            HeapTy::Value(fn_ty @ NativeTy::Fn(..)) => {
                for capture in lambdas.get(fn_ty).into_iter().flatten() {
                    targets.push(HeapTy::Cell(capture.clone()));
                }
            }
            HeapTy::Value(_) => {}
        }
        for target in targets {
            let to = intern(target, &mut nodes, &mut edges, &mut work);
            if !edges[from].contains(&to) {
                edges[from].push(to);
            }
        }
    }

    // Strongly connected components; a node is tracked when its component
    // has a cycle.
    let components = tarjan(&edges);
    let mut tracked = FxHashSet::default();
    let mut decisions: Vec<(usize, bool, String)> = Vec::new();
    let describe_heap = |heap: &HeapTy<'db>| match heap {
        HeapTy::Value(ty) => describe(ty),
        HeapTy::Cell(ty) => format!("cell of {}", describe(ty)),
    };
    for component in &components {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        for &node in component {
            let reason = if cyclic {
                let mut members: Vec<String> = component
                    .iter()
                    .map(|&member| describe_heap(&nodes[member]))
                    .collect();
                members.sort();
                if members.len() > 6 {
                    let more = members.len() - 6;
                    members.truncate(6);
                    members.push(format!("{more} more"));
                }
                format!("on a cycle with: {}", members.join(", "))
            } else if edges[node].is_empty() {
                "holds no handles".to_owned()
            } else {
                format!(
                    "reaches {} heap type(s), none back to itself",
                    edges[node].len()
                )
            };
            if cyclic {
                tracked.insert(nodes[node].clone());
            }
            decisions.push((node, cyclic, reason));
        }
    }
    decisions.sort_by_key(|(node, ..)| *node);
    let types = decisions
        .into_iter()
        .map(|(node, tracked, reason)| TrackedType {
            described: describe_heap(&nodes[node]),
            tracked,
            reason,
        })
        .collect();
    Tracking {
        tracked,
        report: MemoryReport {
            types,
            sites: AllocSites::default(),
        },
    }
}

/// The heap types a value of type `ty` holds handles to directly: itself
/// for a heap type, through `T | null`, union members, interface
/// implementors and an iterator's array otherwise.
fn heap_tys<'db>(
    ty: &NativeTy<'db>,
    all_classes: &[NativeTy<'db>],
    out: &mut dyn FnMut(HeapTy<'db>),
) {
    match ty {
        NativeTy::Array(_) | NativeTy::Map(..) | NativeTy::Class(_) | NativeTy::Fn(..) => {
            out(HeapTy::Value(ty.clone()));
        }
        NativeTy::Option(inner) => heap_tys(inner, all_classes, out),
        NativeTy::Union(members) => {
            for member in members {
                heap_tys(member, all_classes, out);
            }
        }
        NativeTy::Interface(instance) => {
            for implementor in &instance.implementors {
                heap_tys(implementor, all_classes, out);
            }
        }
        NativeTy::ArrayIter(element) => out(HeapTy::Value(NativeTy::Array(element.clone()))),
        // A thrown value can be any class instance the program throws.
        NativeTy::Thrown => {
            for class in all_classes {
                out(HeapTy::Value(class.clone()));
            }
        }
        NativeTy::Int
        | NativeTy::Bool
        | NativeTy::Float
        | NativeTy::Bigint
        | NativeTy::Str
        | NativeTy::Null
        | NativeTy::Enum(_) => {}
    }
}

/// Tarjan's strongly connected components, iteratively.
fn tarjan(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let n = edges.len();
    let mut index = vec![UNSEEN; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut next = 0;
    for start in 0..n {
        if index[start] != UNSEEN {
            continue;
        }
        // (node, next edge to look at)
        let mut call: Vec<(usize, usize)> = vec![(start, 0)];
        index[start] = next;
        low[start] = next;
        next += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(&(node, edge)) = call.last() {
            if let Some(&to) = edges[node].get(edge) {
                if let Some(top) = call.last_mut() {
                    top.1 += 1;
                }
                if index[to] == UNSEEN {
                    index[to] = next;
                    low[to] = next;
                    next += 1;
                    stack.push(to);
                    on_stack[to] = true;
                    call.push((to, 0));
                } else if on_stack[to] {
                    low[node] = low[node].min(index[to]);
                }
            } else {
                call.pop();
                if let Some(&(parent, _)) = call.last() {
                    low[parent] = low[parent].min(low[node]);
                }
                if low[node] == index[node] {
                    let mut component = Vec::new();
                    while let Some(member) = stack.pop() {
                        on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    components.push(component);
                }
            }
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::tarjan;

    #[test]
    fn components_find_cycles_and_self_loops() {
        // 0 -> 1 -> 2 -> 0, 3 -> 3, 4 -> 0.
        let edges = vec![vec![1], vec![2], vec![0], vec![3], vec![0]];
        let mut components: Vec<Vec<usize>> = tarjan(&edges)
            .into_iter()
            .map(|mut component| {
                component.sort_unstable();
                component
            })
            .collect();
        components.sort();
        assert_eq!(components, vec![vec![0, 1, 2], vec![3], vec![4]]);
    }
}
