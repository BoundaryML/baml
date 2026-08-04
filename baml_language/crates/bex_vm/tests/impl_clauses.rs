//! The VM as a clause supplier for the type-relation solver.
//!
//! These assert the shape and the order of what
//! [`TypeContext::for_each_clause`] hands back, both of which are contract rather
//! than convenience: the solver matches a receiver against every clause of an
//! interface, so a missing bound would let a bounded impl apply where it must not,
//! and a non-deterministic order would let two builds of one program resolve the
//! same call differently.

use std::{
    ops::ControlFlow,
    sync::{Arc, atomic::AtomicBool},
};

use baml_project::testing::compile_source;
use baml_type::{ClauseId, TyTemplate, TypeName, normalize::TypeContext};
use bex_vm::BexVm;

const SOURCE: &str = r#"
interface Shape {
    function area(self) -> int throws never
}

interface Boxed {
    function unwrap(self) -> int throws never
}

class Square {
    side int
    implements Shape {
        function area(self) -> int throws never { self.side * self.side }
    }
}

class Circle {
    r int
    implements Shape {
        function area(self) -> int throws never { self.r * self.r * 3 }
    }
}

class Wrap<T> {
    item T
}

/// A bounded, generic clause: applies only where its parameter is itself a `Shape`.
implements<T extends Shape> Boxed for Wrap<T> {
    function unwrap(self) -> int throws never { 0 }
}

function main() -> int throws never { 0 }
"#;

fn vm() -> BexVm {
    let program = compile_source(SOURCE);
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

/// An owned snapshot of one supplied clause, for inspection after the walk.
struct ClauseShape {
    id: ClauseId,
    num_vars: usize,
    self_pattern: TyTemplate,
    bound_names: Vec<Vec<String>>,
}

/// The head name of a for-pattern, which is what identifies a clause here.
fn head_name(pattern: &TyTemplate) -> Option<String> {
    match pattern {
        TyTemplate::Class(qtn, ..) | TyTemplate::Enum(qtn, ..) => {
            Some(qtn.name().as_str().to_owned())
        }
        _ => None,
    }
}

/// The source above declares its interfaces at the root, so their qualified names are
/// local ones. Collected here because these tests inspect the whole candidate set; real
/// consumers short-circuit instead, which is what `ControlFlow` is for.
fn clauses_of(vm: &BexVm, interface: &str) -> Vec<ClauseShape> {
    let mut out = Vec::new();
    vm.for_each_clause(
        &TypeName::local(baml_type::Name::new(interface)),
        &mut |clause| {
            out.push(ClauseShape {
                id: clause.id,
                num_vars: clause.num_vars,
                self_pattern: clause.self_pattern.clone(),
                bound_names: clause
                    .bounds
                    .iter()
                    .map(|set| {
                        set.iter()
                            .map(|bound| bound.name.name().as_str().to_owned())
                            .collect()
                    })
                    .collect(),
            });
            ControlFlow::Continue(())
        },
    );
    out
}

#[test]
fn every_implementation_of_an_interface_is_supplied() {
    let vm = vm();
    let clauses = clauses_of(&vm, "Shape");

    let mut heads: Vec<String> = clauses
        .iter()
        .filter_map(|clause| head_name(&clause.self_pattern))
        .collect();
    heads.sort();
    heads.dedup();
    assert!(
        heads.iter().any(|h| h == "Square") && heads.iter().any(|h| h == "Circle"),
        "expected both implementors among {heads:?}"
    );
}

#[test]
fn a_generic_clause_carries_its_arity_and_its_bounds() {
    let vm = vm();
    let clauses = clauses_of(&vm, "Boxed");

    let wrap = clauses
        .iter()
        .find(|clause| head_name(&clause.self_pattern).as_deref() == Some("Wrap"))
        .expect("the `Wrap<T>` clause");

    // One parameter, so a match against this clause needs a one-slot frame…
    assert_eq!(wrap.num_vars, 1);
    assert_eq!(wrap.bound_names.len(), 1);
    // …and that parameter is bounded, which is the half of the clause that has to be
    // discharged as an obligation rather than matched.
    assert_eq!(
        wrap.bound_names[0].as_slice(),
        ["Shape"],
        "`T extends Shape` should be recorded"
    );

    // The pattern addresses the parameter positionally, which is what makes the clause
    // form independent of how the impl spelled its generics.
    assert!(
        matches!(&wrap.self_pattern, TyTemplate::Class(_, args, _)
            if matches!(args.as_slice(), [TyTemplate::TypeArgRef(0)])),
        "expected `Wrap<#0>`, got {:?}",
        wrap.self_pattern
    );
}

#[test]
fn an_unimplemented_interface_supplies_nothing() {
    let vm = vm();
    // A name no package declares: the supplier must come back empty rather than
    // failing, since "nobody implements this" is a legitimate answer.
    assert_eq!(clauses_of(&vm, "NoSuchInterface").len(), 0);
}

#[test]
fn enumeration_order_is_stable() {
    let vm = vm();
    let (first, second) = (clauses_of(&vm, "Shape"), clauses_of(&vm, "Shape"));
    let ids = |clauses: &[ClauseShape]| clauses.iter().map(|c| c.id).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&second));
}

#[test]
fn a_break_stops_the_walk() {
    let vm = vm();
    let mut seen = 0;
    vm.for_each_clause(
        &TypeName::local(baml_type::Name::new("Shape")),
        &mut |_clause| {
            seen += 1;
            ControlFlow::Break(())
        },
    );
    // Shape has at least two implementations (asserted above); a breaking visitor
    // must see exactly one.
    assert_eq!(seen, 1);
}
