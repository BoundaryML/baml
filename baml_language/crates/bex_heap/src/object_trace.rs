//! The VM `Object` enum as a [`Trace::Rust`](crate::Trace::Rust) cell: how its
//! pointers are found, how it is dropped and how its outside storage is
//! measured. This is the enum-specific tracing the copying collector used,
//! unchanged, behind the descriptor hooks.

use bex_vm_types::{
    HeapPtr, Meter, Object, Value,
    types::{Future, Objects, Slots},
};

/// A `HeapPtr` for an address found in the heap. The epoch is zero, as for
/// pointers read back out of a `Value`.
#[inline]
pub(crate) fn heap_ptr_at(address: usize) -> HeapPtr {
    let ptr = address as *mut Object;
    // SAFETY: only an address; nothing is dereferenced here.
    #[cfg(feature = "heap_debug")]
    unsafe {
        HeapPtr::from_ptr(ptr, 0)
    }
    // SAFETY: only an address; nothing is dereferenced here.
    #[cfg(not(feature = "heap_debug"))]
    unsafe {
        HeapPtr::from_ptr(ptr)
    }
}

/// Extract the inner `HeapPtr` held by a settled future (Ready / Error), if
/// the held value is an object reference. Pending / Cancelled / InternalError
/// futures hold no reachable outgoing references; this returns `None` for
/// those, and for settled futures whose payload is a non-object `Value`.
fn future_object_ref(fut: &Future) -> Option<HeapPtr> {
    use bex_vm_types::FutureRead;
    match fut.read() {
        FutureRead::Ready(v) | FutureRead::Error(v) => v.as_object_ptr(),
        FutureRead::Pending(_) | FutureRead::Cancelled | FutureRead::InternalError(_) => None,
    }
}

/// [`RustHooks::trace`](crate::RustHooks::trace) for `Object`.
///
/// # Safety
/// `payload` points to a live `Object`, and no mutator runs.
pub(crate) unsafe fn trace_object_hook(payload: *const u8, worklist: &mut Vec<HeapPtr>) {
    // SAFETY: forwarded to the caller.
    let object = unsafe { &*payload.cast::<Object>() };
    debug_assert!(
        !matches!(object, Object::Tombstone),
        "the collector reached a tombstone at {payload:?}: a stale pointer, or a \
         reservation dropped or held unpublished across a collection"
    );
    trace_object(object, worklist);
}

/// [`RustHooks::forward`](crate::RustHooks::forward) for `Object`.
///
/// # Safety
/// `payload` points to a live `Object`, and no mutator runs.
pub(crate) unsafe fn forward_object_hook(payload: *mut u8, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    // SAFETY: forwarded to the caller.
    let object = unsafe { &mut *payload.cast::<Object>() };
    debug_assert!(
        !matches!(object, Object::Tombstone),
        "the collector reached a tombstone at {payload:?}: a stale pointer, or a \
         reservation dropped or held unpublished across a collection"
    );
    forward_object(object, f);
}

/// [`RustHooks::drop`](crate::RustHooks::drop) for `Object`.
///
/// # Safety
/// `payload` points to a dead `Object` nothing will read again.
pub(crate) unsafe fn drop_object_hook(payload: *mut u8) {
    // SAFETY: forwarded to the caller.
    unsafe { std::ptr::drop_in_place(payload.cast::<Object>()) };
}

/// [`RustHooks::measure`](crate::RustHooks::measure) for `Object`.
///
/// # Safety
/// `payload` points to a live `Object` no mutator can reach.
pub(crate) unsafe fn measure_object_hook(payload: *mut u8, meter: &mut Meter) {
    // SAFETY: forwarded to the caller.
    unsafe { &mut *payload.cast::<Object>() }.measure(meter);
}

/// Add object references to the worklist for tracing. Null pointers may be
/// pushed; marking skips them.
pub(crate) fn trace_object(obj: &Object, worklist: &mut Vec<HeapPtr>) {
    trace_variant(obj, worklist);
    // Heads are ordinary references: a type naming a declaration keeps that
    // declaration alive. An unresolved head has no pointer to trace — emit
    // mints those, and the loader binds them before the VM exists.
    bex_vm_types::head_walk::visit_object_heads(obj, &mut |head| {
        if head.is_resolved() {
            worklist.push(head.ptr());
        }
    });
}

/// Push the heap pointer of every object-valued `Value`.
fn push_values(worklist: &mut Vec<HeapPtr>, values: impl IntoIterator<Item = Value>) {
    worklist.extend(values.into_iter().filter_map(|value| value.as_object_ptr()));
}

