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

#[test]
fn records_variant() {
    let resolutions = member_resolutions(
        r#"
enum Status {
    Active
    Done
}
function mr_variant() -> Status throws never {
    Status.Active
}
"#,
    );
    assert!(
        resolutions.contains(&("Status.Active".into(), "Variant".into())),
        "enum variant value records Variant: {resolutions:?}"
    );
}

#[test]
fn records_path_ladders() {
    let source = r#"
class City {
    name string
}
class Address {
    city City
}
class Person {
    address Address
    function home(self) -> City throws never {
        self.address.city
    }
}
function mr_chain(p: Person) -> string throws never {
    let city_name = p.address.city.name;
    p.address.city.name
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let mut ladders = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        for (&expr, path) in &result.path_resolutions {
            let range = source_map.expr_span(expr);
            let snippet = source[range].to_string();
            let rendered: Vec<String> = path
                .segments
                .iter()
                .map(|segment| {
                    let ty = segment.ty.to_plain().render_canonical();
                    match &segment.resolution {
                        Some(resolution) => format!("{ty}/{}", kind(resolution)),
                        None => ty,
                    }
                })
                .collect();
            ladders.push((snippet, rendered.join(" -> ")));
        }
    }
    ladders.sort();
    ladders.dedup();
    assert_eq!(
        ladders,
        vec![
            (
                "p.address.city.name".to_string(),
                "user.Person -> user.Address/Field -> user.City/Field -> string/Field".to_string()
            ),
            (
                "self.address.city".to_string(),
                "user.Person -> user.Address/Field -> user.City/Field".to_string()
            ),
        ],
        "value-rooted chains record per-segment ladders"
    );
}

#[test]
fn records_call_plans() {
    let source = r#"
function cp_id<T>(x: T) -> T throws never {
    x
}
function cp_defaults(a: int, b: int = 2) -> int throws never {
    a
}
function cp_use() -> int throws never {
    let solved = cp_id(42);
    let named = cp_defaults(b = 5, a = 1);
    cp_defaults(7)
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let mut plans = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        for (&call, plan) in &result.call_plans {
            let range = source_map.expr_span(call);
            let snippet = source[range].to_string();
            let type_args: Vec<String> = plan
                .type_args
                .iter()
                .map(|ty| ty.to_plain().render_canonical())
                .collect();
            let bindings: Vec<String> = plan
                .bindings
                .iter()
                .map(|binding| match binding {
                    baml_compiler2_hir_ty::infer::ParamBinding::Provided { param_index, .. } => {
                        format!("provided:{param_index}")
                    }
                    baml_compiler2_hir_ty::infer::ParamBinding::OmittedDefault {
                        param_index,
                        param_name,
                    } => format!("default:{param_index}:{param_name}"),
                })
                .collect();
            plans.push(format!(
                "{snippet} | type_args [{}] | bindings [{}]",
                type_args.join(", "),
                bindings.join(", ")
            ));
        }
    }
    plans.sort();
    assert_eq!(
        plans,
        vec![
            "cp_defaults(7) | type_args [] | bindings [provided:0, default:1:b]".to_string(),
            "cp_defaults(b = 5, a = 1) | type_args [] | bindings [provided:0, provided:1]"
                .to_string(),
            "cp_id(42) | type_args [int] | bindings [provided:0]".to_string(),
        ],
        "call plans record solved instantiations and param-ordered bindings"
    );
}
