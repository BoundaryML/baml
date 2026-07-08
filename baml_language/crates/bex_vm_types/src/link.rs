//! Load-time linker (B-693 §3): folds symbolic [`CompilationUnit`]s into a
//! runnable [`Program`].
//!
//! The linker assigns final absolute `ObjectIndex`/`GlobalIndex` values,
//! resolves each import against the merged export table, and patches operands
//! by calling the *existing* [`relink::visit_object_operands`](crate::relink)
//! walker with a per-unit relocation closure — the walker is the link-time
//! patcher, not the input to a bespoke splice heuristic. The output `Program`
//! is unchanged in shape; borsh of `link(all units)` must equal borsh of a full
//! compile (the byte-identity oracle, design §5).
//!
//! # Stage 1 scope
//!
//! This module implements only the trivial skeleton: linking units whose
//! references are all **local** (empty import tables). It places each unit's
//! objects at a running base and rebases local operands `Local(i) ->
//! object_base + i`. The full pass-major interleaving, import resolution,
//! generic-function interning, and `$init` synthesis (design §3b) are Stage 2,
//! marked with `STAGE 2:` below. For a single local-only unit the skeleton
//! already reproduces the flat-`Program` layout, which is the Stage 1 gate.

use std::collections::HashMap;

use crate::{
    ConstValue, GlobalIndex, ObjectIndex, Program,
    relink::{IndexOperand, visit_object_operands},
    unit::{CompilationUnit, LocalRef},
};

/// An error raised while linking symbolic units into a [`Program`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a fully-qualified name that no unit exports. In Stage 1
    /// this also fires for *any* import, because cross-unit resolution is not
    /// wired yet.
    UnresolvedImport(String),
    /// Two units export the same fully-qualified name.
    DuplicateExport(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedImport(name) => {
                write!(f, "unresolved import: no unit exports `{name}`")
            }
            Self::DuplicateExport(name) => {
                write!(f, "duplicate export: `{name}` is exported by more than one unit")
            }
        }
    }
}

impl std::error::Error for LinkError {}

/// Total number of pool objects a unit contributes (across all four buckets).
fn unit_object_count(unit: &CompilationUnit) -> usize {
    unit.classes.len() + unit.enums.len() + unit.interfaces.len() + unit.code.len()
}

/// Absolute object index of a unit-local [`LocalRef`], given the unit's object
/// base. Buckets are laid out classes, then enums, then interfaces, then code
/// (§2a / §3b), so each bucket contributes its predecessors' lengths.
fn local_ref_abs(unit: &CompilationUnit, object_base: usize, local_ref: LocalRef) -> usize {
    let classes = unit.classes.len();
    let enums = unit.enums.len();
    let interfaces = unit.interfaces.len();
    match local_ref {
        LocalRef::Class(k) => object_base + k as usize,
        LocalRef::Enum(k) => object_base + classes + k as usize,
        LocalRef::Interface(k) => object_base + classes + enums + k as usize,
        LocalRef::Code(k) => object_base + classes + enums + interfaces + k as usize,
    }
}

