//! BEP-066 recursive runtime-class builders.
//!
//! Opaque handles contain only plain Rust descriptors. Every BAML heap value
//! retained by a builder or pending expression is stored in an ordinary
//! instance/array field, so existing GC tracing and relocation remain the
//! single source of truth for heap reachability.

use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use baml_compiler_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticPhase,
    runtime_type::{self, DuplicateMemberKind, InvalidIdentifierKind, SerializedKeyContainer},
};
use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr,
    types::{Class, ClassField, Object, TypeValue, Value},
};
use indexmap::IndexMap;

use super::{
    BamlClassReflectClassBuilder, BamlClassReflectClassPendingType, PackageBamlImpl,
    type_kinds::{
        ReflectedTypeRow, WitnessField, alloc_compilation_error, compiler_diagnostic,
        is_baml_identifier, reflected_type_row, register_class_witnesses, validate_class_witnesses,
    },
};
use crate::{BexVm, errors::VmRustFnError};

const BUILDER_FQN: &str = "baml.reflect.class.Builder";
const PENDING_FQN: &str = "baml.reflect.class.PendingType";

static NEXT_BUILDER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct BuilderHandle {
    state: Arc<Mutex<BuilderState>>,
}

struct BuilderState {
    id: u64,
    name: String,
    fields: Vec<BuilderField>,
    frozen: bool,
}

#[derive(Clone)]
struct BuilderField {
    name: String,
    root_index: usize,
}

#[derive(Clone, Copy)]
enum PendingOp {
    Direct,
    Array,
    Optional,
    Union,
}

#[derive(Clone, Copy)]
struct PendingHandle {
    op: PendingOp,
}

struct BuilderNode {
    value: Value,
    handle: BuilderHandle,
    id: u64,
    name: String,
    fields: Vec<BuilderField>,
    frozen: bool,
    roots: Vec<Value>,
}

struct ClassPlan {
    name: baml_type::QualifiedTypeName,
    ptr: HeapPtr,
    ty: bex_vm_types::RealizedTy,
}

struct ClassIdentityPlan {
    name: baml_type::QualifiedTypeName,
    ty: bex_vm_types::RealizedTy,
}

enum PreparedFieldType {
    Concrete(Box<TypeValue>),
    Pending(Value),
}

struct PreparedField {
    name: String,
    field_type: PreparedFieldType,
    alias: Option<String>,
    description: Option<String>,
    docstring: Option<String>,
    other: IndexMap<String, String>,
}

type PreparedGroup = IndexMap<u64, Vec<PreparedField>>;
type BuilderDiagnostics = Vec<Diagnostic>;

fn lock_state(handle: &BuilderHandle) -> MutexGuard<'_, BuilderState> {
    handle
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn class_name(vm: &BexVm, value: Value) -> Option<String> {
    let instance = vm.as_instance(&value).ok()?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        return None;
    };
    Some(class.name.to_string())
}

fn builder_parts(vm: &BexVm, value: Value) -> Result<(HeapPtr, BuilderHandle), String> {
    if class_name(vm, value).as_deref() != Some(BUILDER_FQN) {
        return Err("expected a reflect.class.Builder value".into());
    }
    let ptr = value
        .as_object_ptr()
        .ok_or_else(|| "builder is not a heap value".to_string())?;
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "builder is not an instance".to_string())?;
    let handle = vm
        .as_rust_data::<BuilderHandle>(&instance.load_field(0))
        .map_err(|_| "builder has invalid native state".to_string())?
        .clone();
    Ok((ptr, handle))
}

fn pending_parts(vm: &BexVm, value: Value) -> Result<(HeapPtr, PendingHandle), String> {
    if class_name(vm, value).as_deref() != Some(PENDING_FQN) {
        return Err("expected a reflect.class.PendingType value".into());
    }
    let ptr = value
        .as_object_ptr()
        .ok_or_else(|| "pending type is not a heap value".to_string())?;
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "pending type is not an instance".to_string())?;
    let handle = *vm
        .as_rust_data::<PendingHandle>(&instance.load_field(0))
        .map_err(|_| "pending type has invalid native state".to_string())?;
    Ok((ptr, handle))
}

