//! Recorded-table assertions (S16): the fattened `InferenceResult`
//! carries MIR's inputs alongside the types; these tests pin each
//! table's entries on small bodies, keyed by source snippet. The
//! corpus-scale check is the differential MIR gate; these pin the
//! per-road recording semantics the gate builds on.

use baml_compiler2_hir_ty::{
    infer::{CallTypeArgPlan, MemberResolution, infer_body},
    render::Viewpoint,
};

use crate::engine::TestDbExt;

/// Every recorded member resolution in `source`, as sorted
/// `(snippet, kind)` pairs - the snippet is the recorded expression's
/// exact text, so assertions read as the language they pin.
fn member_resolutions(source: &str) -> Vec<(String, String)> {
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", source);
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
        MemberResolution::External(_) => "External",
        MemberResolution::ExternalField { .. } => "ExternalField",
        MemberResolution::ExternalVariant { .. } => "ExternalVariant",
        MemberResolution::ExternalInterfaceVirtualField { .. } => "ExternalInterfaceVirtualField",
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
    // An implements-block method is Impl-owned (never a class-inherent
    // method), so a concrete receiver resolves through the impl tier and
    // the record is InterfaceConcreteMethod carrying the impl block, the
    // resolved body, and the carried owner frame.
    assert!(
        resolutions.contains(&("d.describe".into(), "InterfaceConcreteMethod".into())),
        "concrete receiver resolves through the matched impl: {resolutions:?}"
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
    let file = db.file("test.baml", source);
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
                    let ty = segment.ty.render_with(&Viewpoint::canonical(&db));
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
    let file = db.file("test.baml", source);
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
                .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
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
fn static_method_call_uses_owner_then_function_generic_frame() {
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;

    let source = r#"
class RtBox<T> {
    value: T,
    function new(value: T) -> RtBox<T> throws never {
        RtBox<T> { value: value }
    }
}
function rt_owner_use() -> RtBox<int> throws never {
    RtBox<int>.new(1)
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", source);
    let mut seen = false;
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|diag| matches!(diag.error, TirTypeError::WrongTypeArgArity { .. })),
            "the receiver's written owner argument is part of the unbound frame: {:?}",
            result.diagnostics
        );
        for (&call, plan) in &result.call_plans {
            if &source[source_map.expr_span(call)] != "RtBox<int>.new(1)" {
                continue;
            }
            seen = true;
            assert_eq!(plan.own_offset, 0);
            assert!(matches!(
                plan.slots.as_slice(),
                [CallTypeArgPlan { ty, .. }]
                    if ty.render_with(&Viewpoint::canonical(&db)) == "int"
            ));
            assert_eq!(
                plan.type_args
                    .iter()
                    .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
                    .collect::<Vec<_>>(),
                vec!["int"]
            );
        }
    }
    assert!(seen, "static owner-generic call plan was not recorded");
}

#[test]
fn runtime_call_special_contracts_are_narrow_and_diagnostic() {
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;

    let source = r#"
function sc_id<T>(value: T) -> T throws never { value }
function sc_contract<F>() -> null throws never { null }

function sc_bare() -> int throws never {
    let runtime_t = reflect.Type.of<int>();
    sc_id<runtime_t>(1)
}
function sc_ordinary_contract() -> null throws never {
    sc_contract<(string) -> string>()
}
function sc_extract(pkg: reflect.Package) -> null {
    let extracted = pkg.get_function<(string) -> string>("root.Target");
    null
}
function sc_session(session: reflect.Session) -> null {
    let value = session.eval("1");
    null
}
function sc_sealed() -> reflect.class.Type throws never {
    reflect.class.Type {}
}
function sc_companion() -> baml.Map<string, int> throws never {
    baml.Map {}
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", source);
    let mut errors = Vec::new();
    let mut extraction_throws = None;
    let mut session_args = None;
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let source_map = baml_compiler2_ppir::body_source_map(&db, owner);
        let result = infer_body(&db, owner);
        errors.extend(result.diagnostics.iter().map(|diag| diag.error.clone()));
        let Some(source_map) = source_map else {
            continue;
        };
        for (&call, plan) in &result.call_plans {
            let snippet = &source[source_map.expr_span(call)];
            if snippet.starts_with("pkg.get_function") {
                extraction_throws = plan.slots.first().and_then(|slot| match &slot.ty {
                    baml_type::Ty::Function { throws, .. } => {
                        Some(throws.render_with(&Viewpoint::canonical(&db)))
                    }
                    _ => None,
                });
            }
            if snippet.starts_with("session.eval") {
                session_args = Some(
                    plan.type_args
                        .iter()
                        .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
                        .collect::<Vec<_>>(),
                );
            }
        }
    }

    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, TirTypeError::FunctionTypeMissingThrows))
            .count(),
        1,
        "only the ordinary function type reports missing throws: {errors:?}"
    );
    assert!(errors.iter().any(|error| matches!(
        error,
        TirTypeError::ComputedGenericArgumentRequiresUnreflect { name }
            if name.as_str() == "runtime_t"
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        TirTypeError::CannotConstructReflectionKind { class_name }
            if Viewpoint::canonical(&db).path(class_name) == "reflect.class.Type"
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        TirTypeError::CannotConstructBuiltinCompanion { class_name, companion }
            if Viewpoint::canonical(&db).path(class_name) == "baml.Map" && companion.builtin == "map"
    )));
    assert_eq!(extraction_throws.as_deref(), Some("unknown"));
    assert_eq!(session_args, Some(vec!["unknown".to_string()]));
}

