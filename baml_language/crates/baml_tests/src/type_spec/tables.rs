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
        MemberResolution::CompiledFree { .. } => "CompiledFree",
        MemberResolution::BoundMethod { .. } => "BoundMethod",
        MemberResolution::UnboundMethod { .. } => "UnboundMethod",
        MemberResolution::CompiledBoundMethod { .. } => "CompiledBoundMethod",
        MemberResolution::CompiledUnboundMethod { .. } => "CompiledUnboundMethod",
        MemberResolution::InterfaceVirtualMethod { .. } => "InterfaceVirtualMethod",
        MemberResolution::InterfaceConcreteMethod { .. } => "InterfaceConcreteMethod",
        MemberResolution::InterfaceVirtualField { .. } => "InterfaceVirtualField",
        MemberResolution::CompiledInterfaceVirtualField { .. } => "CompiledInterfaceVirtualField",
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
                    baml_compiler2_hir_ty::infer::ParamBinding::Provided {
                        param_index, ..
                    } => {
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

#[test]
fn records_function_adapter() {
    let source = r#"
function ea_flex(a: int, b: int = 2) -> int throws never {
    a
}
function ea_take(f: (x: int, y: int = 9) -> int throws never) -> int throws never {
    f(1)
}
function ea_use() -> int throws never {
    ea_take(ea_flex)
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let mut adjustments = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        for (&expr, steps) in &result.expr_adjustments {
            let range = source_map.expr_span(expr);
            adjustments.push((source[range].to_string(), steps.len()));
        }
    }
    adjustments.sort();
    assert_eq!(
        adjustments,
        vec![("ea_flex".to_string(), 1)],
        "optional-param name drift records a FunctionAdapter at the checked expr"
    );
}

#[test]
fn infers_parameter_defaults_as_own_root() {
    let source = r#"
function pd_take(a: int, tag: string = "t", n: int = 1 + 2, bad: int = "x") -> int throws never {
    a
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let functions = baml_compiler2_ppir::item_data::file_functions(&db, file);
    let function = *functions.first().expect("one function");
    let owner = baml_compiler2_hir::body::BodyOwnerId::ParameterDefaults(function);
    let result = infer_body(&db, owner);
    let defaults = baml_compiler2_ppir::function_parameter_defaults(&db, function);
    let rendered: Vec<String> = defaults
        .params
        .iter()
        .enumerate()
        .filter_map(|(index, default)| {
            let default = default.as_ref()?;
            let expr = default.expr.expr();
            let ty = result
                .type_of_expr
                .get(&expr)
                .map(|ty| ty.to_plain().render_canonical())
                .unwrap_or_else(|| "<missing>".into());
            let mismatch = if result.type_mismatches.contains_key(&expr) {
                " MISMATCH"
            } else {
                ""
            };
            Some(format!("{index}: {ty}{mismatch}"))
        })
        .collect();
    assert_eq!(
        rendered,
        vec![
            "1: \"t\"".to_string(),
            "2: 3".to_string(),
            "3: \"x\" MISMATCH".to_string(),
        ],
        "defaults check against their parameter's declared type"
    );
}

#[test]
fn call_plan_waits_for_sibling_vars() {
    // The finish fixpoint must not commit a bounded var from its ground
    // lowers while a sibling var occurring in a DEFERRED lower is still
    // solvable (rustc solves fallback only at quiescence): deep_equals'
    // T here has lowers {"caught", "caught" | ?T_gen[]} and must wait
    // for ?T_gen = int, giving string | int[] - not commit to string
    // from the ground subset and silently fail the deferred bound.
    let source = r#"
function de_eq<T>(a: T, b: T) -> bool throws never {
    true
}
function de_gen<T>(x: T) -> T[] throws string {
    [x]
}
function de_probe() -> bool throws never {
    de_eq(de_gen(1) catch (e) {
        string => "caught"
    }, "caught")
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
            let snippet = &source[source_map.expr_span(call)];
            let args: Vec<String> = plan
                .type_args
                .iter()
                .map(|ty| ty.to_plain().render_canonical())
                .collect();
            plans.push(format!(
                "{} -> {args:?}",
                snippet.split('(').next().unwrap_or(snippet)
            ));
        }
    }
    plans.sort();
    assert_eq!(
        plans,
        vec![
            "de_eq -> [\"string | int[]\"]".to_string(),
            "de_gen -> [\"int\"]".to_string(),
        ],
        "solver committed a var while a sibling in its deferred lowers was still solvable"
    );
}

#[test]
fn call_plan_effect_solves_from_deferred_lambda() {
    // Goals before solving (rustc's fulfillment-before-defaults, round
    // ordering): E's only REGISTERED bound is the declared-throws upper
    // (`throws unknown`), while the lambda argument's `throws never`
    // lower rides a deferred sub - draining goals first lands the lower
    // before E commits, so E = never, not the minimum-upper unknown.
    let source = r#"
function ir_probe() -> int throws unknown {
    let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
    it.reduce((a: int, x: int) -> int { a + x }, 0)
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
            let snippet = &source[source_map.expr_span(call)];
            if !snippet.starts_with("it.reduce") {
                continue;
            }
            plans.push(
                plan.type_args[plan.own_offset..]
                    .iter()
                    .map(|ty| ty.to_plain().render_canonical())
                    .collect::<Vec<_>>(),
            );
        }
    }
    assert_eq!(
        plans,
        vec![vec!["int".to_string(), "never".to_string()]],
        "reduce's own args must solve A = int, E = never"
    );
}