fn instance_array_field(vm: &BexVm, value: Value, index: usize) -> Result<Vec<Value>, String> {
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "expected an instance".to_string())?;
    vm.as_array(&instance.load_field(index))
        .map(|array| array.to_vec())
        .map_err(|_| "native roots field is not an array".to_string())
}

fn instance_field(vm: &BexVm, value: Value, index: usize) -> Result<Value, String> {
    vm.as_instance(&value)
        .map(|instance| instance.load_field(index))
        .map_err(|_| "expected an instance".to_string())
}

fn store_instance_field(
    vm: &BexVm,
    value: Value,
    index: usize,
    field: Value,
) -> Result<(), String> {
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "expected an instance".to_string())?;
    instance
        .try_store_field(index, field)
        .map_err(|_| "native instance field index is invalid".to_string())?;
    // A builder instance can be arbitrarily old by the time `build()` stores the
    // freshly minted `Object::Type` into it; dirty its card so a minor collection
    // rescans the instance instead of dropping the young type value.
    if let Some(instance_ptr) = value.as_object_ptr() {
        vm.tlab.heap().write_barrier(instance_ptr, field);
    }
    Ok(())
}

fn replace_array_field(
    vm: &mut BexVm,
    value: Value,
    index: usize,
    values: Vec<Value>,
) -> Result<(), String> {
    let array = Value::object(
        vm.tlab
            .alloc_array(bex_vm_types::RealizedTy::unknown(), values),
    );
    store_instance_field(vm, value, index, array)
}

fn append_unique_neighbor(vm: &mut BexVm, builder: Value, neighbor: Value) -> Result<(), String> {
    let (_, neighbor_handle) = builder_parts(vm, neighbor)?;
    let neighbor_id = lock_state(&neighbor_handle).id;
    let mut values = instance_array_field(vm, builder, 2)?;
    let already_present = values.iter().any(|value| {
        builder_parts(vm, *value)
            .map(|(_, handle)| lock_state(&handle).id == neighbor_id)
            .unwrap_or(false)
    });
    if !already_present {
        values.push(neighbor);
        replace_array_field(vm, builder, 2, values)?;
    }
    Ok(())
}

fn alloc_pending(vm: &mut BexVm, op: PendingOp, roots: Vec<Value>) -> Value {
    let class = vm.resolve_class(PENDING_FQN);
    let handle = Value::object(vm.alloc_rust_data(Arc::new(PendingHandle { op })));
    let roots = Value::object(vm.tlab.alloc_array(bex_vm_types::RealizedTy::unknown(), roots));
    Value::object(vm.alloc_instance(class, vec![handle, roots, Value::NULL]))
}

pub(crate) fn alloc_builder(vm: &mut BexVm, name: &str) -> Value {
    let class = vm.resolve_class(BUILDER_FQN);
    let handle = BuilderHandle {
        state: Arc::new(Mutex::new(BuilderState {
            id: NEXT_BUILDER_ID.fetch_add(1, Ordering::Relaxed),
            name: name.to_string(),
            fields: Vec::new(),
            frozen: false,
        })),
    };
    let handle = Value::object(vm.alloc_rust_data(Arc::new(handle)));
    let roots = Value::object(
        vm.tlab
            .alloc_array(bex_vm_types::RealizedTy::unknown(), Vec::new()),
    );
    let neighbors = Value::object(
        vm.tlab
            .alloc_array(bex_vm_types::RealizedTy::unknown(), Vec::new()),
    );
    Value::object(vm.alloc_instance(class, vec![handle, roots, neighbors, Value::NULL]))
}

