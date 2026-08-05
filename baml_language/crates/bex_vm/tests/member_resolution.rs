//! Hand-derived-verdict suite for runtime interface resolution, over programs
//! authored to pin each behavior dispatch depends on: first-match selection,
//! literal/enum-variant folding (top level only), nested and recursive bound
//! discharge, blanket impls (wrapper-headed and bare), existential bound
//! discharge, union- and recursive-alias-spelled arguments (equirecursive
//! identity), interface-argument keying with empty/exact/wrong requests, and
//! associated-binding width. Every membership goal asserts an explicit expected
//! verdict and every selection asserts its expected bindings, so a drift in
//! either direction — lost proofs or invented ones — fails loudly.
//!
//! (During the solver migration this suite also ran every goal differentially
//! against the pre-session resolver; that baseline certified at exact parity —
//! same verdicts, same rules by identity, same bindings — and was then deleted.
//! The hand-derived verdicts here are the permanent resolution suite.)
//!
//! The template matcher's own width/backtracking rules are pinned separately by
//! `baml_type`'s matcher tests.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use baml_type::{Freshness, Literal, Name, RealizedTy, TyAttr, TypeName};
use bex_vm::{
    BexVm,
    package_baml::testing::{resolve_implements_rule, type_implements},
};

fn vm(source: &str) -> BexVm {
    BexVm::from_program(compile_source(source), Arc::new(AtomicBool::new(false)))
        .expect("from_program")
}

fn tn(name: &str) -> TypeName {
    TypeName::local(Name::new(name))
}

fn attr() -> TyAttr {
    TyAttr::default()
}

fn class(name: &str, args: Vec<RealizedTy>) -> RealizedTy {
    RealizedTy::Class(tn(name), args, attr())
}

fn union(members: Vec<RealizedTy>) -> RealizedTy {
    RealizedTy::Union(members, attr())
}

fn list(inner: RealizedTy) -> RealizedTy {
    RealizedTy::List(Box::new(inner), attr())
}

fn alias(name: &str) -> RealizedTy {
    RealizedTy::TypeAlias(tn(name), attr())
}

fn existential(name: &str) -> RealizedTy {
    RealizedTy::Interface(tn(name), Vec::new(), Vec::new(), attr())
}

fn int() -> RealizedTy {
    RealizedTy::Int { attr: attr() }
}

fn string() -> RealizedTy {
    RealizedTy::String { attr: attr() }
}

fn lit(value: i64) -> RealizedTy {
    RealizedTy::Literal(Literal::Int(value), Freshness::Regular, attr())
}

/// One membership question with its hand-derived verdict.
struct Membership {
    subject: RealizedTy,
    iface: &'static str,
    args: Vec<RealizedTy>,
    assoc: Vec<(Name, RealizedTy)>,
    expect: bool,
}

impl Membership {
    fn plain(subject: RealizedTy, iface: &'static str, expect: bool) -> Self {
        Self {
            subject,
            iface,
            args: Vec::new(),
            assoc: Vec::new(),
            expect,
        }
    }

    fn with_args(
        subject: RealizedTy,
        iface: &'static str,
        args: Vec<RealizedTy>,
        expect: bool,
    ) -> Self {
        Self {
            subject,
            iface,
            args,
            assoc: Vec::new(),
            expect,
        }
    }

    fn with_assoc(
        subject: RealizedTy,
        iface: &'static str,
        assoc: Vec<(&str, RealizedTy)>,
        expect: bool,
    ) -> Self {
        Self {
            subject,
            iface,
            args: Vec::new(),
            assoc: assoc
                .into_iter()
                .map(|(name, ty)| (Name::new(name), ty))
                .collect(),
            expect,
        }
    }
}

/// One selection question. `expect` is `None` for "no rule applies" and
/// `Some(bindings)` for "a rule is selected, with exactly these bound type args"
/// — every selection in this corpus has fully determined bindings (they are the
/// concrete subterms the for-pattern matched), so which-rule agreement is pinned
/// through them.
struct Selection {
    subject: RealizedTy,
    iface: &'static str,
    args: Vec<RealizedTy>,
    expect: Option<Vec<RealizedTy>>,
}

impl Selection {
    fn new(
        subject: RealizedTy,
        iface: &'static str,
        args: Vec<RealizedTy>,
        expect: Option<Vec<RealizedTy>>,
    ) -> Self {
        Self {
            subject,
            iface,
            args,
            expect,
        }
    }
}

