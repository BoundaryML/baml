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
    AggregateKind, BinOp, Constant, Db, MirFunction, MirFunctionBody, MirFunctionKind, Operand,
    OptLevel, Place, RuntimeTy, Rvalue, StatementKind, Terminator, UnaryOp, lower_function,
};
use bex_vm_types::{Object, Program};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

type Gen<T> = Result<T, String>;
struct Layout {
    fields: Vec<Repr>,
    offsets: Vec<usize>,
    bytes: usize,
    pointers: usize,
    tagged: usize,
}
struct Plan<'a> {
    root: SourceRoot,
    lang: baml_base::LangRoots,
    index: ProgramIndex<'a>,
    program: &'a Program,
    layouts: BTreeMap<usize, Layout>,
    lists: BTreeSet<Repr>,
    json_types: Vec<Vec<usize>>,
    enums: BTreeSet<usize>,
    specialized_calls: BTreeMap<(usize, usize), usize>,
    type_codes: Vec<baml_compiler2_mir::TyTemplate>,
    next_object: usize,
    class_instances: Vec<(usize, Vec<bex_vm_types::RuntimeTy>, usize)>,
    concrete_types: Vec<RuntimeTy>,
    heads: BTreeMap<baml_type::typetag::TypeTag, baml_type::DeclName>,
    dispatches: BTreeMap<(usize, usize), Vec<(Repr, usize)>>,
    function_frames: BTreeMap<usize, Vec<(String, RuntimeTy)>>,
    structural_strings: bool,
    structural_string_calls: BTreeSet<(usize, usize, Repr)>,
    string_overrides: BTreeMap<Repr, usize>,
    alias_tests: std::cell::RefCell<BTreeSet<baml_type::DeclName>>,
}
struct Emitter<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    body: &'a MirFunctionBody<'db>,
    object: usize,
    descriptors: &'a BTreeMap<usize, usize>,
    lists: &'a BTreeMap<Repr, usize>,
    all: &'a BTreeMap<usize, MirFunction<'db>>,
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
    all: &'a BTreeMap<usize, MirFunction<'db>>,
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
mod layout;
mod loop_ranges;
mod places;
mod repr;
mod stack;
mod statements;
mod telemetry;

use body::recursive;
use diagnostics::no;
use repr::Repr;

pub use driver::{compile_rt, compile_rt_benchmark};

mod calls;
mod formatting;
mod globals;
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