/// The edges of one object, its type heads aside.
fn trace_variant(obj: &Object, worklist: &mut Vec<HeapPtr>) {
    match obj {
        Object::Closure(closure) => {
            worklist.push(closure.function);
            push_values(worklist, closure.captures.iter().copied());
        }
        Object::BoundMethod(bm) => {
            worklist.push(bm.function);
            push_values(worklist, [bm.receiver]);
        }
        Object::Cell(cell) => push_values(worklist, [cell.load()]),
        Object::Variant(var) => worklist.push(var.enm),
        Object::Future(fut) => worklist.extend(future_object_ref(fut)),
        Object::Package(package) => trace_package(package, worklist),
        Object::Function(function) => trace_function(function, worklist),
        Object::GenericFunction(function) => worklist.push(function.runtime_package),
        // A declaration's own type edges are its heads, traced by
        // `trace_object` for every object kind; what these arms add is the
        // back-edge to the package that owns them, so reaching a member
        // keeps its package — and so its globals and dependencies — alive.
        // A class's inherent method bodies are direct pointers, like an
        // interface's default bodies.
        Object::Class(class) => {
            worklist.extend(class.owner.edges().iter().copied());
            worklist.extend(class.methods.values().map(|method| method.function_ptr));
        }
        Object::Enum(enm) => worklist.extend(enm.owner.edges().iter().copied()),
        // An impl rule points at its interface and at each method body. For
        // the static image those are compile-time pointers, but a rule a
        // runtime package or an anonymous-class witness allocated points at
        // collectible objects, so the edges are traced like any other.
        Object::ImplRule(rule) => {
            worklist.push(rule.interface_head);
            worklist.extend(rule.methods.values().map(|method| method.fqn));
        }
        // Runtime-declared interfaces/aliases back-reference their owning
        // package (null for static declarations), and an interface's
        // default-method bodies are direct pointers.
        Object::Interface(interface) => {
            worklist.push(interface.owner);
            worklist.extend(
                interface
                    .structural_default
                    .as_ref()
                    .map(|d| d.function_ptr),
            );
            worklist.extend(interface.methods.iter().map(|method| method.default_fn));
        }
        Object::TypeAlias(alias) => worklist.push(alias.owner),
        // `HostClosure` carries only an `Arc<HostValueArc>` (Rust-side stub,
        // not a heap object) and a `Box<RuntimeTy>` (no `HeapPtr`s). A `type`
        // value is exactly the type it denotes: every edge it has is a head.
        Object::Tombstone
        | Object::Shared(_)
        | Object::HostClosure(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::RustData(_)
        | Object::Type(_)
        | Object::Float(_) => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
}

fn trace_package(package: &bex_vm_types::types::Package, worklist: &mut Vec<HeapPtr>) {
    worklist.extend(package.classes.values().copied());
    worklist.extend(package.enums.values().copied());
    worklist.extend(package.interfaces.values().copied());
    worklist.extend(package.type_aliases.values().copied());
    worklist.extend(package.edges.values().map(|edge| edge.target));
    worklist.extend(package.init);
    worklist.extend(package.test_init);
    for (interface, rules) in &package.impl_rules {
        worklist.push(*interface);
        worklist.extend(rules.iter().copied());
    }
    if let Slots::Own { cells, .. } = &package.slots {
        push_values(worklist, cells.iter().map(|slot| slot.load()));
    }
    if let Objects::Own(objects) = &package.objects {
        worklist.extend(objects.iter().copied());
    }
}

/// A runtime function's owner and constants. A compile-time function (null
/// owner) holds only compile-time constants.
fn trace_function(function: &bex_vm_types::Function, worklist: &mut Vec<HeapPtr>) {
    if function.owner.as_ptr().is_null() {
        return;
    }
    worklist.push(function.owner);
    push_values(
        worklist,
        function.bytecode.resolved_constants.iter().copied(),
    );
}

/// Replace every heap pointer `obj` holds by what `f` returns for it, in
/// place: the pointers [`trace_object`] pushes, nulls aside, each visited
/// once. Identity never changes — a head keeps its tag, a map its order — so
/// nothing rehashes except a package's impl-rule table, keyed by pointer.
pub(crate) fn forward_object(obj: &mut Object, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    forward_variant(obj, f);
    // Heads are repointed as they were traced: only resolved ones.
    bex_vm_types::head_walk::visit_object_heads_mut(obj, &mut |head| {
        if head.is_resolved() {
            head.forward_to(f(head.ptr()));
        }
    });
}

/// Repoint one pointer, leaving a null alone.
fn forward_ptr(ptr: &mut HeapPtr, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    if !ptr.as_ptr().is_null() {
        *ptr = f(*ptr);
    }
}

/// Repoint an object-valued `Value`, leaving a scalar or null alone.
fn forward_value(value: &mut Value, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    if let Some(ptr) = value.as_object_ptr() {
        *value = Value::object(f(ptr));
    }
}

/// Repoint the value an atomic slot holds.
fn forward_slot(
    slot: &bex_vm_types::types::AtomicValueSlot,
    f: &mut dyn FnMut(HeapPtr) -> HeapPtr,
) {
    let mut value = slot.load();
    forward_value(&mut value, f);
    slot.store(value);
}

/// The edges of one object, its type heads aside: [`trace_variant`]'s, each
/// rewritten where it is stored.
fn forward_variant(obj: &mut Object, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    match obj {
        Object::Closure(closure) => {
            forward_ptr(&mut closure.function, f);
            for value in &mut closure.captures {
                forward_value(value, f);
            }
        }
        Object::BoundMethod(bm) => {
            forward_ptr(&mut bm.function, f);
            forward_value(&mut bm.receiver, f);
        }
        Object::Cell(cell) => forward_slot(&cell.value, f),
        Object::Variant(var) => forward_ptr(&mut var.enm, f),
        // The settled value is the only edge; see `future_object_ref`.
        // SAFETY: no mutator runs, so nothing settles the future meanwhile.
        Object::Future(fut) => {
            if let Some(value) = unsafe { fut.value_mut_for_fixup() } {
                forward_value(value, f);
            }
        }
        Object::Package(package) => forward_package(package, f),
        Object::Function(function) => forward_function(function, f),
        Object::GenericFunction(function) => forward_ptr(&mut function.runtime_package, f),
        Object::Class(class) => {
            for ptr in class.owner.edges_mut() {
                forward_ptr(ptr, f);
            }
            for method in class.methods.values_mut() {
                forward_ptr(&mut method.function_ptr, f);
            }
        }
        Object::Enum(enm) => {
            for ptr in enm.owner.edges_mut() {
                forward_ptr(ptr, f);
            }
        }
        Object::ImplRule(rule) => {
            forward_ptr(&mut rule.interface_head, f);
            for method in rule.methods.values_mut() {
                forward_ptr(&mut method.fqn, f);
            }
        }
        Object::Interface(interface) => {
            forward_ptr(&mut interface.owner, f);
            if let Some(default) = &mut interface.structural_default {
                forward_ptr(&mut default.function_ptr, f);
            }
            for method in &mut interface.methods {
                forward_ptr(&mut method.default_fn, f);
            }
        }
        Object::TypeAlias(alias) => forward_ptr(&mut alias.owner, f),
        Object::Tombstone
        | Object::Shared(_)
        | Object::HostClosure(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::RustData(_)
        | Object::Type(_)
        | Object::Float(_) => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
}

/// [`trace_package`]'s edges, rewritten. The impl-rule table is keyed by
/// interface pointer, so it is rebuilt in its own order.
fn forward_package(
    package: &mut bex_vm_types::types::Package,
    f: &mut dyn FnMut(HeapPtr) -> HeapPtr,
) {
    let declarations = package
        .classes
        .values_mut()
        .chain(package.enums.values_mut())
        .chain(package.interfaces.values_mut())
        .chain(package.type_aliases.values_mut())
        .chain(package.edges.values_mut().map(|edge| &mut edge.target))
        .chain(package.init.iter_mut())
        .chain(package.test_init.iter_mut());
    for ptr in declarations {
        forward_ptr(ptr, f);
    }
    let rules = std::mem::take(&mut package.impl_rules);
    package.impl_rules = rules
        .into_iter()
        .map(|(mut interface, mut rules)| {
            forward_ptr(&mut interface, f);
            for rule in &mut rules {
                forward_ptr(rule, f);
            }
            (interface, rules)
        })
        .collect();
    if let Slots::Own { cells, .. } = &package.slots {
        for cell in cells.iter() {
            forward_slot(cell, f);
        }
    }
    if let Objects::Own(objects) = &mut package.objects {
        for ptr in objects.iter_mut() {
            forward_ptr(ptr, f);
        }
    }
}

/// [`trace_function`]'s edges, rewritten: nothing for a compile-time
/// function.
fn forward_function(function: &mut bex_vm_types::Function, f: &mut dyn FnMut(HeapPtr) -> HeapPtr) {
    if function.owner.as_ptr().is_null() {
        return;
    }
    forward_ptr(&mut function.owner, f);
    for value in &mut function.bytecode.resolved_constants {
        forward_value(value, f);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use baml_type::Name;
    use bex_vm_types::{
        AtomicValueSlot, BoundMethod, Class, ClassMethodDef, Enum, Function, HeapPtr, Object,
        ObjectIndex, RealizedTy, TypeHead, Value,
        types::{
            Cell, Closure, Edge, EdgeKind, Future, FutureId, GenericFunction, InterfaceDef,
            InterfaceMethodDef, LocalName, MethodImpl, Objects, Owner, Package, RuntimeImplRule,
            Slots, StructuralDefault, TypeAliasDef, Variant,
        },
    };

    use super::{forward_object, heap_ptr_at, trace_object};

    /// A class declaring `fields`, with a fresh tag so instances get a layout.
    pub(crate) fn class_with_fields(fields: Vec<(&str, RealizedTy)>) -> Object {
        Object::Class(Box::new(Class {
            path: LocalName::new(Vec::new(), Name::new("T")),
            fields: fields
                .into_iter()
                .map(|(name, ty)| bex_vm_types::ClassField::new(name, ty))
                .collect(),
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            layout: std::sync::OnceLock::default(),
            redact: false,
        }))
    }

    /// Hands out a distinct fresh address for every pointer it is asked to
    /// forward, remembering what it was asked.
    #[derive(Default)]
    pub(crate) struct Forwarder {
        pub(crate) visited: Vec<HeapPtr>,
        pub(crate) moved: Vec<(HeapPtr, HeapPtr)>,
    }

    impl Forwarder {
        const BASE: usize = 0x7f00_0000_0000;

        pub(crate) fn forward(&mut self, old: HeapPtr) -> HeapPtr {
            assert!(!old.as_ptr().is_null(), "a null is never forwarded");
            self.visited.push(old);
            let new = heap_ptr_at(Self::BASE + 32 * (self.moved.len() + 1));
            self.moved.push((old, new));
            new
        }

        /// Where `old` was sent; `old` itself if it was never visited.
        pub(crate) fn image(&self, old: HeapPtr) -> HeapPtr {
            self.moved
                .iter()
                .find(|(from, _)| *from == old)
                .map_or(old, |(_, to)| *to)
        }
    }

    /// A pointer the tests never dereference: an aligned address only.
    fn p(k: usize) -> HeapPtr {
        heap_ptr_at(0x1_0000 * k)
    }

    fn non_null_edges(object: &Object) -> Vec<HeapPtr> {
        let mut edges = Vec::new();
        trace_object(object, &mut edges);
        edges.retain(|edge| !edge.as_ptr().is_null());
        edges
    }

    fn path(name: &str) -> LocalName {
        LocalName::new(Vec::new(), Name::new(name))
    }

    fn function(owner: HeapPtr, constants: Vec<Value>) -> Object {
        use bex_vm_types::{Bytecode, FunctionKind, types::FunctionOrigin};
        Object::Function(Box::new(Function {
            identity: bex_vm_types::types::FunctionIdentity::Declared(
                bex_vm_types::types::FnPath::Free(path("f")),
            ),
            source_file: "user.baml".to_string(),
            docstring: None,
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode {
                resolved_constants: constants,
                ..Bytecode::default()
            },
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
            compiled_body: None,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type: bex_vm_types::TyTemplate::Unknown,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: bex_vm_types::TyTemplate::Never,
            origin: FunctionOrigin::Internal,
            native_key: None,
            body_meta: None,
            trace_passthrough: false,
            owner,
        }))
    }

    fn settled_future(value: Value) -> Object {
        let future = Future::pending(
            FutureId::from_usize(0),
            RealizedTy::unknown(),
            RealizedTy::unknown(),
            bex_vm_types::types::CancellationToken::new(),
        );
        // SAFETY: the future is local; nothing else can settle it.
        assert!(unsafe { future.settle_ready(value) });
        Object::Future(future)
    }

    fn package() -> Object {
        let mut package = Package::empty(Name::new("p"));
        package.classes.insert(path("C"), p(10));
        package.enums.insert(path("E"), p(11));
        package.interfaces.insert(path("I"), p(12));
        package.type_aliases.insert(path("A"), p(13));
        package.edges.insert(
            Name::new("dep"),
            Edge {
                target: p(14),
                kind: EdgeKind::Dependency,
            },
        );
        package.init = Some(p(15));
        package.test_init = None;
        package.impl_rules.insert(p(16), vec![p(17), p(18)]);
        package.impl_rules.insert(p(19), vec![]);
        package.slots = Slots::Own {
            cells: Box::new([
                AtomicValueSlot::new(Value::object(p(20))),
                AtomicValueSlot::new(Value::int(5)),
            ]),
            initialized: false,
        };
        package.objects = Objects::Own(Box::new([p(21), HeapPtr::null()]));
        Object::Package(Box::new(package))
    }

    fn class_with_edges() -> Object {
        let Object::Class(mut class) = class_with_fields(vec![("a", RealizedTy::Int)]) else {
            unreachable!("class_with_fields builds a class")
        };
        class.owner = Owner::Package(p(30));
        class.methods.insert(
            Name::new("m"),
            ClassMethodDef {
                function: ObjectIndex::from_raw(0),
                function_ptr: p(31),
            },
        );
        // A field typed by a resolved head: an edge the head walk owns.
        class.fields[0].field_template = bex_vm_types::TyTemplate::class(
            TypeHead::new(p(32), baml_type::typetag::TypeTag::fresh_dynamic()),
            Box::new([]),
        );
        Object::Class(class)
    }

    fn anonymous_class_with_witnesses() -> Object {
        let Object::Class(mut class) = class_with_fields(vec![]) else {
            unreachable!("class_with_fields builds a class")
        };
        class.owner.set_witnesses(Box::new([p(33), p(34)]));
        class.methods.insert(
            Name::new("unbound"),
            ClassMethodDef {
                function: ObjectIndex::from_raw(0),
                function_ptr: HeapPtr::null(),
            },
        );
        Object::Class(class)
    }

    fn enum_owned_by(owner: Owner) -> Object {
        Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
            path: path("E"),
            variants: vec![],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner,
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        }))
    }

    fn impl_rule() -> Object {
        let mut methods = indexmap::IndexMap::new();
        methods.insert(
            Name::new("m"),
            MethodImpl {
                fqn: p(40),
                frame: vec![],
            },
        );
        Object::ImplRule(Box::new(RuntimeImplRule {
            interface_head: p(41),
            for_ty_pattern: bex_vm_types::TyTemplate::Unknown,
            generic_param_bounds: vec![],
            interface_args: vec![],
            interface_assoc: vec![],
            methods,
            field_links: Box::new([]),
        }))
    }

    fn interface() -> Object {
        let method = |default_fn| InterfaceMethodDef {
            name: Name::new("m"),
            args: vec![],
            kwargs: vec![],
            returns: bex_vm_types::RuntimeTy::unknown(),
            errors: bex_vm_types::RuntimeTy::unknown(),
            default: None,
            default_fn,
        };
        Object::Interface(Box::new(InterfaceDef {
            path: path("I"),
            structural_default: Some(StructuralDefault {
                kind: baml_type::StructuralInterface::ToString,
                function: ObjectIndex::from_raw(0),
                function_ptr: p(50),
            }),
            type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
            args: vec![],
            requires: vec![],
            assoc: vec![],
            fields: vec![],
            methods: vec![method(p(51)), method(HeapPtr::null())],
            owner: p(52),
        }))
    }

    /// One object of every variant that carries pointers, and a few that do
    /// not, with the number of edges each is expected to have.
    fn every_variant() -> Vec<(Object, usize)> {
        vec![
            (
                Object::Closure(Closure {
                    function: p(1),
                    captures: Box::new([Value::object(p(2)), Value::int(7)]),
                    captured_type_args: Box::new([]),
                }),
                2,
            ),
            (
                Object::BoundMethod(BoundMethod {
                    function: p(3),
                    receiver: Value::object(p(4)),
                    type_args: Box::new([]),
                }),
                2,
            ),
            (Object::Cell(Cell::new(Value::object(p(5)))), 1),
            (Object::Cell(Cell::new(Value::int(3))), 0),
            (
                Object::Variant(Variant {
                    enm: p(6),
                    index: 0,
                }),
                1,
            ),
            (settled_future(Value::object(p(7))), 1),
            (settled_future(Value::int(9)), 0),
            (
                Object::Future(Future::pending(
                    FutureId::from_usize(1),
                    RealizedTy::unknown(),
                    RealizedTy::unknown(),
                    bex_vm_types::types::CancellationToken::new(),
                )),
                0,
            ),
            (package(), 12),
            (
                function(p(60), vec![Value::object(p(61)), Value::int(2)]),
                2,
            ),
            // A compile-time function keeps its constants as they are.
            (function(HeapPtr::null(), vec![Value::object(p(62))]), 0),
            (
                Object::GenericFunction(GenericFunction {
                    function: bex_vm_types::GlobalIndex::from_raw(0),
                    type_args: Box::new([]),
                    runtime_package: p(63),
                }),
                1,
            ),
            (class_with_edges(), 3),
            (anonymous_class_with_witnesses(), 2),
            (enum_owned_by(Owner::Package(p(70))), 1),
            (enum_owned_by(Owner::anonymous()), 0),
            (impl_rule(), 2),
            (interface(), 3),
            (
                Object::TypeAlias(Box::new(TypeAliasDef {
                    path: path("A"),
                    type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
                    definition: RealizedTy::string(),
                    owner: p(80),
                })),
                1,
            ),
            (Object::String("leaf".into()), 0),
            (Object::Float(1.5), 0),
        ]
    }

    /// `forward` visits exactly the pointers `trace` yields, rewrites each
    /// to what the closure returned, and leaves everything else alone.
    #[test]
    fn forward_rewrites_exactly_what_trace_yields_for_every_variant() {
        for (mut object, edges) in every_variant() {
            let kind = bex_vm_types::ObjectType::of(&object);
            let before = non_null_edges(&object);
            assert_eq!(before.len(), edges, "{kind:?}: edges traced");

            let mut forwarder = Forwarder::default();
            forward_object(&mut object, &mut |old| forwarder.forward(old));
            assert_eq!(
                forwarder.visited, before,
                "{kind:?}: forward visits what trace yields"
            );

            let expected: Vec<HeapPtr> = before.iter().map(|&old| forwarder.image(old)).collect();
            assert_eq!(
                non_null_edges(&object),
                expected,
                "{kind:?}: every pointer moved"
            );
            assert!(
                non_null_edges(&object)
                    .iter()
                    .all(|edge| !before.contains(edge)),
                "{kind:?}: no old pointer survives"
            );
        }
    }

    /// The scalars beside the pointers are untouched by a forward.
    #[test]
    fn forward_leaves_scalars_and_structure_alone() {
        let mut forwarder = Forwarder::default();
        let mut f = |old| forwarder.forward(old);

        let mut closure = Object::Closure(Closure {
            function: p(1),
            captures: Box::new([Value::object(p(2)), Value::int(7)]),
            captured_type_args: Box::new([]),
        });
        forward_object(&mut closure, &mut f);
        let Object::Closure(closure) = &closure else {
            unreachable!()
        };
        assert_eq!(closure.captures[1], Value::int(7));

        let mut package = package();
        forward_object(&mut package, &mut f);
        let Object::Package(package) = &package else {
            unreachable!()
        };
        let Slots::Own { cells, .. } = &package.slots else {
            unreachable!()
        };
        assert_eq!(cells[1].load(), Value::int(5));
        assert_eq!(
            package
                .impl_rules
                .values()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            [2, 0],
            "the rule table keeps its order and shape"
        );
        assert_eq!(package.objects.own().unwrap()[1], HeapPtr::null());
        assert_eq!(package.test_init, None);

        let mut function = function(HeapPtr::null(), vec![Value::object(p(62))]);
        forward_object(&mut function, &mut f);
        let Object::Function(function) = &function else {
            unreachable!()
        };
        assert_eq!(
            function.bytecode.resolved_constants[0],
            Value::object(p(62))
        );

        let mut interface = interface();
        forward_object(&mut interface, &mut f);
        let Object::Interface(interface) = &interface else {
            unreachable!()
        };
        assert_eq!(interface.methods[1].default_fn, HeapPtr::null());

        let mut class = class_with_edges();
        let tag = {
            let Object::Class(class) = &class else {
                unreachable!()
            };
            class.type_tag
        };
        forward_object(&mut class, &mut f);
        let Object::Class(class) = &class else {
            unreachable!()
        };
        assert_eq!(class.type_tag, tag);
        let bex_vm_types::TyTemplate::Class(head, _) = &class.fields[0].field_template else {
            unreachable!()
        };
        assert_eq!(head.ptr(), forwarder.image(p(32)), "a head is repointed");
    }
}