/// Run a corpus over one program, collecting every divergence instead of
/// stopping at the first, so a failure reports the whole picture.
fn run(vm: &BexVm, memberships: &[Membership], selections: &[Selection]) {
    let mut failures: Vec<String> = Vec::new();

    for goal in memberships {
        let iface = tn(goal.iface);
        let got = type_implements(vm, &goal.subject, &iface, &goal.args, &goal.assoc);
        if got != goal.expect {
            failures.push(format!(
                "{} : {}<args: {:?}, assoc: {:?}> expected {}, got {got}",
                goal.subject, goal.iface, goal.args, goal.assoc, goal.expect
            ));
        }
    }

    for goal in selections {
        let iface = tn(goal.iface);
        let got = resolve_implements_rule(vm, &goal.subject, &iface, &goal.args);
        let describe = || format!("select {} : {}<{:?}>", goal.subject, goal.iface, goal.args);
        match (&goal.expect, got) {
            (None, Some(_)) => {
                failures.push(format!("{} expected no rule, got one", describe()));
            }
            (Some(_), None) => {
                failures.push(format!("{} expected a rule, got none", describe()));
            }
            (Some(expected), Some((_, bindings))) if *expected != bindings => {
                failures.push(format!(
                    "{} bound {bindings:?}, expected {expected:?}",
                    describe()
                ));
            }
            _ => {}
        }
    }

    assert!(
        failures.is_empty(),
        "{} divergence(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// Nominal impls, a bounded generic impl, a concrete base + bounded blanket over a
/// wrapper (an inductive recursion with a base case), and literal/variant folding.
#[test]
fn nominal_generic_and_recursive_bounds() {
    let vm = vm(r#"
interface Shape {
    function area(self) -> int throws never
}

interface Boxed {
    function unwrap(self) -> int throws never
}

interface Marker {
    function mark(self) -> int throws never
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

enum Color {
    Red
    Green
}

implements<T extends Shape> Boxed for Wrap<T> {
    function unwrap(self) -> int throws never { 0 }
}

implements Marker for int {
    function mark(self) -> int throws never { 0 }
}

implements Marker for Color {
    function mark(self) -> int throws never { 1 }
}

implements<T extends Marker> Marker for Wrap<T> {
    function mark(self) -> int throws never { 2 }
}

function main() -> int throws never { 0 }
"#);
    let square = || class("Square", Vec::new());
    let wrap = |inner: RealizedTy| class("Wrap", vec![inner]);

    let memberships = [
        Membership::plain(square(), "Shape", true),
        Membership::plain(class("Circle", Vec::new()), "Shape", true),
        Membership::plain(int(), "Shape", false),
        // Bounded generic impl: the bound is discharged (or not) as a nested goal.
        Membership::plain(wrap(square()), "Boxed", true),
        Membership::plain(wrap(int()), "Boxed", false),
        // `Wrap<Square> : Shape` fails, so the nesting does not chain.
        Membership::plain(wrap(wrap(square())), "Boxed", false),
        // The Marker recursion grounds out in the `int` base case.
        Membership::plain(wrap(int()), "Marker", true),
        Membership::plain(wrap(wrap(int())), "Marker", true),
        Membership::plain(wrap(wrap(wrap(string()))), "Marker", false),
        // Literal and enum-variant subjects fold to their bases — at the top level
        // only: a literal *argument* stays literal, and `Wrap<1> : Marker` needs
        // `1 : Marker`, which folds to `int : Marker` at its own goal.
        Membership::plain(lit(1), "Marker", true),
        Membership::plain(
            RealizedTy::EnumVariant(tn("Color"), Name::new("Red"), attr()),
            "Marker",
            true,
        ),
        Membership::plain(wrap(lit(1)), "Marker", true),
        // A union subject implements nothing: unions can be *subtypes* of an
        // existential but are never implementors (only concrete types implement).
        Membership::plain(
            union(vec![square(), class("Circle", Vec::new())]),
            "Shape",
            false,
        ),
        // An existential subject is not an implementor either (no blanket admits it
        // here: the Marker blanket's `for` head is `Wrap<T>`, not bare `T`).
        Membership::plain(existential("Marker"), "Marker", false),
    ];
    let selections = [
        Selection::new(square(), "Shape", Vec::new(), Some(Vec::new())),
        Selection::new(int(), "Shape", Vec::new(), None),
        Selection::new(wrap(square()), "Boxed", Vec::new(), Some(vec![square()])),
        Selection::new(wrap(int()), "Boxed", Vec::new(), None),
        Selection::new(int(), "Marker", Vec::new(), Some(Vec::new())),
        Selection::new(
            wrap(wrap(int())),
            "Marker",
            Vec::new(),
            Some(vec![wrap(int())]),
        ),
        Selection::new(wrap(string()), "Marker", Vec::new(), None),
        // The literal folds to `int`, selecting the base impl (no generics to bind).
        Selection::new(lit(1), "Marker", Vec::new(), Some(Vec::new())),
    ];
    run(&vm, &memberships, &selections);
}

/// Generic interface arguments (selection keys on them; empty requests match any
/// instantiation), first-match order between two impls of one interface, and
/// associated-binding width (requests may be narrower than the impl provides,
/// never different).
#[test]
fn interface_args_and_associated_bindings() {
    let vm = vm(r#"
interface Convert<T> {
    function conv(self) -> T throws never
}

interface Iter {
    type Item
    type Error
    function next(self) -> int throws never
}

class Foo {
    x int
    implements Convert<int> {
        function conv(self) -> int throws never { self.x }
    }
    implements Iter {
        type Item = int
        type Error = string
        function next(self) -> int throws never { 0 }
    }
}

class Gen<T> {
    item T
    implements Convert<T> {
        function conv(self) -> T throws never { self.item }
    }
}

class Multi {
    implements Convert<int> {
        function conv(self) -> int throws never { 0 }
    }
    implements Convert<string> {
        function conv(self) -> string throws never { "s" }
    }
}

function main() -> int throws never { 0 }
"#);
    let foo = || class("Foo", Vec::new());
    let generic = |arg: RealizedTy| class("Gen", vec![arg]);
    let multi = || class("Multi", Vec::new());

    let memberships = [
        Membership::with_args(foo(), "Convert", Vec::new(), true),
        Membership::with_args(foo(), "Convert", vec![int()], true),
        Membership::with_args(foo(), "Convert", vec![string()], false),
        Membership::with_args(generic(string()), "Convert", vec![string()], true),
        Membership::with_args(generic(string()), "Convert", vec![int()], false),
        Membership::with_args(generic(string()), "Convert", Vec::new(), true),
        Membership::with_args(multi(), "Convert", vec![int()], true),
        Membership::with_args(multi(), "Convert", vec![string()], true),
        Membership::with_args(
            multi(),
            "Convert",
            vec![union(vec![int(), string()])],
            false,
        ),
        // Associated requests: subset satisfied, exact satisfied, mismatch and
        // unknown names refused.
        Membership::with_assoc(foo(), "Iter", vec![("Item", int())], true),
        Membership::with_assoc(
            foo(),
            "Iter",
            vec![("Item", int()), ("Error", string())],
            true,
        ),
        Membership::with_assoc(foo(), "Iter", vec![("Item", string())], false),
        Membership::with_assoc(foo(), "Iter", vec![("Missing", int())], false),
        Membership::plain(foo(), "Iter", true),
    ];
    let selections = [
        // Two applicable clauses under an empty request: first match in the baked
        // order wins (both impls are generic-free, so bindings are empty either way).
        Selection::new(multi(), "Convert", Vec::new(), Some(Vec::new())),
        Selection::new(multi(), "Convert", vec![int()], Some(Vec::new())),
        Selection::new(multi(), "Convert", vec![string()], Some(Vec::new())),
        Selection::new(foo(), "Convert", vec![int()], Some(Vec::new())),
        Selection::new(foo(), "Convert", vec![string()], None),
        Selection::new(
            generic(string()),
            "Convert",
            vec![string()],
            Some(vec![string()]),
        ),
        Selection::new(foo(), "Iter", Vec::new(), Some(Vec::new())),
    ];
    run(&vm, &memberships, &selections);
}

/// Union-typed and recursive-alias-typed impl-head arguments: the pattern side is
/// compared semantically, so member order and alias spelling never matter — the
/// equirecursive identity the canonical algebra provides (`type A = int | A[]` is
/// its own unfolding).
#[test]
fn union_and_recursive_alias_arguments() {
    let vm = vm(r#"
interface Pick {
    function pick(self) -> int throws never
}

interface Deep {
    function deep(self) -> int throws never
}

type Json = int | Json[]

class Jar<T> {
    item T
}

class Pot<T> {
    item T
}

implements Pick for Jar<int | string> {
    function pick(self) -> int throws never { 0 }
}

implements Pick for Pot<Json> {
    function pick(self) -> int throws never { 1 }
}

function main() -> int throws never { 0 }
"#);
    let jar = |arg: RealizedTy| class("Jar", vec![arg]);
    let pot = |arg: RealizedTy| class("Pot", vec![arg]);
    let json_unfolded = || union(vec![int(), list(alias("Json"))]);

    let memberships = [
        Membership::plain(jar(union(vec![int(), string()])), "Pick", true),
        // Reversed member order is the same union.
        Membership::plain(jar(union(vec![string(), int()])), "Pick", true),
        Membership::plain(jar(int()), "Pick", false),
        // The alias name and its one-step unfolding are the same type.
        Membership::plain(pot(alias("Json")), "Pick", true),
        Membership::plain(pot(json_unfolded()), "Pick", true),
        Membership::plain(pot(list(alias("Json"))), "Pick", false),
        Membership::plain(pot(alias("Json")), "Deep", false),
    ];
    let selections = [
        Selection::new(
            jar(union(vec![string(), int()])),
            "Pick",
            Vec::new(),
            Some(Vec::new()),
        ),
        Selection::new(pot(json_unfolded()), "Pick", Vec::new(), Some(Vec::new())),
        Selection::new(pot(list(alias("Json"))), "Pick", Vec::new(), None),
    ];
    run(&vm, &memberships, &selections);
}

/// Existential bound discharge (the single-`Self` dispatchability arm): an
/// interface-existential binding satisfies a bound naming its own interface, both
/// under a wrapper head and under a bare blanket — while never *implementing* the
/// interface as a subject.
#[test]
fn existential_bound_discharge() {
    let vm = vm(r#"
interface Shape {
    function area(self) -> int throws never
}

interface Viewable {
    function view(self) -> int throws never
}

interface Tagged {
    function tag(self) -> int throws never
}

class Square {
    side int
    implements Shape {
        function area(self) -> int throws never { self.side * self.side }
    }
}

class Holder<T> {
    item T
}

implements<T extends Shape> Viewable for Holder<T> {
    function view(self) -> int throws never { 0 }
}

implements<T extends Shape> Tagged for T {
    function tag(self) -> int throws never { 1 }
}

function main() -> int throws never { 0 }
"#);
    let square = || class("Square", Vec::new());
    let holder = |arg: RealizedTy| class("Holder", vec![arg]);

    let memberships = [
        Membership::plain(holder(square()), "Viewable", true),
        // The bound `T extends Shape` is discharged by the existential itself.
        Membership::plain(holder(existential("Shape")), "Viewable", true),
        Membership::plain(holder(int()), "Viewable", false),
        // The bare blanket admits every Shape implementor — and the existential,
        // through the same dispatchability arm.
        Membership::plain(square(), "Tagged", true),
        Membership::plain(existential("Shape"), "Tagged", true),
        Membership::plain(int(), "Tagged", false),
        Membership::plain(holder(square()), "Tagged", false),
        // But an existential subject still does not implement its own interface:
        // membership needs a concrete impl, and no blanket here has a bare head
        // for Shape.
        Membership::plain(existential("Shape"), "Shape", false),
    ];
    let selections = [
        Selection::new(
            holder(existential("Shape")),
            "Viewable",
            Vec::new(),
            Some(vec![existential("Shape")]),
        ),
        Selection::new(
            existential("Shape"),
            "Tagged",
            Vec::new(),
            Some(vec![existential("Shape")]),
        ),
        Selection::new(existential("Shape"), "Shape", Vec::new(), None),
    ];
    run(&vm, &memberships, &selections);
}

/// A bound-recursion chain deep enough to exercise the goal stack but far under
/// the depth cap: verdicts, selections, and bindings must hold at every level.
#[test]
fn deep_bound_chains() {
    let vm = vm(r#"
interface Deep {
    function deep(self) -> int throws never
}

class Wrap<T> {
    item T
}

implements Deep for int {
    function deep(self) -> int throws never { 0 }
}

implements<T extends Deep> Deep for Wrap<T> {
    function deep(self) -> int throws never { 1 }
}

function main() -> int throws never { 0 }
"#);
    let wrap = |inner: RealizedTy| class("Wrap", vec![inner]);
    let nested = |depth: usize, core: RealizedTy| (0..depth).fold(core, |ty, _| wrap(ty));

    let memberships = [
        Membership::plain(nested(8, int()), "Deep", true),
        Membership::plain(nested(8, string()), "Deep", false),
        Membership::plain(nested(1, int()), "Deep", true),
    ];
    let selections = [
        Selection::new(
            nested(8, int()),
            "Deep",
            Vec::new(),
            Some(vec![nested(7, int())]),
        ),
        Selection::new(nested(8, string()), "Deep", Vec::new(), None),
    ];
    run(&vm, &memberships, &selections);
}
