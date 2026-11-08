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

type Gen<T> = Result<T, String>;
struct MirFunction<'db> {
    arity: usize,
    kind: std::borrow::Cow<'db, MirFunctionKind<'db>>,
    lambdas: std::borrow::Cow<'db, [baml_compiler2_mir::MirFunction<'db>]>,
}
impl<'db> From<&'db baml_compiler2_mir::MirFunction<'db>> for MirFunction<'db> {
    fn from(mir: &'db baml_compiler2_mir::MirFunction<'db>) -> Self {
        Self {
            arity: mir.arity,
            kind: std::borrow::Cow::Borrowed(&mir.kind),
            lambdas: std::borrow::Cow::Borrowed(&mir.lambdas),
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
struct Plan<'a> {
    lang: baml_base::LangRoots,
    index: ProgramIndex<'a>,
    program: &'a Program,
    layouts: BTreeMap<usize, Layout>,
    lists: BTreeSet<Repr>,
    map_types: std::cell::RefCell<Vec<(Repr, Repr, String, String)>>,
    json_types: Vec<Vec<usize>>,
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
    all: &'a BTreeMap<usize, MirFunction<'db>>,
    iters: BTreeSet<usize>,
    iter_arrays: BTreeMap<usize, usize>,
    arity: usize,
    block: usize,
    unwind_jumps: Vec<String>,
    sites: Vec<usize>,
    compact_sites: Vec<u32>,
    raise_kinds: Vec<&'static str>,
    telemetry_sites: &'a std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
    site: usize,
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
mod places;
mod repr;
mod statements;
mod telemetry;

use body::recursive;
use diagnostics::no;
use repr::Repr;

pub use driver::compile_rt;

mod calls;
mod globals;
mod map_json;
mod maps;
mod registry;
mod strings;
mod tasks;

use layout::rt_descriptors;
