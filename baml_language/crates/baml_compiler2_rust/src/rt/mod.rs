//! Closed-world typed emission. Unsupported MIR is a pack error, never bytecode.
use crate::{
    index::ProgramIndex,
    structure::{Cfg, Flow, Stmt, structurize},
};
use baml_base::SourceRoot;
use baml_compiler2_hir::{
    item_data::{file_functions, file_lets, is_required_interface_method},
    package::world_files,
};
use baml_compiler2_mir::{
    AggregateKind, BinOp, Constant, Db, MirFunctionBody, MirFunctionKind, Operand, OptLevel, Place,
    RuntimeTy, Rvalue, StatementKind, Terminator, UnaryOp, lower_function,
};
use bex_vm_types::{Object, Program};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

type TestMir<'db> = std::rc::Rc<MirFunction<'db>>;

type Gen<T> = Result<T, String>;
#[derive(Clone)]
struct MirFunction<'db> {
    identity: Option<baml_compiler2_mir::MirFunctionId<'db>>,
    arity: usize,
    kind: MirFunctionKind<'db>,
    lambdas: Vec<MirFunction<'db>>,
}
impl<'db> From<&baml_compiler2_mir::MirFunction<'db>> for MirFunction<'db> {
    fn from(mir: &baml_compiler2_mir::MirFunction<'db>) -> Self {
        Self {
            identity: Some(mir.identity.clone()),
            arity: mir.arity,
            kind: mir.kind.clone(),
            lambdas: mir.lambdas.iter().map(Self::from).collect(),
        }
    }
}
struct Layout {
    fields: Vec<Repr>,
    offsets: Vec<usize>,
    bytes: usize,
    pointers: usize,
    tagged: usize,
}
#[derive(Clone, PartialEq, Eq)]
struct ListElement {
    repr: Repr,
    members: Vec<Repr>,
    declared: Option<String>,
}
struct Plan<'a> {
    root: SourceRoot,
    lang: baml_base::LangRoots,
    index: std::rc::Rc<ProgramIndex<'a>>,
    program: &'a Program,
    layouts: BTreeMap<usize, Layout>,
    bigints: bool,
    json_tree: bool,
    json_public_to: bool,
    json_to_types: Vec<baml_compiler2_mir::TyTemplate>,
    lists: BTreeSet<Repr>,
    list_elements: std::cell::RefCell<Vec<ListElement>>,
    json_types: Vec<Vec<usize>>,
    environments: bool,
    task_support: bool,
    cleanup_methods: BTreeMap<usize, usize>,
    captures: BTreeMap<usize, Vec<Repr>>,
    capture_types: BTreeMap<usize, Vec<RuntimeTy>>,
    lambda_bodies: BTreeMap<usize, usize>,
    lambda_targets: BTreeMap<(usize, usize), usize>,
    generic_functions: Vec<(usize, Vec<RuntimeTy>, usize)>,
    generic_values: BTreeMap<(usize, usize, usize), usize>,
    virtual_values: BTreeMap<(usize, usize, usize), Vec<(Repr, usize)>>,
    callables: BTreeSet<usize>,
    closure_lists: BTreeMap<u64, baml_type::RuntimeTy<usize>>,
    enums: BTreeSet<usize>,
    specialized_calls: BTreeMap<(usize, usize), usize>,
    type_codes: Vec<baml_compiler2_mir::TyTemplate>,
    reflected: bool,
    reflect_meta: bool,
    /// Static class entry, class object and its substituted field types.
    reflection_fields: Vec<(usize, usize, Vec<bex_vm_types::RuntimeTy>)>,
    reflection_types: Vec<RuntimeTy>,
    next_object: usize,
    class_instances: Vec<(usize, Vec<bex_vm_types::RuntimeTy>, usize)>,
    concrete_types: Vec<RuntimeTy>,
    heads: BTreeMap<baml_type::typetag::TypeTag, baml_type::DeclName>,
    dispatches: BTreeMap<(usize, usize), Vec<(Repr, usize)>>,
    function_frames: BTreeMap<usize, Vec<(String, RuntimeTy)>>,
    structural_strings: bool,
    structural_string_calls: BTreeSet<(usize, usize, Repr)>,
    string_overrides: BTreeMap<Repr, usize>,
    structural_equals: bool,
    equals_overrides: BTreeMap<Repr, usize>,
    alias_tests: std::cell::RefCell<BTreeSet<baml_type::DeclName>>,
    list_stringify: bool,
    list_render_roots: std::cell::RefCell<BTreeSet<Repr>>,
    json_decode_codes: BTreeSet<usize>,
    json_list_types: Vec<baml_compiler2_mir::TyTemplate>,
    map_types: std::cell::RefCell<Vec<maps::MapType>>,
    json_maps: BTreeMap<Repr, RuntimeTy>,
    map_stringify: bool,
    globals: BTreeMap<usize, Repr>,
    init_chains: Vec<(usize, Vec<(usize, usize, usize)>)>,
}
struct Emitter<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    body: &'a MirFunctionBody<'db>,
    object: usize,
    descriptors: &'a BTreeMap<usize, usize>,
    lists: &'a BTreeMap<Repr, usize>,
    all: &'a BTreeMap<usize, TestMir<'db>>,
    iters: BTreeSet<usize>,
    iter_arrays: BTreeMap<usize, usize>,
    arity: usize,
    block: usize,
    statement: usize,
    stack: stack::Placement,
    proven_arithmetic: BTreeSet<(usize, usize)>,
    unwind_jumps: Vec<String>,
    sites: Vec<usize>,
    compact_sites: Vec<u32>,
    raise_kinds: Vec<&'static str>,
    telemetry_sites: &'a std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
    site: usize,
    string_cx: std::cell::Cell<bool>,
    string_literals: std::cell::RefCell<BTreeMap<String, usize>>,
}

struct ProgramEmission<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    all: &'a BTreeMap<usize, TestMir<'db>>,
    descriptors: &'a BTreeMap<usize, usize>,
}

mod admission;
mod body;
mod builtins;
mod call_cost;
mod capture_codegen;
mod closures;
mod containers;
mod diagnostics;
mod dispatch;
mod driver;
mod errors;
mod expressions;
mod host;
mod json;
mod json_flat_lists;
mod json_lists;
mod json_scalars;
mod json_tree;
mod layout;
mod lists;
mod equality;
mod operators;
mod hashing;
mod loop_ranges;
mod places;
mod repr;
mod reflection;
mod stack;
mod statements;
mod telemetry;

use body::recursive;
use diagnostics::no;
use repr::Repr;

pub use driver::{RtTestCompiler, compile_rt, compile_rt_benchmark, compile_rt_test};

mod calls;
mod formatting;
mod globals;
mod map_json;
mod maps;
mod regex;
mod registry;
mod string_bytes;
mod string_defaults;
mod strings;
mod tasks;

use layout::rt_descriptors;

mod switches;

mod generics;

mod interface_fields;

mod callbacks;

mod builtin_classes;
mod calendar;
mod io;
mod io_clock;
mod logging;
mod native;
mod provider;
mod provider_schema;
mod random;
mod scalars;
