//! Differential tests: every generated adapter against the VM's `Value`
//! glue, on receivers of each slot kind. Both sides start from identical
//! receivers; results, errors and the receiver afterwards must match.

use bex_heap::TlabHolder;
use bex_vm_types::{
    Object, RealizedTy, Value, ValueKind,
    errors::{VmInternalError, VmRustFnError},
    layout::{Fault, Kind, NeedsVm, ObjPtr, RawKind, SlotKind, Slots, Strings, Tagged},
    types::ObjectType,
};
use indexmap::IndexMap;

use super::generated as adapters;
use crate::{
    BexVm,
    package_baml::value_model::{ValueModel, obj_ptr, ptr_value, vm_error},
    package_baml::{
        BamlClassArray, BamlClassMap, BamlClassOpsEquals_for_string, BamlClassString,
        NativeCallResult, PackageBamlImpl,
    },
    vm::tests::test_vm,
};
use bex_vm_types::types::SharedKind;

// ── Fixtures ─────────────────────────────────────────────────────────────

/// A literal fixture value.
#[derive(Clone, Copy, Debug)]
enum Lit {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(&'static str),
}

/// A declared element type, one per slot kind and raw flavour.
#[derive(Clone, Copy, Debug)]
enum Ty {
    Int,
    Float,
    Bool,
    Str,
    OptInt,
    Unknown,
}

impl Ty {
    fn realized(self) -> RealizedTy {
        match self {
            Ty::Int => RealizedTy::int(),
            Ty::Float => RealizedTy::Float,
            Ty::Bool => RealizedTy::Bool,
            Ty::Str => RealizedTy::string(),
            Ty::OptInt => RealizedTy::optional(RealizedTy::int()),
            Ty::Unknown => RealizedTy::unknown(),
        }
    }
}

/// A container fixture: element (or map value) type, contents, and a value
/// that fits its slot.
struct Fixture {
    ty: Ty,
    elems: &'static [Lit],
    sample: Lit,
}

const LISTS: &[Fixture] = &[
    Fixture {
        ty: Ty::Int,
        elems: &[Lit::Int(3), Lit::Int(-1), Lit::Int(2)],
        sample: Lit::Int(7),
    },
    Fixture {
        ty: Ty::Int,
        elems: &[],
        sample: Lit::Int(7),
    },
    Fixture {
        ty: Ty::Float,
        elems: &[Lit::Float(1.5), Lit::Float(-0.0), Lit::Float(2.25)],
        sample: Lit::Float(0.5),
    },
    Fixture {
        ty: Ty::Bool,
        elems: &[Lit::Bool(true), Lit::Bool(false)],
        sample: Lit::Bool(true),
    },
    Fixture {
        ty: Ty::Str,
        elems: &[Lit::Str("b"), Lit::Str("a"), Lit::Str("c")],
        sample: Lit::Str("z"),
    },
    Fixture {
        ty: Ty::OptInt,
        elems: &[Lit::Int(1), Lit::Null, Lit::Int(3)],
        sample: Lit::Null,
    },
    Fixture {
        ty: Ty::Unknown,
        elems: &[
            Lit::Str("x"),
            Lit::Int(2),
            Lit::Null,
            Lit::Float(1.5),
            Lit::Bool(false),
        ],
        sample: Lit::Str("y"),
    },
];

/// Map fixtures; keys are `"a"`, `"b"`, ... in order.
const MAPS: &[Fixture] = &[
    Fixture {
        ty: Ty::Int,
        elems: &[Lit::Int(1), Lit::Int(2)],
        sample: Lit::Int(9),
    },
    Fixture {
        ty: Ty::Float,
        elems: &[Lit::Float(0.25)],
        sample: Lit::Float(4.0),
    },
    Fixture {
        ty: Ty::Str,
        elems: &[Lit::Str("x"), Lit::Str("y")],
        sample: Lit::Str("w"),
    },
    Fixture {
        ty: Ty::OptInt,
        elems: &[Lit::Int(1), Lit::Null],
        sample: Lit::Null,
    },
    Fixture {
        ty: Ty::Int,
        elems: &[],
        sample: Lit::Int(9),
    },
];

const MAP_KEYS: [&str; 3] = ["a", "b", "c"];

/// String receivers.
const STRINGS: &[&str] = &["", "hello world", "a\nb\r\nc\n", "héllo→✓", "a,b,,c"];

fn lit(vm: &mut BexVm, value: Lit) -> Value {
    match value {
        Lit::Null => Value::NULL,
        Lit::Int(int) => Value::int(int),
        Lit::Float(float) => Value::object(vm.alloc_float(float)),
        Lit::Bool(boolean) => Value::bool(boolean),
        Lit::Str(text) => Value::object(vm.alloc_string(text.to_string())),
    }
}

fn list(vm: &mut BexVm, fixture: &Fixture) -> Value {
    let values = fixture.elems.iter().map(|&elem| lit(vm, elem)).collect();
    Value::object(vm.alloc_array(fixture.ty.realized(), values))
}

fn map(vm: &mut BexVm, fixture: &Fixture) -> Value {
    let entries: IndexMap<bex_vm_types::BexStr, Value> = MAP_KEYS
        .iter()
        .zip(fixture.elems)
        .map(|(&key, &value)| (key.into(), lit(vm, value)))
        .collect();
    Value::object(vm.alloc_map(RealizedTy::string(), fixture.ty.realized(), entries))
}

// ── Comparison ───────────────────────────────────────────────────────────

/// Structural rendering: equal dumps mean equal BAML values.
fn dump(vm: &BexVm, value: Value) -> String {
    let ValueKind::Object(ptr) = value.kind() else {
        return format!("{:?}", value.kind());
    };
    let join = |values: Vec<Value>| -> String {
        let parts: Vec<String> = values.into_iter().map(|v| dump(vm, v)).collect();
        parts.join(", ")
    };
    match vm.get_object(ptr) {
        Object::Float(float) => format!("{float:?}f"),
        Object::Shared(SharedKind::Str) => format!("{:?}", vm.expect_str(ptr)),
        Object::Shared(SharedKind::List) => {
            let array = vm.expect_list(ptr);
            format!(
                "[{}]",
                join(vm.list_values(array, bex_vm_types::object_format::Access::Shared))
            )
        }
        Object::Uint8Array(bytes) => format!("bytes{:?}", bytes.to_vec()),
        Object::Shared(SharedKind::Map) => {
            let entries: Vec<String> = vm
                .map_snapshot(
                    vm.expect_map(ptr),
                    bex_vm_types::object_format::Access::Shared,
                )
                .snapshot_entries()
                .into_iter()
                .map(|(key, value)| format!("{}: {}", dump(vm, key), dump(vm, value)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        other => ObjectType::of(other).to_string(),
    }
}

/// What a call produced, comparable across the two paths.
#[derive(Debug, PartialEq)]
enum Outcome {
    Value(String),
    Error(String),
    /// The receiver was not the declared kind (`TypeError` on the VM).
    WrongReceiver,
}

fn vm_outcome(vm: &BexVm, result: NativeCallResult) -> Outcome {
    match result {
        NativeCallResult::Done(value) => Outcome::Value(dump(vm, value)),
        NativeCallResult::Error(VmRustFnError::InternalError(VmInternalError::TypeError {
            ..
        })) => Outcome::WrongReceiver,
        NativeCallResult::Error(error) => Outcome::Error(format!("{error:?}")),
        NativeCallResult::YieldToCall { .. } => panic!("unexpected yield"),
    }
}

fn adapter_outcome(vm: &BexVm, result: Result<Value, Fault>) -> Outcome {
    match result {
        Ok(value) => Outcome::Value(dump(vm, value)),
        Err(Fault::WrongReceiver { .. }) => Outcome::WrongReceiver,
        Err(fault) => {
            let error = vm_error(&fault).unwrap_or_else(|| panic!("no VM error for {fault:?}"));
            Outcome::Error(format!("{error:?}"))
        }
    }
}

// ── Cases ────────────────────────────────────────────────────────────────

type Glue = fn(&mut BexVm, &[Value]) -> NativeCallResult;
type Adapter = fn(&mut ValueModel<'_>, &[Value]) -> Result<Value, Fault>;

/// An argument after the receiver.
#[derive(Clone, Copy, Debug)]
enum Arg {
    Lit(Lit),
    /// The fixture's sample element.
    Sample,
    /// Another list built from the same fixture.
    SameList,
}

/// Which argument lists a case runs with.
#[derive(Clone, Copy)]
enum Args {
    None,
    Index,
    Item,
    ItemAt,
    Bounds,
    OtherList,
    Separator,
    Key,
    KeyItem,
    Delimiter,
    OtherString,
}

const INDICES: [i64; 6] = [0, 1, -1, 2, 5, -9];
const BOUNDS: [(i64, i64); 6] = [(0, 2), (1, -1), (-2, 9), (2, 1), (-9, 1), (0, 0)];

fn arg_lists(args: Args) -> Vec<Vec<Arg>> {
    let int = |i| Arg::Lit(Lit::Int(i));
    let str = |s| Arg::Lit(Lit::Str(s));
    match args {
        Args::None => vec![vec![]],
        Args::Index => INDICES.iter().map(|&i| vec![int(i)]).collect(),
        Args::Item => vec![vec![Arg::Sample]],
        Args::ItemAt => INDICES.iter().map(|&i| vec![Arg::Sample, int(i)]).collect(),
        Args::Bounds => BOUNDS.iter().map(|&(s, e)| vec![int(s), int(e)]).collect(),
        Args::OtherList => vec![vec![Arg::SameList]],
        Args::Separator => vec![vec![str(", ")], vec![str("")]],
        Args::Key => ["a", "b", "zz"].iter().map(|&k| vec![str(k)]).collect(),
        Args::KeyItem => ["a", "new"]
            .iter()
            .map(|&k| vec![str(k), Arg::Sample])
            .collect(),
        Args::Delimiter => [",", "", "l", "zz"].iter().map(|&d| vec![str(d)]).collect(),
        Args::OtherString => ["hello world", "x"].iter().map(|&s| vec![str(s)]).collect(),
    }
}

struct Case {
    path: &'static str,
    glue: Glue,
    adapter: Adapter,
    args: Args,
}

fn ptr(value: Value) -> ObjPtr {
    obj_ptr(value)
}

fn int(value: Value) -> i64 {
    value.as_int().expect("an int argument")
}

fn opt(m: &mut ValueModel<'_>, value: Option<Tagged>) -> Value {
    m.value(value.unwrap_or(Tagged::Null))
}

const LIST_CASES: &[Case] = &[
    Case {
        path: "baml.Array.length",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_length,
        adapter: |m, a| adapters::baml_array_length(m, ptr(a[0])).map(Value::int),
        args: Args::None,
    },
    Case {
        path: "baml.Array.at",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_at,
        adapter: |m, a| {
            let found = adapters::baml_array_at(m, ptr(a[0]), int(a[1]))?;
            Ok(opt(m, found))
        },
        args: Args::Index,
    },
    Case {
        path: "baml.Array.push",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_push,
        adapter: |m, a| {
            let item = m.tagged(a[1]);
            adapters::baml_array_push(m, ptr(a[0]), item).map(Value::int)
        },
        args: Args::Item,
    },
    Case {
        path: "baml.Array.pop",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_pop,
        adapter: |m, a| {
            let popped = adapters::baml_array_pop(m, ptr(a[0]))?;
            Ok(opt(m, popped))
        },
        args: Args::None,
    },
    Case {
        path: "baml.Array.remove_at",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_remove_at,
        adapter: |m, a| {
            let removed = adapters::baml_array_remove_at(m, ptr(a[0]), int(a[1]))?;
            Ok(opt(m, removed))
        },
        args: Args::Index,
    },
    Case {
        path: "baml.Array.concat",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_concat,
        adapter: |m, a| adapters::baml_array_concat(m, ptr(a[0]), ptr(a[1])).map(ptr_value),
        args: Args::OtherList,
    },
    Case {
        path: "baml.Array.reverse",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_reverse,
        adapter: |m, a| adapters::baml_array_reverse(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.Array.slice",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_slice,
        adapter: |m, a| {
            adapters::baml_array_slice(m, ptr(a[0]), int(a[1]), int(a[2])).map(ptr_value)
        },
        args: Args::Bounds,
    },
    Case {
        path: "baml.Array.clear",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_clear,
        adapter: |m, a| adapters::baml_array_clear(m, ptr(a[0])).map(|()| Value::NULL),
        args: Args::None,
    },
    Case {
        path: "baml.Array.shift",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_shift,
        adapter: |m, a| {
            let shifted = adapters::baml_array_shift(m, ptr(a[0]))?;
            Ok(opt(m, shifted))
        },
        args: Args::None,
    },
    Case {
        path: "baml.Array.unshift",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_unshift,
        adapter: |m, a| {
            let item = m.tagged(a[1]);
            adapters::baml_array_unshift(m, ptr(a[0]), item).map(Value::int)
        },
        args: Args::Item,
    },
    Case {
        path: "baml.Array.insert",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_insert,
        adapter: |m, a| {
            let item = m.tagged(a[1]);
            adapters::baml_array_insert(m, ptr(a[0]), item, int(a[2])).map(Value::int)
        },
        args: Args::ItemAt,
    },
    Case {
        path: "baml.Array.join",
        glue: <PackageBamlImpl as BamlClassArray>::__glue_join,
        adapter: |m, a| adapters::baml_array_join(m, ptr(a[0]), ptr(a[1])).map(ptr_value),
        args: Args::Separator,
    },
];

const MAP_CASES: &[Case] = &[
    Case {
        path: "baml.Map.length",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_length,
        adapter: |m, a| adapters::baml_map_length(m, ptr(a[0])).map(Value::int),
        args: Args::None,
    },
    Case {
        path: "baml.Map.has",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_has,
        adapter: |m, a| {
            let key = m.tagged(a[1]);
            adapters::baml_map_has(m, ptr(a[0]), key).map(Value::bool)
        },
        args: Args::Key,
    },
    Case {
        path: "baml.Map.get",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_get,
        adapter: |m, a| {
            let key = m.tagged(a[1]);
            let found = adapters::baml_map_get(m, ptr(a[0]), key)?;
            Ok(opt(m, found))
        },
        args: Args::Key,
    },
    Case {
        path: "baml.Map.index",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_index,
        adapter: |m, a| {
            let key = m.tagged(a[1]);
            let found = adapters::baml_map_index(m, ptr(a[0]), key)?;
            Ok(m.value(found))
        },
        args: Args::Key,
    },
    Case {
        path: "baml.Map.set",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_set,
        adapter: |m, a| {
            let (key, value) = (m.tagged(a[1]), m.tagged(a[2]));
            let previous = adapters::baml_map_set(m, ptr(a[0]), key, value)?;
            Ok(opt(m, previous))
        },
        args: Args::KeyItem,
    },
    Case {
        path: "baml.Map.delete",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_delete,
        adapter: |m, a| {
            let key = m.tagged(a[1]);
            let removed = adapters::baml_map_delete(m, ptr(a[0]), key)?;
            Ok(opt(m, removed))
        },
        args: Args::Key,
    },
    Case {
        path: "baml.Map.get_or_insert",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_get_or_insert,
        adapter: |m, a| {
            let (key, value) = (m.tagged(a[1]), m.tagged(a[2]));
            let found = adapters::baml_map_get_or_insert(m, ptr(a[0]), key, value)?;
            Ok(m.value(found))
        },
        args: Args::KeyItem,
    },
    Case {
        path: "baml.Map.clear",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_clear,
        adapter: |m, a| adapters::baml_map_clear(m, ptr(a[0])).map(|()| Value::NULL),
        args: Args::None,
    },
    Case {
        path: "baml.Map.keys",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_keys,
        adapter: |m, a| adapters::baml_map_keys(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.Map.values",
        glue: <PackageBamlImpl as BamlClassMap>::__glue_values,
        adapter: |m, a| adapters::baml_map_values(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
];

const STRING_CASES: &[Case] = &[
    Case {
        path: "baml.String.char_count",
        glue: <PackageBamlImpl as BamlClassString>::__glue_char_count,
        adapter: |m, a| adapters::baml_string_char_count(m, ptr(a[0])).map(Value::int),
        args: Args::None,
    },
    Case {
        path: "baml.String.split",
        glue: <PackageBamlImpl as BamlClassString>::__glue_split,
        adapter: |m, a| adapters::baml_string_split(m, ptr(a[0]), ptr(a[1])).map(ptr_value),
        args: Args::Delimiter,
    },
    Case {
        path: "baml.String.chars",
        glue: <PackageBamlImpl as BamlClassString>::__glue_chars,
        adapter: |m, a| adapters::baml_string_chars(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.String.lines",
        glue: <PackageBamlImpl as BamlClassString>::__glue_lines,
        adapter: |m, a| adapters::baml_string_lines(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.String.slice",
        glue: <PackageBamlImpl as BamlClassString>::__glue_slice,
        adapter: |m, a| {
            adapters::baml_string_slice(m, ptr(a[0]), int(a[1]), int(a[2])).map(ptr_value)
        },
        args: Args::Bounds,
    },
    Case {
        path: "baml.String.to_utf8",
        glue: <PackageBamlImpl as BamlClassString>::__glue_to_utf8,
        adapter: |m, a| adapters::baml_string_to_utf8(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.String.to_code_points",
        glue: <PackageBamlImpl as BamlClassString>::__glue_to_code_points,
        adapter: |m, a| adapters::baml_string_to_code_points(m, ptr(a[0])).map(ptr_value),
        args: Args::None,
    },
    Case {
        path: "baml.ops.Equals$for$string.eq",
        glue: <PackageBamlImpl as BamlClassOpsEquals_for_string>::__glue_eq,
        adapter: |m, a| {
            adapters::baml_ops_equals_for_string_eq(m, ptr(a[0]), ptr(a[1])).map(Value::bool)
        },
        args: Args::OtherString,
    },
];

// ── Runner ───────────────────────────────────────────────────────────────

type Build = Box<dyn Fn(&mut BexVm) -> Value>;

/// A named way to build a fresh receiver, and an element that fits it.
struct Receiver {
    name: String,
    build: Build,
    sample: Lit,
}

fn build_args(vm: &mut BexVm, receiver: &Receiver, args: &[Arg]) -> Vec<Value> {
    let first = (receiver.build)(vm);
    let rest = args.iter().map(|&arg| match arg {
        Arg::Lit(value) => lit(vm, value),
        Arg::Sample => lit(vm, receiver.sample),
        Arg::SameList => (receiver.build)(vm),
    });
    std::iter::once(first)
        .chain(rest.collect::<Vec<_>>())
        .collect()
}

/// Run `case` through the VM's `Value` glue: its outcome and the receiver
/// afterwards.
fn run_vm(vm: &mut BexVm, case: &Case, receiver: &Receiver, args: &[Arg]) -> (Outcome, String) {
    let vm_args = build_args(vm, receiver, args);
    let vm_result = (case.glue)(vm, &vm_args);
    (vm_outcome(vm, vm_result), dump(vm, vm_args[0]))
}

/// Run `case` both ways from identical receivers; return the outcome and
/// the receiver afterwards for each side.
fn run(case: &Case, receiver: &Receiver, args: &[Arg]) -> [(Outcome, String); 2] {
    let mut vm = test_vm(Vec::new());
    let vm_side = run_vm(&mut vm, case, receiver, args);
    let adapter_args = build_args(&mut vm, receiver, args);
    let adapter_result = (case.adapter)(&mut ValueModel::compiled(&mut vm), &adapter_args);
    let adapter_side = (
        adapter_outcome(&vm, adapter_result),
        dump(&vm, adapter_args[0]),
    );
    [vm_side, adapter_side]
}

fn check_all(cases: &[Case], receivers: &[Receiver]) -> usize {
    let mut runs = 0;
    for case in cases {
        for receiver in receivers {
            for args in arg_lists(case.args) {
                let [vm_side, adapter_side] = run(case, receiver, &args);
                let name = &receiver.name;
                assert_eq!(
                    adapter_side, vm_side,
                    "{} on {name} with {args:?}",
                    case.path
                );
                runs += 1;
            }
        }
    }
    runs
}

fn fixture_receivers(
    fixtures: &'static [Fixture],
    build: fn(&mut BexVm, &Fixture) -> Value,
) -> Vec<Receiver> {
    fixtures
        .iter()
        .map(|fixture| Receiver {
            name: format!("{:?}{:?}", fixture.ty, fixture.elems),
            build: Box::new(move |vm| build(vm, fixture)),
            sample: fixture.sample,
        })
        .collect()
}

#[test]
fn list_adapters_match_the_vm_on_every_slot_kind() {
    let runs = check_all(LIST_CASES, &fixture_receivers(LISTS, list));
    assert!(runs > 100, "{runs}");
}

#[test]
fn map_adapters_match_the_vm_on_every_value_slot_kind() {
    let runs = check_all(MAP_CASES, &fixture_receivers(MAPS, map));
    assert!(runs > 50, "{runs}");
}

#[test]
fn string_adapters_match_the_vm() {
    let receivers: Vec<Receiver> = STRINGS
        .iter()
        .map(|&text| Receiver {
            name: format!("{text:?}"),
            build: Box::new(move |vm| lit(vm, Lit::Str(text))),
            sample: Lit::Null,
        })
        .collect();
    let runs = check_all(STRING_CASES, &receivers);
    assert!(runs > 40, "{runs}");
}

/// The differential cases above cover every generated adapter.
#[test]
fn every_generated_adapter_has_a_differential_case() {
    let mut tested: Vec<&str> = [LIST_CASES, MAP_CASES, STRING_CASES]
        .iter()
        .flat_map(|cases| cases.iter().map(|case| case.path))
        .collect();
    tested.sort_unstable();
    let mut generated = adapters::ADAPTER_PATHS.to_vec();
    generated.sort_unstable();
    assert_eq!(tested, generated);
}

// ── Array.map: the VM's continuation, answered by a Rust callee ──────────

/// A callee: scale numbers, negate bools, suffix strings.
fn callee(vm: &mut BexVm, value: Value) -> Value {
    let tagged = ValueModel::new(vm).tagged(value);
    let result = match tagged {
        Tagged::Null => Tagged::Int(0),
        Tagged::Int(int) => Tagged::Int(int * 10),
        Tagged::Float(float) => Tagged::Float(float * 2.0),
        Tagged::Bool(boolean) => Tagged::Bool(!boolean),
        Tagged::Ptr(_) => {
            let text = format!("{}!", dump(vm, value));
            return Value::object(vm.alloc_string(text));
        }
    };
    ValueModel::new(vm).value(result)
}

/// Run the VM's `Array.map`, answering each yield with [`callee`].
fn vm_map(vm: &mut BexVm, array: Value) -> Value {
    let f = Value::object(vm.alloc_string("callee".to_string()));
    let mut result = <PackageBamlImpl as BamlClassArray>::__glue_map(vm, &[array, f]);
    loop {
        match result {
            NativeCallResult::Done(value) => return value,
            NativeCallResult::YieldToCall {
                args, continuation, ..
            } => {
                let answer = callee(vm, args[0]);
                result = continuation.call(vm, answer);
            }
            NativeCallResult::Error(error) => panic!("map failed: {error:?}"),
        }
    }
}

// ── Specialized entry points, called directly by slot kind ───────────────

#[test]
fn specialized_list_adapters_take_the_slot_representation() {
    let mut vm = test_vm(Vec::new());
    let ints = ptr(list(&mut vm, &LISTS[0]));
    let strings = ptr(list(&mut vm, &LISTS[4]));
    let optionals = ptr(list(&mut vm, &LISTS[5]));
    let z = obj_ptr(lit(&mut vm, Lit::Str("z")));
    let mut m = ValueModel::compiled(&mut vm);

    let seven = bex_vm_types::layout::RawWord::from_int(7);
    assert_eq!(adapters::baml_array_push_raw8(&mut m, ints, seven), Ok(4));
    assert_eq!(
        adapters::baml_array_at_raw8(&mut m, ints, -1),
        Ok(Some(seven))
    );
    assert_eq!(adapters::baml_array_length_raw8(&mut m, ints), Ok(4));
    assert_eq!(adapters::baml_array_pop_raw8(&mut m, ints), Ok(Some(seven)));

    assert_eq!(adapters::baml_array_push_ptr(&mut m, strings, z), Ok(4));
    assert_eq!(adapters::baml_array_at_ptr(&mut m, strings, 3), Ok(Some(z)));
    assert_eq!(adapters::baml_array_length_ptr(&mut m, strings), Ok(4));
    assert_eq!(adapters::baml_array_pop_ptr(&mut m, strings), Ok(Some(z)));

    assert_eq!(
        adapters::baml_array_push_tagged16(&mut m, optionals, Tagged::Null),
        Ok(4)
    );
    assert_eq!(
        adapters::baml_array_at_tagged16(&mut m, optionals, 0),
        Ok(Some(Tagged::Int(1)))
    );
    assert_eq!(
        adapters::baml_array_length_tagged16(&mut m, optionals),
        Ok(4)
    );
    assert_eq!(
        adapters::baml_array_pop_tagged16(&mut m, optionals),
        Ok(Some(Tagged::Null))
    );
}

#[test]
fn specialized_map_adapters_take_the_slot_representation() {
    let mut vm = test_vm(Vec::new());
    let ints = ptr(map(&mut vm, &MAPS[0]));
    let strings = ptr(map(&mut vm, &MAPS[2]));
    let optionals = ptr(map(&mut vm, &MAPS[3]));
    let key = obj_ptr(lit(&mut vm, Lit::Str("a")));
    let w = obj_ptr(lit(&mut vm, Lit::Str("w")));
    let mut m = ValueModel::compiled(&mut vm);
    let one = bex_vm_types::layout::RawWord::from_int(1);
    let nine = bex_vm_types::layout::RawWord::from_int(9);

    assert_eq!(
        adapters::baml_map_get_raw8(&mut m, ints, key),
        Ok(Some(one))
    );
    assert_eq!(
        adapters::baml_map_set_raw8(&mut m, ints, key, nine),
        Ok(Some(one))
    );
    assert_eq!(adapters::baml_map_has_raw8(&mut m, ints, key), Ok(true));
    assert_eq!(adapters::baml_map_length_raw8(&mut m, ints), Ok(2));

    let previous = adapters::baml_map_set_ptr(&mut m, strings, key, w).unwrap();
    assert_eq!(
        previous.map(|p| m.str_text(p).to_string()),
        Some("x".to_string())
    );
    assert_eq!(
        adapters::baml_map_get_ptr(&mut m, strings, key),
        Ok(Some(w))
    );
    assert_eq!(adapters::baml_map_length_ptr(&mut m, strings), Ok(2));

    assert_eq!(
        adapters::baml_map_get_tagged16(&mut m, optionals, key),
        Ok(Some(Tagged::Int(1)))
    );
    assert_eq!(
        adapters::baml_map_set_tagged16(&mut m, optionals, key, Tagged::Null),
        Ok(Some(Tagged::Int(1)))
    );
    assert_eq!(
        adapters::baml_map_has_tagged16(&mut m, optionals, key),
        Ok(true)
    );
    assert_eq!(adapters::baml_map_length_tagged16(&mut m, optionals), Ok(2));
}

/// A string key's hash is computed on its first map use and read from the
/// cell after: ten thousand distinct keys, each set once and looked up
/// twice, and every cell holds its map hash after the first use.
#[test]
fn string_keys_cache_their_map_hash_in_the_cell() {
    const KEYS: usize = 10_000;
    let mut vm = test_vm(Vec::new());
    let map = ptr(Value::object(vm.alloc_map(
        RealizedTy::string(),
        RealizedTy::int(),
        IndexMap::new(),
    )));
    let keys: Vec<ObjPtr> = (0..KEYS)
        .map(|i| ptr(Value::object(vm.alloc_string(format!("key-{i}-ключ")))))
        .collect();
    fn cell(vm: &BexVm, key: ObjPtr) -> bex_vm_types::StrRef<'_> {
        vm.expect_str(ptr_value(key).as_object_ptr().unwrap())
    }
    for &key in &keys {
        assert_eq!(
            cell(&vm, key).cached_hash(),
            None,
            "a fresh cell holds no hash"
        );
    }

    let mut m = ValueModel::compiled(&mut vm);
    for (i, &key) in keys.iter().enumerate() {
        let value = bex_vm_types::layout::RawWord::from_int(i as i64);
        assert_eq!(
            adapters::baml_map_set_raw8(&mut m, map, key, value),
            Ok(None)
        );
    }
    assert_eq!(adapters::baml_map_length_raw8(&mut m, map), Ok(KEYS as i64));
    drop(m);

    let stored: Vec<u64> = vm
        .map_snapshot(
            vm.map_at(ptr_value(map).as_object_ptr().unwrap()).unwrap(),
            bex_vm_types::object_format::Access::Shared,
        )
        .hashes()
        .collect();
    for (i, &key) in keys.iter().enumerate() {
        let string = cell(&vm, key);
        let cached = string
            .cached_hash()
            .expect("the first use left the hash in the cell");
        assert_ne!(cached, 0);
        assert_eq!(
            cached,
            bex_vm_types::map_string_hash(&string.text()).max(1),
            "the cached hash is the map's string hash"
        );
        assert_eq!(stored[i], cached, "the entry stores the cell's hash");
    }

    let mut m = ValueModel::compiled(&mut vm);
    for (i, &key) in keys.iter().enumerate() {
        let expected = Ok(Some(bex_vm_types::layout::RawWord::from_int(i as i64)));
        assert_eq!(adapters::baml_map_get_raw8(&mut m, map, key), expected);
        assert_eq!(adapters::baml_map_get_raw8(&mut m, map, key), expected);
        assert_eq!(adapters::baml_map_has_raw8(&mut m, map, key), Ok(true));
    }
    // A different cell with the same text finds the entry too.
    let twin = m.str_alloc("key-4321-ключ");
    assert_eq!(
        adapters::baml_map_get_raw8(&mut m, map, twin),
        Ok(Some(bex_vm_types::layout::RawWord::from_int(4321)))
    );
    let missing = m.str_alloc("key-4321-ключ!");
    assert_eq!(adapters::baml_map_get_raw8(&mut m, map, missing), Ok(None));
    drop(m);
    assert!(cell(&vm, twin).cached_hash().is_some());
    assert!(cell(&vm, missing).cached_hash().is_some());
}

#[test]
fn specialized_string_adapters_take_pointers() {
    let mut vm = test_vm(Vec::new());
    let hello = obj_ptr(lit(&mut vm, Lit::Str("héllo")));
    let other = obj_ptr(lit(&mut vm, Lit::Str("héllo")));
    let mut m = ValueModel::compiled(&mut vm);
    assert_eq!(adapters::baml_string_char_count_ptr(&mut m, hello), Ok(5));
    assert_eq!(
        adapters::baml_ops_equals_for_string_eq_ptr(&mut m, hello, other),
        Ok(true)
    );
    let sliced = adapters::baml_string_slice_ptr(&mut m, hello, 1, 3).unwrap();
    assert_eq!(dump(&vm, ptr_value(sliced)), "\"él\"");
}

// ── Deferrals and receiver checks ────────────────────────────────────────

#[test]
fn a_non_string_map_key_defers_to_the_vm() {
    let mut vm = test_vm(Vec::new());
    let ints = ptr(map(&mut vm, &MAPS[0]));
    let mut m = ValueModel::compiled(&mut vm);
    assert_eq!(
        adapters::baml_map_get(&mut m, ints, Tagged::Int(1)),
        Err(Fault::NeedsVm(NeedsVm::KeyHash))
    );
}

#[test]
fn joining_a_non_string_object_defers_to_the_vm() {
    let mut vm = test_vm(Vec::new());
    let inner = list(&mut vm, &LISTS[0]);
    let nested = Value::object(vm.alloc_array(RealizedTy::unknown(), vec![inner]));
    let separator = obj_ptr(lit(&mut vm, Lit::Str(",")));
    let mut m = ValueModel::compiled(&mut vm);
    assert_eq!(
        adapters::baml_array_join(&mut m, ptr(nested), separator),
        Err(Fault::NeedsVm(NeedsVm::Render))
    );
}

#[test]
fn a_receiver_of_another_kind_is_rejected_like_the_vm() {
    let mut vm = test_vm(Vec::new());
    let text = lit(&mut vm, Lit::Str("not a list"));
    let vm_result = <PackageBamlImpl as BamlClassArray>::__glue_length(&mut vm, &[text]);
    let vm_side = vm_outcome(&vm, vm_result);
    let mut m = ValueModel::compiled(&mut vm);
    let fault = adapters::baml_array_length(&mut m, ptr(text)).unwrap_err();
    assert_eq!(
        fault,
        Fault::WrongReceiver {
            expected: Kind::List,
            found: Kind::Str
        }
    );
    assert_eq!(vm_side, Outcome::WrongReceiver);
}

#[test]
fn a_generic_argument_must_fit_the_receiver_slot() {
    let mut vm = test_vm(Vec::new());
    let ints = ptr(list(&mut vm, &LISTS[0]));
    let mut m = ValueModel::compiled(&mut vm);
    let slot = SlotKind::Raw(RawKind::Int);
    assert_eq!(
        adapters::baml_array_push(&mut m, ints, Tagged::Bool(true)),
        Err(Fault::SlotMismatch {
            slot,
            value: Tagged::Bool(true)
        })
    );
    assert!(Slots::<ObjPtr>::untag(&mut (), Tagged::Int(1), SlotKind::Ptr).is_err());
}

// ── Interpreter outputs pinned across the move to shared bodies ──────────

/// What the interpreter's glue produced for every case above, recorded from
/// the `Value` bodies these builtins had before they became generic.
const GOLDEN: &str = include_str!("vm_glue_golden.txt");

fn string_receivers() -> Vec<Receiver> {
    STRINGS
        .iter()
        .map(|&text| Receiver {
            name: format!("{text:?}"),
            build: Box::new(move |vm| lit(vm, Lit::Str(text))),
            sample: Lit::Null,
        })
        .collect()
}

/// One recorded line: the glue's outcome and the receiver afterwards.
fn record_case(case: &Case, receiver: &Receiver, args: &[Arg]) -> String {
    let mut vm = test_vm(Vec::new());
    let (outcome, after) = run_vm(&mut vm, case, receiver, args);
    let name = &receiver.name;
    format!("{} {name} {args:?} => {outcome:?} | {after}", case.path)
}

/// `Array.map`'s continuation over one fixture.
fn record_map(fixture: &Fixture) -> String {
    let mut vm = test_vm(Vec::new());
    let array = list(&mut vm, fixture);
    let mapped = vm_map(&mut vm, array);
    format!(
        "baml.Array.map {:?} => {}",
        fixture.elems,
        dump(&vm, mapped)
    )
}

fn record_vm_glue() -> String {
    let suites = [
        (LIST_CASES, fixture_receivers(LISTS, list)),
        (MAP_CASES, fixture_receivers(MAPS, map)),
        (STRING_CASES, string_receivers()),
    ];
    let runs = suites.iter().flat_map(|(cases, receivers)| {
        cases.iter().flat_map(move |case| {
            receivers.iter().flat_map(move |receiver| {
                arg_lists(case.args)
                    .into_iter()
                    .map(move |args| record_case(case, receiver, &args))
            })
        })
    });
    let lines: Vec<String> = runs.chain(LISTS.iter().map(record_map)).collect();
    lines.join("\n") + "\n"
}

#[test]
fn vm_glue_outputs_are_unchanged() {
    let actual = record_vm_glue();
    if baml_env::os_var("BLESS_VM_GLUE").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/adapters/vm_glue_golden.txt"
        );
        std::fs::write(path, &actual).unwrap();
        return;
    }
    assert_eq!(actual, GOLDEN);
}