fn compilation_error(vm: &mut BexVm, id: DiagnosticId, message: String) -> VmRustFnError {
    VmRustFnError::Thrown(alloc_compilation_error(
        vm,
        &[compiler_diagnostic(id, message)],
    ))
}

fn shared_compilation_error(vm: &mut BexVm, diagnostic: Diagnostic) -> VmRustFnError {
    VmRustFnError::Thrown(alloc_compilation_error(vm, &[diagnostic]))
}

impl BamlClassReflectClassBuilder for PackageBamlImpl {
    fn field(
        vm: &mut BexVm,
        builder: &Value,
        name: &bex_str::BexStr,
        ty: &Value,
    ) -> Result<Value, VmRustFnError> {
        let (_, handle) = builder_parts(vm, *builder)
            .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
        let builder_name = lock_state(&handle).name.clone();
        {
            let state = lock_state(&handle);
            if state.frozen {
                return Err(compilation_error(
                    vm,
                    DiagnosticId::TypeMismatch,
                    format!("class builder `{builder_name}` is frozen and cannot be mutated"),
                ));
            }
            if state.fields.iter().any(|field| field.name == name.as_str()) {
                return Err(shared_compilation_error(
                    vm,
                    runtime_type::duplicate_member(
                        DuplicateMemberKind::Field,
                        &builder_name,
                        name.as_str(),
                    ),
                ));
            }
        }

        let (stored_type, pending_builders) = if class_name(vm, *ty).as_deref() == Some(PENDING_FQN)
        {
            match resolve_pending_if_ready(vm, *ty)
                .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?
            {
                Some(resolved) => {
                    reflected_type_row(vm, resolved).map_err(|message| {
                        compilation_error(vm, DiagnosticId::TypeMismatch, message)
                    })?;
                    (resolved, Vec::new())
                }
                None => {
                    let builders = direct_builders(vm, *ty).map_err(|message| {
                        compilation_error(vm, DiagnosticId::TypeMismatch, message)
                    })?;
                    let mut unfrozen = Vec::with_capacity(builders.len());
                    for target in builders {
                        let (_, target_handle) = builder_parts(vm, target).map_err(|message| {
                            compilation_error(vm, DiagnosticId::TypeMismatch, message)
                        })?;
                        if !lock_state(&target_handle).frozen {
                            unfrozen.push(target);
                        }
                    }
                    (*ty, unfrozen)
                }
            }
        } else {
            reflected_type_row(vm, *ty)
                .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
            (*ty, Vec::new())
        };

        let mut roots = instance_array_field(vm, *builder, 1)
            .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
        let root_index = roots.len();
        roots.push(stored_type);
        replace_array_field(vm, *builder, 1, roots)
            .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
        lock_state(&handle).fields.push(BuilderField {
            name: name.to_string(),
            root_index,
        });

        for target in pending_builders {
            append_unique_neighbor(vm, *builder, target)
                .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
            append_unique_neighbor(vm, target, *builder)
                .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
        }

        Ok(*builder)
    }

    fn type_(vm: &mut BexVm, builder: &Value) -> Value {
        alloc_pending(vm, PendingOp::Direct, vec![*builder])
    }

    fn _build(
        vm: &mut BexVm,
        builder: &Value,
        implementations: &[Value],
    ) -> Result<Value, VmRustFnError> {
        build_group(vm, *builder, implementations)
    }
}

impl BamlClassReflectClassPendingType for PackageBamlImpl {
    fn array(vm: &mut BexVm, pendingtype: &Value) -> Value {
        alloc_pending(vm, PendingOp::Array, vec![*pendingtype])
    }

    fn optional(vm: &mut BexVm, pendingtype: &Value) -> Value {
        alloc_pending(vm, PendingOp::Optional, vec![*pendingtype])
    }

    fn union(vm: &mut BexVm, pendingtype: &Value, others: &[Value]) -> Value {
        let mut roots = Vec::with_capacity(others.len() + 1);
        roots.push(*pendingtype);
        roots.extend_from_slice(others);
        alloc_pending(vm, PendingOp::Union, roots)
    }

