//! Chidamber-Kemerer style metrics for classes and interfaces, built from the
//! per-method results.

use std::collections::{HashMap, HashSet};

use baml_base::SourceFile;
use baml_compiler2_hir::{
    item_data::{
        class_data, class_impls, file_classes, file_interfaces, impl_block_data, interface_data,
    },
    loc::{ClassLoc, DeclRef, FunctionLoc},
};

use crate::{
    TypeKind, TypeMetrics,
    calls::{TypeRef, Uses},
    count,
};

/// What the type metrics need from one analyzed method.
pub(crate) struct MethodFacts<'a, 'db> {
    pub cyclomatic: u32,
    pub uses: &'a Uses<'db>,
}

pub(crate) fn type_metrics<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    files: &[SourceFile],
    methods: &HashMap<FunctionLoc<'db>, MethodFacts<'_, 'db>>,
) -> Vec<TypeMetrics> {
    let mut out = Vec::new();
    for &file in files {
        for &class in file_classes(db, file) {
            let data = class_data(db, class);
            let impls = class_impls(db, class);
            let mut own: Vec<FunctionLoc<'db>> = data.methods.clone();
            for &block in impls {
                own.extend(impl_block_data(db, block).methods.iter().copied());
            }
            let coupling = coupling(TypeRef::Class(DeclRef::Source(class)), &own, methods);
            out.push(TypeMetrics {
                name: data.name.clone(),
                file,
                kind: TypeKind::Class,
                fields: count(data.fields.len()),
                nom: coupling.nom,
                wmc: coupling.wmc,
                rfc: coupling.rfc,
                cbo: coupling.cbo,
                lcom4: Some(lcom4(class, &own, methods)),
                interfaces_implemented: count(impls.len()),
            });
        }
        for &interface in file_interfaces(db, file) {
            let data = interface_data(db, interface);
            let coupling = coupling(
                TypeRef::Interface(DeclRef::Source(interface)),
                &data.methods,
                methods,
            );
            out.push(TypeMetrics {
                name: data.name.clone(),
                file,
                kind: TypeKind::Interface,
                fields: count(data.fields.len()),
                nom: coupling.nom,
                wmc: coupling.wmc,
                rfc: coupling.rfc,
                cbo: coupling.cbo,
                lcom4: None,
                interfaces_implemented: count(data.requires.len()),
            });
        }
    }
    out
}

struct Coupling {
    nom: u32,
    wmc: u32,
    rfc: u32,
    cbo: u32,
}

/// NOM, WMC, RFC and CBO for the type `this` with methods `own`.
fn coupling<'db>(
    this: TypeRef<'db>,
    own: &[FunctionLoc<'db>],
    methods: &HashMap<FunctionLoc<'db>, MethodFacts<'_, 'db>>,
) -> Coupling {
    let mut response: HashSet<_> = own.iter().map(|&m| DeclRef::Source(m)).collect();
    let mut coupled = HashSet::new();
    let mut wmc = 0;
    for facts in own.iter().filter_map(|m| methods.get(m)) {
        wmc += facts.cyclomatic;
        response.extend(facts.uses.callees.iter().copied());
        coupled.extend(facts.uses.coupled.iter().copied());
    }
    coupled.remove(&this);
    Coupling {
        nom: count(own.len()),
        wmc,
        rfc: count(response.len()),
        cbo: count(coupled.len()),
    }
}

/// LCOM4 (Hitz and Montazeri): the number of connected components among the
/// class's methods, where two methods connect when they use a common field of
/// the class or one calls the other.
fn lcom4<'db>(
    class: ClassLoc<'db>,
    own: &[FunctionLoc<'db>],
    methods: &HashMap<FunctionLoc<'db>, MethodFacts<'_, 'db>>,
) -> u32 {
    let analyzed: Vec<(FunctionLoc<'db>, &Uses<'db>)> = own
        .iter()
        .filter_map(|&m| methods.get(&m).map(|facts| (m, facts.uses)))
        .collect();
    let class = DeclRef::Source(class);
    let connected = |a: &Uses<'db>, a_loc: FunctionLoc<'db>, b: &Uses<'db>, b_loc| {
        a.callees.contains(&DeclRef::Source(b_loc))
            || b.callees.contains(&DeclRef::Source(a_loc))
            || a.fields
                .iter()
                .any(|field| field.0 == class && b.fields.contains(field))
    };

    let mut component = vec![usize::MAX; analyzed.len()];
    let mut components = 0;
    for start in 0..analyzed.len() {
        if component[start] != usize::MAX {
            continue;
        }
        component[start] = components;
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            for j in 0..analyzed.len() {
                if component[j] == usize::MAX
                    && connected(analyzed[i].1, analyzed[i].0, analyzed[j].1, analyzed[j].0)
                {
                    component[j] = components;
                    stack.push(j);
                }
            }
        }
        components += 1;
    }
    count(components)
}