// The inference tables behind a scoped `type T = …` binding, which BAML
// cannot observe: the call plan carries the binding's own rigid parameter
// (marked by the scoped index bit) while the name is in scope, and the
// shadowed static class again once the block closes. The observable
// semantics live in `baml_src/ns_scoped_type_bindings` and the
// `scoped_type_*` diagnostic projects.
#[test]
fn scoped_type_bindings_are_rigid_frame_parameters() {
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;

    let source = r#"
class ScopeT {}
function scope_id<X>(value: unknown) -> reflect.Type throws never { reflect.Type.of<X>() }
function scope_use(runtime_t: reflect.Type) -> reflect.Type throws never {
    let inner = {
        type ScopeT = unreflect(runtime_t)
        scope_id<ScopeT>(1)
    }
    scope_id<ScopeT>(3)
}
function scope_branch(runtime_t: reflect.Type, choose: bool) -> reflect.Type throws never {
    let branch_value = if choose {
        type ScopeT = unreflect(runtime_t)
        scope_id<ScopeT>(2)
    } else {
        scope_id<int>(0)
    }
    scope_id<ScopeT>(4)
}
function scope_bad() -> null throws never {
    type Bad = unreflect(42)
    null
}
function scope_shape_bad(runtime_t: reflect.Type) -> null throws never {
    type ShapeT = unreflect(runtime_t)
    let impossible: ShapeT[] = 42
    null
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", source);
    let mut saw_inner = false;
    let mut saw_outer = 0;
    let mut saw_branch = false;
    let mut saw_bad_operand = false;
    let mut saw_static_shape_error = false;
    let mut diagnostics = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(&db, file) {
        let Some(source_map) = baml_compiler2_ppir::body_source_map(&db, owner) else {
            continue;
        };
        let result = infer_body(&db, owner);
        diagnostics.extend(result.diagnostics.iter().map(|diag| diag.error.clone()));
        saw_bad_operand |= result.diagnostics.iter().any(|diag| {
            matches!(
                &diag.error,
                TirTypeError::TypeMismatch { expected, got }
                    if expected.render_with(&Viewpoint::canonical(&db)) == "reflect.Type | reflect.TypeView" && got.render_with(&Viewpoint::canonical(&db)) == "42"
            )
        });
        saw_static_shape_error |= result.diagnostics.iter().any(|diag| {
            matches!(
                &diag.error,
                TirTypeError::TypeMismatch { expected, got }
                    if expected.render_with(&Viewpoint::canonical(&db)) == "ShapeT[]"
                        && got.render_with(&Viewpoint::canonical(&db)) == "42"
            )
        });

        let binding = result
            .type_bindings
            .values()
            .find(|binding| binding.name.as_str() == "ScopeT");
        for (&call, plan) in &result.call_plans {
            let snippet = &source[source_map.expr_span(call)];
            if snippet == "scope_id<ScopeT>(1)" {
                let binding = binding.expect("ScopeT binding recorded");
                saw_inner = true;
                assert!(binding.parameter.index() & 0x8000_0000 != 0);
                assert!(matches!(
                    plan.type_args.as_slice(),
                    [ty] if matches!(ty, baml_type::Ty::TypeVar(param, _)
                        if param == &binding.parameter)
                ));
            }
            if snippet == "scope_id<ScopeT>(2)" {
                let binding = binding.expect("branch ScopeT binding recorded");
                saw_branch = true;
                assert!(matches!(
                    plan.type_args.as_slice(),
                    [ty] if matches!(ty, baml_type::Ty::TypeVar(param, _)
                        if param == &binding.parameter)
                ));
            }
            if snippet == "scope_id<ScopeT>(3)" || snippet == "scope_id<ScopeT>(4)" {
                saw_outer += 1;
                assert!(matches!(
                    plan.type_args.as_slice(),
                    [ty] if ty.render_with(&Viewpoint::canonical(&db)) == "user.ScopeT"
                ));
            }
        }
    }
    assert!(saw_inner && saw_branch);
    assert_eq!(
        saw_outer, 2,
        "the static class is restored at both closing braces"
    );
    assert!(
        saw_bad_operand,
        "type-binding operand must be checked below `type`"
    );
    assert!(
        saw_static_shape_error,
        "a literal is not a value of a rigid scoped type"
    );
    assert_eq!(
        diagnostics.len(),
        2,
        "valid scoped bindings must not create static-only errors: {diagnostics:?}"
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
    let file = db.file("test.baml", source);
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
                .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
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
    let file = db.file("test.baml", source);
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
                .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
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
function ir_probe() -> int {
    let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
    it.reduce((a: int, x: int) -> int { a + x }, 0)
}
"#;
    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", source);
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
                    .map(|ty| ty.render_with(&Viewpoint::canonical(&db)))
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
    let file = db.file("test.baml", source);
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
                renders.push(ty.render_with(&Viewpoint::canonical(&db)));
            }
        }
    }
    assert_eq!(
        renders,
        vec!["baml.json.json[]".to_string()],
        "the ascribed binding records the written nominal type"
    );
}