    fn resolved(vm: &mut BexVm, pendingtype: &Value) -> Option<Value> {
        resolve_pending_if_ready(vm, *pendingtype).ok().flatten()
    }
}

fn direct_builders(vm: &BexVm, pending: Value) -> Result<Vec<Value>, String> {
    fn walk(
        vm: &BexVm,
        pending: Value,
        seen_pending: &mut HashSet<HeapPtr>,
        seen_builders: &mut HashSet<u64>,
        builders: &mut Vec<Value>,
    ) -> Result<(), String> {
        let (ptr, handle) = pending_parts(vm, pending)?;
        if !seen_pending.insert(ptr) {
            return Ok(());
        }
        for root in instance_array_field(vm, pending, 1)? {
            match handle.op {
                PendingOp::Direct => {
                    let (_, builder_handle) = builder_parts(vm, root)?;
                    let id = lock_state(&builder_handle).id;
                    if seen_builders.insert(id) {
                        builders.push(root);
                    }
                }
                PendingOp::Array | PendingOp::Optional | PendingOp::Union => {
                    if class_name(vm, root).as_deref() == Some(PENDING_FQN) {
                        walk(vm, root, seen_pending, seen_builders, builders)?;
                    }
                }
            }
        }
        Ok(())
    }

    let mut builders = Vec::new();
    walk(
        vm,
        pending,
        &mut HashSet::new(),
        &mut HashSet::new(),
        &mut builders,
    )?;
    Ok(builders)
}

fn collect_group(vm: &BexVm, start: Value) -> Result<IndexMap<u64, BuilderNode>, String> {
    let mut group = IndexMap::new();
    let mut queue = VecDeque::from([start]);
    while let Some(value) = queue.pop_front() {
        let (_, handle) = builder_parts(vm, value)?;
        let state = lock_state(&handle);
        let id = state.id;
        if group.contains_key(&id) {
            continue;
        }
        let node = BuilderNode {
            value,
            handle: handle.clone(),
            id,
            name: state.name.clone(),
            fields: state.fields.clone(),
            frozen: state.frozen,
            roots: instance_array_field(vm, value, 1)?,
        };
        drop(state);
        queue.extend(instance_array_field(vm, value, 2)?);
        group.insert(id, node);
    }
    Ok(group)
}

fn prepare_group(
    vm: &BexVm,
    group: &IndexMap<u64, BuilderNode>,
) -> Result<PreparedGroup, BuilderDiagnostics> {
    let mut diagnostics = Vec::new();
    let mut names = HashSet::new();
    let mut prepared = IndexMap::new();

    for node in group.values() {
        if !is_baml_identifier(&node.name) {
            diagnostics.push(
                runtime_type::invalid_identifier(InvalidIdentifierKind::Class, &node.name)
                    .with_phase(DiagnosticPhase::Hir),
            );
        }
        if !names.insert(node.name.clone()) {
            diagnostics.push(compiler_diagnostic(
                DiagnosticId::DuplicateName,
                format!(
                    "duplicate class name `{}` in recursive builder group",
                    node.name
                ),
            ));
        }

        let mut fields = Vec::with_capacity(node.fields.len());
        let mut serialized_keys = HashSet::new();
        for field in &node.fields {
            if !is_baml_identifier(&field.name) {
                diagnostics.push(
                    runtime_type::invalid_identifier(
                        InvalidIdentifierKind::Field,
                        &format!("{}.{}", node.name, field.name),
                    )
                    .with_phase(DiagnosticPhase::Hir),
                );
            }
            let Some(&root) = node.roots.get(field.root_index) else {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "class builder `{}` has invalid native field state",
                        node.name
                    ),
                ));
                continue;
            };

            let row = if class_name(vm, root).as_deref() == Some(PENDING_FQN) {
                match validate_pending(vm, root, group) {
                    Ok(()) => PreparedField {
                        name: field.name.clone(),
                        field_type: PreparedFieldType::Pending(root),
                        alias: None,
                        description: None,
                        docstring: None,
                        other: IndexMap::new(),
                    },
                    Err(message) => {
                        diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                        continue;
                    }
                }
            } else {
                match reflected_type_row(vm, root) {
                    Ok(ReflectedTypeRow {
                        type_value,
                        alias,
                        description,
                        docstring,
                        other,
                    }) => PreparedField {
                        name: field.name.clone(),
                        field_type: PreparedFieldType::Concrete(Box::new(type_value)),
                        alias,
                        description,
                        docstring,
                        other,
                    },
                    Err(message) => {
                        diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                        continue;
                    }
                }
            };
            let serialized = row.alias.as_deref().unwrap_or(&row.name);
            if !serialized_keys.insert(serialized.to_string()) {
                diagnostics.push(runtime_type::duplicate_serialized_key(
                    serialized,
                    SerializedKeyContainer::Class,
                ));
            }
            fields.push(row);
        }
        prepared.insert(node.id, fields);
    }

    if diagnostics.is_empty() {
        Ok(prepared)
    } else {
        Err(diagnostics)
    }
}

