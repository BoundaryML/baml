//! Recorded-table assertions (S16): the fattened `InferenceResult`
//! carries MIR's inputs alongside the types; these tests pin each
//! table's entries on small bodies, keyed by source snippet. The
//! corpus-scale check is the differential MIR gate; these pin the
//! per-road recording semantics the gate builds on.

use baml_compiler2_hir_ty::infer::{MemberResolution, infer_body};

/// Every recorded member resolution in `source`, as sorted
/// `(snippet, kind)` pairs - the snippet is the recorded expression's
/// exact text, so assertions read as the language they pin.
fn member_resolutions(source: &str) -> Vec<(String, String)> {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let mut out = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        for (&expr, resolution) in &result.member_resolutions {
            let range = source_map.expr_span(expr);
            let snippet = source[range].to_string();
            out.push((snippet, kind(resolution).to_string()));
        }
    }
    out.sort();
    out
}

fn kind(resolution: &MemberResolution<'_>) -> &'static str {
    match resolution {
        MemberResolution::Field { .. } => "Field",
        MemberResolution::Variant { .. } => "Variant",
        MemberResolution::Free { .. } => "Free",
        MemberResolution::BoundMethod { .. } => "BoundMethod",
        MemberResolution::UnboundMethod { .. } => "UnboundMethod",
        MemberResolution::InterfaceVirtualMethod { .. } => "InterfaceVirtualMethod",
        MemberResolution::InterfaceConcreteMethod { .. } => "InterfaceConcreteMethod",
        MemberResolution::InterfaceVirtualField { .. } => "InterfaceVirtualField",
    }
}

#[test]
fn records_field_and_bound_method() {
    let resolutions = member_resolutions(
        r#"
class Person {
    name string
    function get_name(self) -> string throws never {
        self.name
    }
}
function mr_use(p: Person) -> string throws never {
    let n = p.name;
    p.get_name()
}
"#,
    );
    assert!(
        resolutions.contains(&("p.name".into(), "Field".into())),
        "field read records Field: {resolutions:?}"
    );
    assert!(
        resolutions.contains(&("self.name".into(), "Field".into())),
        "self field read records Field: {resolutions:?}"
    );
    assert!(
        resolutions.contains(&("p.get_name".into(), "BoundMethod".into())),
        "method callee records BoundMethod at the CALLEE expr: {resolutions:?}"
    );
}

#[test]
fn records_free_and_unbound() {
    let resolutions = member_resolutions(
        r#"
class Counter {
    function magic() -> int throws never {
        42
    }
}
function mr_free(x: int) -> int throws never {
    x
}
function mr_calls() -> int throws never {
    let xs = Counter.magic();
    let f = Counter.magic;
    mr_free(1)
}
"#,
    );
    assert!(
        resolutions.contains(&("mr_free".into(), "Free".into())),
        "direct call records Free at the callee: {resolutions:?}"
    );
    // The call spelling and the value spelling both record the static
    // as UnboundMethod (no receiver; `self` stays a parameter).
    assert_eq!(
        resolutions
            .iter()
            .filter(|(snippet, kind)| snippet == "Counter.magic" && kind == "UnboundMethod")
            .count(),
        2,
        "class statics record UnboundMethod in call and value position: {resolutions:?}"
    );
}

#[test]
fn records_interface_dispatch_modes() {
    let resolutions = member_resolutions(
        r#"
interface Named {
    name string
    function describe(self) -> string throws never
}
class Dog {
    dog_name string
    implements Named {
        name links dog_name
        function describe(self) -> string throws never {
            "dog"
        }
    }
}
function mr_virtual(n: Named) -> string throws never {
    let field = n.name;
    n.describe()
}
function mr_concrete(d: Dog) -> string throws never {
    d.describe()
}
"#,
    );
    assert!(
        resolutions.contains(&("n.name".into(), "InterfaceVirtualField".into())),
        "existential field read is virtual: {resolutions:?}"
    );
    assert!(
        resolutions.contains(&("n.describe".into(), "InterfaceVirtualMethod".into())),
        "existential method call is a virtual slot: {resolutions:?}"
    );
    // The item tree hoists implements-block methods into the class's
    // method list, so the class-inherent road resolves the concrete
    // call and the record is BoundMethod carrying the impl method's
    // FunctionLoc (the callable MIR must emit). TIR spells this
    // InterfaceConcreteMethod with the impl block; whether MIR needs
    // that distinction surfaces at the differential gate.
    assert!(
        resolutions.contains(&("d.describe".into(), "BoundMethod".into())),
        "concrete receiver resolves through the hoisted class method: {resolutions:?}"
    );
}