#[test]
fn assign_target_chain_records() {
    // A member/place assignment target types as an ordinary place
    // expression (r-a's infer_assignee_expr): the chain's types and
    // resolutions record - MIR's slot road resolves the store through
    // them - and the field's type is the value's expectation.
    let source = r#"
class FaNode {
    value int
}
function fa_probe(n: FaNode) -> int throws never {
    n.value = 5;
    n.value
}
"#;
    let resolutions = member_resolutions(source);
    assert_eq!(
        resolutions
            .iter()
            .filter(|(snippet, kind)| snippet == "n.value" && kind == "Field")
            .count(),
        2,
        "the assign TARGET and the read both record Field: {resolutions:?}"
    );
}

#[test]
fn union_field_access_records_virtual_view() {
    // Proper dyn (ruled 2026-08-11): a field access on a UNION receiver
    // whose members all share one realized declaring-interface view
    // records InterfaceVirtualField through that view - TIR's
    // "authoritative for union receivers" rule - so MIR emits the
    // virtual read instead of falling back to a tag switch.
    let source = r#"
interface UvHasSound {
    sound string
}
class UvCat {
    sound string
    implements UvHasSound {}
}
class UvDog {
    sound string
    implements UvHasSound {}
}
function uv_probe(animal: UvCat | UvDog) -> string throws never {
    animal.sound
}
"#;
    let resolutions = member_resolutions(source);
    assert!(
        resolutions.contains(&("animal.sound".into(), "InterfaceVirtualField".into())),
        "union field access records the shared virtual view: {resolutions:?}"
    );
}

#[test]
fn pattern_ascription_records_written_nominal() {
    // Ruling 3 (S15): bindings record the WRITTEN pattern type; aliases
    // are nominal by design. The scrutinee's structural analysis may
    // expand the alias transiently, but the recorded type is the
    // declared form (rustc's user_provided_types discipline - the
    // written annotation is the artifact, normalization never
    // overwrites it).
    let source = r#"
function pa_probe() -> int {
    let j: baml.json.json = baml.json.parse("[1, 2, 3]");
    match (j) {
        let arr: baml.json.json[] => arr.length(),
        _ => -1
    }
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.add_file("test.baml", source);
    let mut renders = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        let body = baml_compiler2_ppir::body(&db, owner);
        let Some(arena) = body.expr_body() else {
            continue;
        };
        for (pat_id, _) in arena.patterns.iter() {
            let snippet = &source[source_map.pattern_span(pat_id)];
            if snippet.starts_with("let arr")
                && let Some(ty) = result.type_of_pat.get(&pat_id)
            {
                renders.push(ty.to_plain().render_canonical());
            }
        }
    }
    assert_eq!(
        renders,
        vec!["baml.json.json[]".to_string()],
        "the ascribed binding records the written nominal type"
    );
}
