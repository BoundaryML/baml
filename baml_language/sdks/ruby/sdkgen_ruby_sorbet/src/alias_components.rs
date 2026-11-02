//! Alias cycles, computed once per generation without expanding their bodies.
use std::collections::{BTreeMap, BTreeSet};

use baml_sdkgen_types::{Name, Symbol, SymbolPool, Ty, visit_type};

pub(crate) fn components(pool: &SymbolPool) -> BTreeMap<Name, usize> {
    let mut edges: BTreeMap<Name, BTreeSet<Name>> = pool
        .iter()
        .filter(|(_, symbol)| matches!(symbol, Symbol::TypeAlias(_)))
        .map(|(name, _)| (name.clone(), BTreeSet::new()))
        .collect();
    for (name, symbol) in pool {
        if let Symbol::TypeAlias(alias) = symbol {
            let mut targets = BTreeSet::new();
            visit_type(&alias.resolves_to, &mut |ty| {
                if let Ty::TypeAlias(target) = ty
                    && edges.contains_key(target)
                {
                    targets.insert(target.clone());
                }
            });
            edges.insert(name.clone(), targets);
        }
    }
    let mut reverse: BTreeMap<_, BTreeSet<_>> = edges
        .keys()
        .cloned()
        .map(|name| (name, BTreeSet::new()))
        .collect();
    for (source, targets) in &edges {
        for target in targets {
            reverse.get_mut(target).unwrap().insert(source.clone());
        }
    }
    // Kosaraju: finish on the forward graph, then collect on the reverse graph.
    // Explicit stacks also handle long generated alias chains without recursion.
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for root in edges.keys() {
        let mut stack = vec![(root, false)];
        while let Some((name, finished)) = stack.pop() {
            if finished {
                order.push(name);
            } else if seen.insert(name) {
                stack.push((name, true));
                stack.extend(edges[name].iter().map(|target| (target, false)));
            }
        }
    }
    let mut result = BTreeMap::new();
    for root in order.into_iter().rev() {
        if result.contains_key(root) {
            continue;
        }
        let component = result.len();
        let mut stack = vec![root];
        while let Some(name) = stack.pop() {
            if result.contains_key(name) {
                continue;
            }
            result.insert(name.clone(), component);
            stack.extend(reverse[name].iter());
        }
    }
    result
}