fn validate_pending(
    vm: &BexVm,
    pending: Value,
    group: &IndexMap<u64, BuilderNode>,
) -> Result<(), String> {
    fn walk(
        vm: &BexVm,
        pending: Value,
        group: &IndexMap<u64, BuilderNode>,
        seen: &mut HashSet<HeapPtr>,
    ) -> Result<(), String> {
        let (ptr, handle) = pending_parts(vm, pending)?;
        if !seen.insert(ptr) {
            return Err("a pending type composite contains a cycle".into());
        }
        let roots = instance_array_field(vm, pending, 1)?;
        match handle.op {
            PendingOp::Direct => {
                if roots.len() != 1 {
                    return Err("a direct pending type has invalid native state".into());
                }
                let (_, builder) = builder_parts(vm, roots[0])?;
                let state = lock_state(&builder);
                let resolved = instance_field(vm, roots[0], 3)?;
                if !group.contains_key(&state.id) && (!state.frozen || resolved.is_null()) {
                    return Err(format!(
                        "pending type references builder `{}` outside its connected group",
                        state.name
                    ));
                }
            }
            PendingOp::Array | PendingOp::Optional => {
                if roots.len() != 1 || class_name(vm, roots[0]).as_deref() != Some(PENDING_FQN) {
                    return Err("a pending type composite has invalid native state".into());
                }
                walk(vm, roots[0], group, seen)?;
            }
            PendingOp::Union => {
                if roots.is_empty() {
                    return Err("a pending union must contain at least one member".into());
                }
                for root in roots {
                    if class_name(vm, root).as_deref() == Some(PENDING_FQN) {
                        walk(vm, root, group, seen)?;
                    } else {
                        reflected_type_row(vm, root)?;
                    }
                }
            }
        }
        seen.remove(&ptr);
        Ok(())
    }

    walk(vm, pending, group, &mut HashSet::new())
}