/// Link symbolic units into a runnable [`Program`].
///
/// `units` must already be in the deterministic file-discovery order (builtins
/// first, then user files) — the linker's positional appends follow that order.
///
/// # Stage 1 behavior
///
/// Handles only units with empty import tables. Objects are placed per unit at a
/// running base (whole-unit concatenation), local operands are rebased through
/// [`relink::visit_object_operands`](crate::relink), each function's global slot
/// is filled with `ConstValue::Object(its object)`, each `let`'s slot is left
/// `Null`, and the function/let name maps are populated. `$init` synthesis,
/// generic-fn interning, package merging, and client metadata are deferred.
///
/// # Errors
///
/// [`LinkError::UnresolvedImport`] if any unit has a non-empty import table
/// (Stage 1 cannot resolve cross-unit references yet), and
/// [`LinkError::DuplicateExport`] if two units claim the same fully-qualified
/// function or `let` name.
pub fn link(units: &[CompilationUnit]) -> Result<Program, LinkError> {
    // STAGE 1 restriction: every reference must be LOCAL. Cross-unit imports
    // need the merged export table + generic-fn interning of §3b step 3.
    // STAGE 2: resolve `object_imports`/`global_imports` here — look each
    // `Symbol` up in the name maps built below (generic-fn imports run through
    // the intern step) instead of refusing to link.
    for unit in units {
        if let Some(import) = unit.object_imports.first().or(unit.global_imports.first()) {
            return Err(LinkError::UnresolvedImport(import.fq_name.clone()));
        }
    }

    let mut program = Program::new();

    // ---- Precompute per-unit bases -------------------------------------
    // STAGE 2: this is whole-unit concatenation — each unit's objects land in
    // one contiguous block. Byte-identity with a full compile needs pass-major
    // interleaving ACROSS units (all units' classes, then all units' enums, …;
    // design §3b steps 2-3 / §9 R3). For a single unit the two coincide, which
    // is the Stage 1 gate.
    let mut object_base = Vec::with_capacity(units.len());
    let mut global_base = Vec::with_capacity(units.len());
    let mut object_acc = 0usize;
    let mut global_acc = 0usize;
    for unit in units {
        object_base.push(object_acc);
        global_base.push(global_acc);
        object_acc += unit_object_count(unit);
        global_acc += unit.exports.globals.len();
    }
    // Every slot is filled below: functions get `ConstValue::Object`, `let`s
    // stay `Null` until `$init` stores their computed value at load time.
    program.globals = vec![ConstValue::Null; global_acc];

    // ---- Global-slot assignment (design §3b step 1) --------------------
    for (u, unit) in units.iter().enumerate() {
        // fq_name -> LocalRef for this unit's exported objects.
        let objects_by_name: HashMap<&str, LocalRef> = unit
            .exports
            .objects
            .iter()
            .map(|(name, local_ref)| (name.as_str(), *local_ref))
            .collect();

        for (name, ordinal) in &unit.exports.globals {
            let slot = global_base[u] + *ordinal as usize;
            match objects_by_name.get(name.as_str()) {
                // A function: its slot holds `Object(function object)`.
                Some(local_ref @ LocalRef::Code(_)) => {
                    let abs = local_ref_abs(unit, object_base[u], *local_ref);
                    program.globals[slot] = ConstValue::Object(ObjectIndex::from_raw(abs));
                    if program.function_indices.insert(name.clone(), abs).is_some() {
                        return Err(LinkError::DuplicateExport(name.clone()));
                    }
                    program.function_global_indices.insert(name.clone(), slot);
                }
                // Anything not backed by a code object owns a `let` slot: it
                // stays `Null` until `$init` runs.
                _ => {
                    if program
                        .let_global_indices
                        .insert(name.clone(), slot)
                        .is_some()
                    {
                        return Err(LinkError::DuplicateExport(name.clone()));
                    }
                }
            }
        }
    }

    // ---- Definition + code placement (design §3b steps 2-3) ------------
    for (u, unit) in units.iter().enumerate() {
        let obase = object_base[u];
        let gbase = global_base[u];
        // Bucket order: classes, enums, interfaces, code (the pool-prefix order
        // a full compile produces).
        let buckets = unit
            .classes
            .iter()
            .chain(&unit.enums)
            .chain(&unit.interfaces)
            .chain(&unit.code);
        for object in buckets {
            let mut object = object.clone();
            // The relink walker *is* the whole splice: rebase every LOCAL index
            // operand. `raw` is a flat unit-local index (§2a); with imports
            // rejected above it is always `< n_local`, so `obase + raw` /
            // `gbase + raw` is the absolute slot.
            visit_object_operands(&mut object, |operand| match operand {
                IndexOperand::Object(idx) => {
                    *idx = ObjectIndex::from_raw(obase + idx.raw());
                }
                IndexOperand::Global(slot) => {
                    *slot = GlobalIndex::from_raw(gbase + slot.raw());
                }
            });
            program.objects.push(object);
        }
    }

    // ---- Whole-program tails (design §3b step 5, partial) --------------
    // These concatenations follow unit order, so they are already correct in
    // Stage 1. The remaining whole-program synthesis is deferred:
    // STAGE 2: $init / $init_test synthesis from `unit.lets` -> package_init_order (R2);
    //          generic-function interning across units (R1);
    //          merge `unit.package_fragment` into `program.packages` (symbolic -> absolute);
    //          client_metadata (flows through $init);
    //          class-tag 47-bit collision check (moves to link time).
    for unit in units {
        program.test_cases.extend(unit.test_cases.iter().cloned());
        for macro_fragment in &unit.template_macros {
            program.template_strings_macros.push_str(macro_fragment);
        }
    }

    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Instruction, Object,
        bytecode::Bytecode,
        relink::visit_index_operands,
        types::{Class, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin},
        unit::{ExportTable, ProgramPackageFrag},
    };

    fn func(name: &str, instructions: Vec<Instruction>) -> Object {
        let bytecode = Bytecode {
            instructions,
            ..Bytecode::default()
        };
        Object::Function(Box::new(Function {
            name: name.to_string(),
            source_file: "user.baml".to_string(),
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: baml_type::RuntimeTy::unknown(),
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: None,
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
        }))
    }

    fn class(name: &str, type_tag: i64) -> Object {
        Object::Class(Box::new(Class {
            name: baml_type::TypeName::local(baml_base::Name::new(name)),
            fields: Vec::new(),
            description: None,
            alias: None,
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
        }))
    }

    /// A single local-only unit: one class + two functions that reference the
    /// class and each other through the per-unit convention.
    fn local_only_unit() -> CompilationUnit {
        use Instruction as I;
        CompilationUnit {
            source_file: "user.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("MyClass", 100)],
            enums: Vec::new(),
            interfaces: Vec::new(),
            code: vec![
                // user.foo: allocates MyClass (local obj 0), calls bar (global 1).
                func(
                    "user.foo",
                    vec![
                        I::AllocInstance {
                            class_obj: ObjectIndex::from_raw(0),
                            ntypeargs: 0,
                        },
                        I::Call {
                            callee: GlobalIndex::from_raw(1),
                            ntypeargs: 0,
                        },
                        I::Return,
                    ],
                ),
                // user.bar: reads foo's global slot (local global 0).
                func(
                    "user.bar",
                    vec![I::LoadGlobal(GlobalIndex::from_raw(0)), I::Return],
                ),
            ],
            object_imports: Vec::new(),
            global_imports: Vec::new(),
            exports: ExportTable {
                objects: vec![
                    ("user.MyClass".to_string(), LocalRef::Class(0)),
                    ("user.foo".to_string(), LocalRef::Code(0)),
                    ("user.bar".to_string(), LocalRef::Code(1)),
                ],
                globals: vec![("user.foo".to_string(), 0), ("user.bar".to_string(), 1)],
            },
            lets: Vec::new(),
            package_fragment: ProgramPackageFrag::default(),
            template_macros: Vec::new(),
            test_cases: Vec::new(),
            throw_facts: Vec::new(),
        }
    }

    /// Collect the `(globals, objects)` index operands of a function object.
    fn operands_of(object: &Object) -> (Vec<usize>, Vec<usize>) {
        let Object::Function(function) = object else {
            panic!("expected a function object");
        };
        let mut function = (**function).clone();
        let mut globals = Vec::new();
        let mut objects = Vec::new();
        visit_index_operands(&mut function, |operand| match operand {
            IndexOperand::Global(slot) => globals.push(slot.raw()),
            IndexOperand::Object(obj) => objects.push(obj.raw()),
        });
        (globals, objects)
    }

    /// The Stage 1 link gate: a single local-only unit links to a `Program`
    /// whose objects are that unit's objects at base 0 with local refs intact.
    #[test]
    fn link_single_local_only_unit() {
        let unit = local_only_unit();
        let program = link(std::slice::from_ref(&unit)).expect("link");

        // Objects: [MyClass, foo, bar] at indices 0, 1, 2 (classes then code).
        assert_eq!(program.objects.len(), 3);
        assert!(matches!(
            &program.objects[ObjectIndex::from_raw(0)],
            Object::Class(_)
        ));
        let Object::Function(foo) = &program.objects[ObjectIndex::from_raw(1)] else {
            panic!("object 1 should be user.foo");
        };
        assert_eq!(foo.name, "user.foo");

        // foo's local refs were rebased: class_obj 0 -> abs 0 (MyClass),
        // callee global 1 -> abs 1 (bar's slot).
        let (foo_globals, foo_objects) = operands_of(&program.objects[ObjectIndex::from_raw(1)]);
        assert_eq!(foo_objects, vec![0], "AllocInstance class_obj rebased");
        assert_eq!(foo_globals, vec![1], "Call callee rebased");

        // bar reads foo's slot (local global 0 -> abs 0).
        let (bar_globals, bar_objects) = operands_of(&program.objects[ObjectIndex::from_raw(2)]);
        assert_eq!(bar_globals, vec![0], "LoadGlobal rebased");
        assert!(bar_objects.is_empty());

        // Globals: two function slots holding their object indices.
        assert_eq!(program.globals.len(), 2);
        assert_eq!(
            program.globals[0],
            ConstValue::Object(ObjectIndex::from_raw(1)),
            "slot 0 = user.foo object"
        );
        assert_eq!(
            program.globals[1],
            ConstValue::Object(ObjectIndex::from_raw(2)),
            "slot 1 = user.bar object"
        );

        // Name maps.
        assert_eq!(program.function_indices.get("user.foo").copied(), Some(1));
        assert_eq!(program.function_indices.get("user.bar").copied(), Some(2));
        assert_eq!(
            program.function_global_indices.get("user.foo").copied(),
            Some(0)
        );
        assert_eq!(
            program.function_global_indices.get("user.bar").copied(),
            Some(1)
        );
        assert!(program.let_global_indices.is_empty());
    }

    /// Linking a borsh round-tripped unit yields a byte-identical `Program`
    /// (design §5 — `link(units) == link(round_trip(units))`).
    #[test]
    fn link_is_stable_across_unit_round_trip() {
        let unit = local_only_unit();
        let program = link(std::slice::from_ref(&unit)).expect("link fresh");

        let bytes = borsh::to_vec(&unit).expect("serialize unit");
        let round_tripped: CompilationUnit =
            borsh::from_slice(&bytes).expect("deserialize unit");
        let program2 = link(std::slice::from_ref(&round_tripped)).expect("link round-tripped");

        assert_eq!(
            borsh::to_vec(&program).expect("serialize program"),
            borsh::to_vec(&program2).expect("serialize program2"),
            "link output must be identical for a round-tripped unit"
        );
    }

    /// A unit with imports is rejected in Stage 1 (cross-unit resolution is
    /// Stage 2).
    #[test]
    fn link_rejects_imports_in_stage_1() {
        use crate::unit::{Symbol, SymbolKind};
        let mut unit = local_only_unit();
        unit.object_imports.push(Symbol {
            kind: SymbolKind::Class,
            fq_name: "other.External".to_string(),
            generic: None,
        });
        // `Program` has no `PartialEq`/`Debug` (Object doesn't), so match the
        // error variant rather than comparing/formatting the whole `Result`.
        match link(std::slice::from_ref(&unit)) {
            Err(LinkError::UnresolvedImport(name)) => assert_eq!(name, "other.External"),
            Err(e) => panic!("expected UnresolvedImport, got a different error: {e:?}"),
            Ok(_) => panic!("expected UnresolvedImport, got Ok"),
        }
    }
}