fn planned_pending_type(
    vm: &BexVm,
    pending: Value,
    plans: &IndexMap<u64, ClassIdentityPlan>,
) -> Result<bex_vm_types::RealizedTy, String> {
    let prior = instance_field(vm, pending, 2)?;
    if !prior.is_null() {
        return cloned_type_value(vm, prior)
            .map(|value| value.ty)
            .ok_or_else(|| "pending type resolved to a non-type value".to_string());
    }

    let (_, handle) = pending_parts(vm, pending)?;
    let roots = instance_array_field(vm, pending, 1)?;
    match handle.op {
        PendingOp::Direct => {
            if roots.len() != 1 {
                return Err("a direct pending type has invalid native state".into());
            }
            let (_, builder) = builder_parts(vm, roots[0])?;
            let id = lock_state(&builder).id;
            if let Some(plan) = plans.get(&id) {
                return Ok(plan.ty.clone());
            }
            let resolved = instance_field(vm, roots[0], 3)?;
            if resolved.is_null() {
                return Err("pending type references a builder outside its planned group".into());
            }
            cloned_type_value(vm, resolved)
                .map(|value| value.ty)
                .ok_or_else(|| "class builder resolved to a non-type value".to_string())
        }
        PendingOp::Array => {
            if roots.len() != 1 || class_name(vm, roots[0]).as_deref() != Some(PENDING_FQN) {
                return Err("a pending type composite has invalid native state".into());
            }
            Ok(bex_vm_types::RealizedTy::List(
                Box::new(planned_pending_type(vm, roots[0], plans)?),
                baml_type::TyAttr::default(),
            ))
        }
        PendingOp::Optional => {
            if roots.len() != 1 || class_name(vm, roots[0]).as_deref() != Some(PENDING_FQN) {
                return Err("a pending type composite has invalid native state".into());
            }
            let base = planned_pending_type(vm, roots[0], plans)?;
            let mut members = match base {
                bex_vm_types::RealizedTy::Union(members, _) => members,
                other => vec![other],
            };
            if !members.iter().any(bex_vm_types::RealizedTy::is_null) {
                members.push(bex_vm_types::RealizedTy::null());
            }
            Ok(bex_vm_types::RealizedTy::Union(
                members,
                baml_type::TyAttr::default(),
            ))
        }
        PendingOp::Union => {
            if roots.is_empty() {
                return Err("a pending union must contain at least one member".into());
            }
            let members = roots
                .into_iter()
                .map(|root| {
                    if class_name(vm, root).as_deref() == Some(PENDING_FQN) {
                        planned_pending_type(vm, root, plans)
                    } else {
                        reflected_type_row(vm, root).map(|row| row.type_value.ty)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(bex_vm_types::RealizedTy::Union(
                members,
                baml_type::TyAttr::default(),
            ))
        }
    }
}

fn planned_witness_fields(
    vm: &BexVm,
    fields: &[PreparedField],
    plans: &IndexMap<u64, ClassIdentityPlan>,
) -> Result<Vec<WitnessField>, String> {
    fields
        .iter()
        .map(|field| {
            let ty = match &field.field_type {
                PreparedFieldType::Concrete(value) => value.ty.clone(),
                PreparedFieldType::Pending(value) => planned_pending_type(vm, *value, plans)?,
            };
            Ok(WitnessField {
                name: field.name.clone(),
                ty,
            })
        })
        .collect()
}

fn build_group(
    vm: &mut BexVm,
    start: Value,
    implementations: &[Value],
) -> Result<Value, VmRustFnError> {
    let group = collect_group(vm, start)
        .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
    let (_, start_handle) = builder_parts(vm, start)
        .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
    let start_id = lock_state(&start_handle).id;

    if group.values().all(|node| node.frozen) {
        let resolved = instance_field(vm, start, 3)
            .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
        if !resolved.is_null() {
            return Ok(resolved);
        }
    }
    if group.values().any(|node| node.frozen) {
        return Err(compilation_error(
            vm,
            DiagnosticId::TypeMismatch,
            "recursive builder group is only partially frozen".into(),
        ));
    }

    let prepared = match prepare_group(vm, &group) {
        Ok(prepared) => prepared,
        Err(diagnostics) => {
            return Err(VmRustFnError::Thrown(alloc_compilation_error(
                vm,
                &diagnostics,
            )));
        }
    };
    // A class's type is headed at its own declaration, so the declarations come
    // first — as field-less placeholders. A recursive group needs that order
    // anyway: members name each other, and only a pointer can be handed out
    // before the thing it points at is complete.
    let mut plans = IndexMap::new();
    let mut identities = IndexMap::new();
    for node in group.values() {
        let name = baml_type::QualifiedTypeName::runtime_local(
            baml_type::Name::new(node.name.as_str()),
            vm.tlab.heap().next_synthetic_name_id(),
        );
        // Counter tag: runtime-created declarations are identified by their
        // heap object, and by this tag across a serialization boundary; the
        // synthesized `$dyn` name is display data only.
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let ptr = vm.tlab.alloc(Object::Class(Box::new(Class {
            name: name.clone(),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: HeapPtr::null(),
        })));
        let ty = bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(ptr, type_tag),
            Vec::new(),
            baml_type::TyAttr::default(),
        );
        identities.insert(
            node.id,
            ClassIdentityPlan {
                name: name.clone(),
                ty: ty.clone(),
            },
        );
        plans.insert(node.id, ClassPlan { name, ptr, ty });
    }

    let witness_fields = planned_witness_fields(vm, &prepared[&start_id], &identities)
        .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
    let mut witness_diagnostics = Vec::new();
    let witnesses = validate_class_witnesses(
        vm,
        &witness_fields,
        implementations,
        &mut witness_diagnostics,
    );
    if !witness_diagnostics.is_empty() {
        return Err(VmRustFnError::Thrown(alloc_compilation_error(
            vm,
            &witness_diagnostics,
        )));
    }
    for node in group.values() {
        let plan = &plans[&node.id];
        let value = Value::object(vm.tlab.alloc_type(TypeValue::new(plan.ty.clone())));
        store_instance_field(vm, node.value, 3, value)
            .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
    }

    for node in group.values() {
        let mut class_fields = Vec::new();
        for field in &prepared[&node.id] {
            let type_value = match &field.field_type {
                PreparedFieldType::Concrete(value) => (**value).clone(),
                PreparedFieldType::Pending(value) => {
                    let resolved = resolve_pending_if_ready(vm, *value)
                        .map_err(|message| {
                            compilation_error(vm, DiagnosticId::TypeMismatch, message)
                        })?
                        .unwrap_or_else(|| {
                            unreachable!("all builders are resolved before pending composites")
                        });
                    cloned_type_value(vm, resolved).unwrap_or_else(|| {
                        unreachable!("pending resolution must produce Object::Type")
                    })
                }
            };
            class_fields.push(ClassField {
                name: field.name.clone(),
                field_type: type_value.ty.clone().into(),
                field_template: bex_vm_types::TyTemplate::from(type_value.ty.clone()),
                description: field.description.clone(),
                alias: field.alias.clone(),
                docstring: field.docstring.clone(),
                other: field.other.clone(),
                skip: false,
                runtime_type: Some(type_value),
            });
        }
        let plan = &plans[&node.id];
        let Object::Class(class) = vm.get_object_mut(plan.ptr) else {
            unreachable!("builder class placeholder changed variant")
        };
        class.fields = class_fields;
    }

    for plan in plans.values() {
        vm.dynamic_dispatch
            .register_class(plan.name.clone(), plan.ptr);
    }
    let start_plan = &plans[&start_id];
    register_class_witnesses(vm, start_plan.ptr, &start_plan.ty, witnesses);

    for node in group.values() {
        lock_state(&node.handle).frozen = true;
    }
    let start_value = instance_field(vm, group[&start_id].value, 3)
        .map_err(|message| compilation_error(vm, DiagnosticId::TypeMismatch, message))?;
    Ok(start_value)
}

fn cloned_type_value(vm: &BexVm, value: Value) -> Option<TypeValue> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(value) = vm.get_object(ptr) else {
        return None;
    };
    Some((**value).clone())
}

fn resolve_pending_if_ready(vm: &mut BexVm, pending: Value) -> Result<Option<Value>, String> {
    let (_, handle) = pending_parts(vm, pending)?;
    let prior = instance_field(vm, pending, 2)?;
    if !prior.is_null() {
        return Ok(Some(prior));
    }
    let roots = instance_array_field(vm, pending, 1)?;

    let mut resolved_roots = Vec::with_capacity(roots.len());
    for root in roots {
        if class_name(vm, root).as_deref() == Some(PENDING_FQN) {
            let Some(value) = resolve_pending_if_ready(vm, root)? else {
                return Ok(None);
            };
            resolved_roots.push(value);
        } else if class_name(vm, root).as_deref() == Some(BUILDER_FQN) {
            let value = instance_field(vm, root, 3)?;
            if value.is_null() {
                return Ok(None);
            }
            resolved_roots.push(value);
        } else {
            resolved_roots.push(root);
        }
    }

    let resolved = match handle.op {
        PendingOp::Direct => resolved_roots
            .first()
            .copied()
            .ok_or_else(|| "a direct pending type has invalid native state".to_string())?,
        PendingOp::Array | PendingOp::Optional | PendingOp::Union => {
            let mut values = Vec::with_capacity(resolved_roots.len());
            for root in resolved_roots {
                values.push(reflected_type_row(vm, root)?.type_value.ty);
            }
            let ty = match handle.op {
                PendingOp::Array => bex_vm_types::RealizedTy::List(
                    Box::new(
                        values
                            .into_iter()
                            .next()
                            .ok_or_else(|| "a pending array has no element".to_string())?,
                    ),
                    baml_type::TyAttr::default(),
                ),
                PendingOp::Optional => {
                    let value = values
                        .into_iter()
                        .next()
                        .ok_or_else(|| "a pending optional has no base".to_string())?;
                    let mut members = match value {
                        bex_vm_types::RealizedTy::Union(members, _) => members,
                        other => vec![other],
                    };
                    if !members.iter().any(bex_vm_types::RealizedTy::is_null) {
                        members.push(bex_vm_types::RealizedTy::null());
                    }
                    bex_vm_types::RealizedTy::Union(members, baml_type::TyAttr::default())
                }
                PendingOp::Union => {
                    bex_vm_types::RealizedTy::Union(values, baml_type::TyAttr::default())
                }
                PendingOp::Direct => unreachable!(),
            };
            Value::object(vm.tlab.alloc_type(TypeValue::new(ty)))
        }
    };
    store_instance_field(vm, pending, 2, resolved)?;
    Ok(Some(resolved))
}

fn unresolved_builder_names(vm: &BexVm, pending: Value) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for builder in direct_builders(vm, pending)? {
        if instance_field(vm, builder, 3)?.is_null() {
            let (_, handle) = builder_parts(vm, builder)?;
            let name = lock_state(&handle).name.clone();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    Ok(names)
}

/// Coerce a `PendingType` supplied through an `unknown` runtime type-argument
/// position. `None` means the value is not a `PendingType`. A resolved pending is
/// transparent; an unresolved one throws a structured diagnostic naming every
/// builder that still needs to be frozen.
pub(crate) fn coerce_pending_type_arg(
    vm: &mut BexVm,
    value: Value,
) -> Option<Result<TypeValue, Value>> {
    if class_name(vm, value).as_deref() != Some(PENDING_FQN) {
        return None;
    }
    match resolve_pending_if_ready(vm, value) {
        Ok(Some(value)) => Some(cloned_type_value(vm, value).ok_or_else(|| {
            alloc_compilation_error(
                vm,
                &[compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    "pending type resolved to a non-type value".into(),
                )],
            )
        })),
        Ok(None) => {
            let names = unresolved_builder_names(vm, value)
                .unwrap_or_else(|_| vec!["<unknown>".to_string()]);
            let names = names
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Some(Err(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "pending type references unresolved class builder {names}; call build() before using it as a runtime type"
                    ),
                )],
            )))
        }
        Err(message) => Some(Err(alloc_compilation_error(
            vm,
            &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
        ))),
    }
}
