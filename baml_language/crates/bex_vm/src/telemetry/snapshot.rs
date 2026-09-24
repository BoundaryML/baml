//! Heap traversal stays in the VM. Frozen graphs contain only owned leaves and
//! snapshot-local indexes. Locks sample containers independently, not atomically.
use bex_vm_types::{HeapPtr, Object, Value, ValueKind};
use btel_snapshot::{
    Builder, Description, Limit, ObjectId, Snapshot, SnapshotObject, SnapshotValue,
};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy)]
pub(super) enum Input<'a> {
    Value(Value),
    FunctionArgs(&'a [Value]),
}
#[derive(Default)]
pub(super) struct Scratch {
    // Heap addresses are construction-only. Leaf entries reuse storage, not identity.
    seen: FxHashMap<HeapPtr, SnapshotValue>,
    rust_seen: FxHashMap<usize, ObjectId>,
    work: Vec<(ObjectId, HeapPtr, usize)>,
}
impl Scratch {
    fn add(&mut self, b: &mut Builder, value: Value, depth: usize) -> SnapshotValue {
        if let Some(ptr) = value.as_object_ptr() {
            if let Some(value) = self.seen.get(&ptr) {
                return *value;
            }
        }
        if b.limits().max_depth.is_some_and(|max| depth > max) {
            return b.limited(Limit::Depth);
        }
        let ptr = match value.kind() {
            ValueKind::Null => return SnapshotValue::Null,
            ValueKind::OmittedArg => return SnapshotValue::OmittedArg,
            ValueKind::Int(v) => return SnapshotValue::Int(v),
            ValueKind::Bool(v) => return SnapshotValue::Bool(v),
            ValueKind::Object(ptr) => ptr,
        };
        // SAFETY: capture holds the heap permit and never allocates in the VM heap.
        let result = match unsafe { ptr.get() } {
            // Boxes are an execution detail, not snapshot identity. No work item
            // or memo-table entry for floats, even when every source box differs.
            Object::Float(v) => return SnapshotValue::Float(*v),
            Object::String(s) => b.string(s).map_or(
                SnapshotValue::Truncated(Limit::Bytes),
                SnapshotValue::String,
            ),
            Object::Bigint(n) => b.bigint(n).map_or(
                SnapshotValue::Truncated(Limit::Bytes),
                SnapshotValue::Bigint,
            ),
            Object::Type(value) => SnapshotValue::Type(b.push_type(owned_type(&value.ty))),
            Object::Variant(v) => {
                let declaration = match self.add(b, Value::object(v.enm), depth + 1) {
                    SnapshotValue::Object(id) => id,
                    SnapshotValue::Truncated(reason) => return b.limited(reason),
                    _ => unreachable!("enum declaration is an object"),
                };
                // SAFETY: enum declaration lives under the same heap permit.
                let Object::Enum(enm) = (unsafe { v.enm.get() }) else {
                    unreachable!("variant enum")
                };
                let name = b.string(&enm.variants[v.index].name.as_str().into());
                match name {
                    Some(name) => SnapshotValue::Enum {
                        declaration,
                        variant: u32::try_from(v.index).expect("enum variant index"),
                        name,
                    },
                    None => SnapshotValue::Truncated(Limit::Bytes),
                }
            }
            Object::RustData(data) => {
                let identity = std::sync::Arc::as_ptr(data).cast::<()>() as usize;
                let id = if let Some(id) = self.rust_seen.get(&identity) {
                    *id
                } else {
                    let Some(id) = b.reserve_object() else {
                        return b.limited(Limit::Objects);
                    };
                    self.rust_seen.insert(identity, id);
                    b.set_object(id, SnapshotObject::NonSnapshotableValue {});
                    id
                };
                SnapshotValue::Object(id)
            }
            _ => {
                let Some(id) = b.reserve_object() else {
                    return b.limited(Limit::Objects);
                };
                // Destination identity exists before children are visited.
                self.work.push((id, ptr, depth));
                SnapshotValue::Object(id)
            }
        };
        self.seen.insert(ptr, result);
        result
    }
    /// SAFETY: source values are live under the caller's heap permit throughout.
    /// No collection, VM allocation, user code, future polling or I/O occurs here.
    pub(super) unsafe fn capture(&mut self, mut b: Builder, input: Input<'_>) -> Snapshot {
        self.seen.clear();
        self.rust_seen.clear();
        self.work.clear();
        let mut value_root = SnapshotValue::Null;
        let args_start = b.value_start();
        if let Input::FunctionArgs(args) = input {
            let count = args.len().min(b.remaining_values());
            b.reserve_values(count);
            for value in args.iter().take(count) {
                let value = self.add(&mut b, *value, 0);
                b.push_value(value);
            }
            if count < args.len() {
                b.limited(Limit::Values);
            }
        } else if let Input::Value(value) = input {
            value_root = self.add(&mut b, value, 0);
        }
        let args_range = b.value_range(args_start);
        while let Some((id, ptr, depth)) = self.work.pop() {
            // SAFETY: inherited heap permit, never relinquished during traversal.
            let object = match unsafe { ptr.get() } {
                Object::Uint8Array(data) => {
                    let data = data.lock();
                    SnapshotObject::Bytes {
                        data: b.copy_bytes(&data),
                        original_len: data.len(),
                    }
                }
                Object::Array(data) => {
                    let element_type = b.push_type(owned_type(&data.element_ty));
                    let data = data.lock();
                    let start = b.value_start();
                    let count = data.len().min(b.remaining_values());
                    b.reserve_values(count);
                    for value in data.iter().take(count) {
                        let value = self.add(&mut b, *value, depth + 1);
                        b.push_value(value);
                    }
                    if count < data.len() {
                        b.limited(Limit::Values);
                    }
                    SnapshotObject::List {
                        element_type,
                        items: b.value_range(start),
                        original_len: data.len(),
                    }
                }
                Object::Map(data) => {
                    let key_type = b.push_type(owned_type(&data.key_ty));
                    let value_type = b.push_type(owned_type(&data.value_ty));
                    let data = data.lock();
                    let start = b.entry_start();
                    let count = data.len().min(b.remaining_entries());
                    b.reserve_entries(count);
                    for (key, value) in data.iter().take(count) {
                        if !b.content(key.len(), !matches!(key, bex_str::BexStr::Inline { .. })) {
                            break;
                        }
                        let value = self.add(&mut b, *value, depth + 1);
                        b.entry(key, value);
                    }
                    let entries = b.entry_range(start);
                    if entries.len() < data.len() {
                        b.limited(Limit::Values);
                    }
                    SnapshotObject::Map {
                        key_type,
                        value_type,
                        entries,
                        original_len: data.len(),
                    }
                }
                Object::Instance(instance) => {
                    let types_start = b.type_start();
                    for ty in &instance.class_type_args {
                        b.push_type(owned_type(ty));
                    }
                    let type_arguments = b.type_range(types_start);
                    let declaration =
                        match self.add(&mut b, Value::object(instance.class), depth + 1) {
                            SnapshotValue::Object(id) => id,
                            SnapshotValue::Truncated(reason) => {
                                b.set_object(id, SnapshotObject::Truncated(reason));
                                continue;
                            }
                            _ => unreachable!("class declaration is an object"),
                        };
                    // SAFETY: class lives under the same heap permit.
                    let Object::Class(class) = (unsafe { instance.class.get() }) else {
                        unreachable!("instance class")
                    };
                    let start = b.entry_start();
                    let count = instance.field_len().min(b.remaining_entries());
                    b.reserve_entries(count);
                    for (i, field) in class.fields.iter().enumerate().take(count) {
                        if !b.content(field.name.len(), false) {
                            break;
                        }
                        let value = self.add(&mut b, instance.load_field(i), depth + 1);
                        b.entry(&field.name.as_str().into(), value);
                    }
                    let fields = b.entry_range(start);
                    if fields.len() < instance.field_len() {
                        b.limited(Limit::Values);
                    }
                    SnapshotObject::Instance {
                        type_arguments,
                        declaration,
                        fields,
                        original_len: instance.field_len(),
                    }
                }
                Object::Class(class) => declaration(&mut b, &class.name, class.type_tag, false),
                Object::Enum(enm) => declaration(&mut b, &enm.name, enm.type_tag, true),
                Object::Cell(cell) => {
                    SnapshotObject::Cell(self.add(&mut b, cell.load(), depth + 1))
                }
                Object::Function(f) => SnapshotObject::Descriptive {
                    kind: Description::Function,
                    name: b.string(&f.name.as_str().into()),
                },
                Object::Closure(_) => describe(Description::Closure),
                Object::BoundMethod(_) => describe(Description::BoundMethod),
                Object::GenericFunction(_) => describe(Description::GenericFunction),
                Object::HostClosure(_) => describe(Description::HostFunction),
                Object::Future(_) => describe(Description::Future),
                Object::UnscheduledFuture(_) => describe(Description::UnscheduledFuture),
                Object::Package(_) => describe(Description::Package),
                Object::Interface(_) => describe(Description::Interface),
                Object::ImplRule(_) => describe(Description::Implementation),
                Object::TypeAlias(_) => describe(Description::TypeAlias),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => describe(Description::Sentinel),
                Object::Float(_)
                | Object::String(_)
                | Object::Bigint(_)
                | Object::Type(_)
                | Object::Variant(_)
                | Object::RustData(_) => unreachable!("inline value queued as object"),
            };
            b.set_object(id, object);
        }
        self.seen.clear();
        self.rust_seen.clear();
        self.work.clear();
        match input {
            Input::Value(_) => b.finish_value(value_root),
            Input::FunctionArgs(args) => b.finish_args(args.len(), args_range),
        }
    }
}
// Preserve immutable type metadata; replace every VM type head with owned identity.
fn owned_type(ty: &bex_vm_types::RealizedTy) -> btel_snapshot::OwnedType {
    ty.map_heads(&mut |head| match head.tagged_name() {
        Some(name) => btel_snapshot::TypeIdentity::Resolved(name),
        None => btel_snapshot::TypeIdentity::Unresolved(head.tag()),
    })
}
fn declaration(
    b: &mut Builder,
    name: &baml_type::DeclarationName,
    tag: baml_type::typetag::TypeTag,
    is_enum: bool,
) -> SnapshotObject {
    let mut bytes = name.item_name().len();
    if let Some(q) = name.declared() {
        bytes += q.package().len();
        bytes += q
            .namespace()
            .iter()
            .map(baml_type::Name::len)
            .sum::<usize>();
    }
    if b.content(bytes, false) {
        SnapshotObject::Declaration {
            name: name.clone(),
            tag,
            is_enum,
        }
    } else {
        SnapshotObject::Truncated(Limit::Bytes)
    }
}
fn describe(kind: Description) -> SnapshotObject {
    SnapshotObject::Descriptive { kind, name: None }
}
#[cfg(test)]
mod tests {
    use btel_snapshot::{SnapshotObject as Obj, SnapshotValue as Val};

    use super::*;
    fn captured_object(snapshot: &Snapshot, value: Val) -> &Obj {
        let Val::Object(id) = value else {
            panic!("expected object, got {value:?}")
        };
        snapshot.object(id)
    }
    use std::sync::Arc;

    use bex_vm_types::RealizedTy;
    fn ty() -> RealizedTy {
        RealizedTy::Null
    }
    fn capture(values: &[Value]) -> Snapshot {
        let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
        // SAFETY: all test values remain live; fixtures do not collect during capture.
        unsafe {
            Scratch::default().capture(pool.try_acquire().unwrap(), Input::FunctionArgs(values))
        }
    }
    #[test]
    fn mutable_graph_cycles_aliases_and_bytes_survive_mutation_and_heap_teardown() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let bytes = vm.tlab.alloc_uint8array(vec![1, 2, 3]);
        let list = vm
            .tlab
            .alloc_array(ty(), vec![Value::object(bytes), Value::object(bytes)]);
        // SAFETY: fixture holds the live heap with no collection.
        let Object::Array(array) = (unsafe { list.get() }) else {
            unreachable!()
        };
        array.lock_mut().push(Value::object(list));
        let snapshot = capture(&[Value::object(list)]);
        array.lock_mut().clear();
        let Object::Uint8Array(data) = (unsafe { bytes.get() }) else {
            unreachable!()
        };
        data.lock_mut().fill(7);
        // Captures are independent: the graph does not contribute any GC roots.
        unsafe {
            vm.heap
                .collect_garbage_generational(&[], bex_heap::CollectionLevel::Major)
        };
        drop(vm);
        let root = snapshot.roots()[0];
        let Obj::List {
            items,
            original_len,
            ..
        } = captured_object(&snapshot, root)
        else {
            panic!()
        };
        assert_eq!(*original_len, 3);
        let children = snapshot.values(*items);
        assert_eq!(children[0], children[1]);
        assert_eq!(children[2], root);
        let Obj::Bytes { data, .. } = captured_object(&snapshot, children[0]) else {
            panic!()
        };
        assert_eq!(snapshot.bytes(*data), [1, 2, 3]);
    }
    #[test]
    fn scalar_kinds_strings_bigints_and_non_snapshotable_rust_objects() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let string = vm.tlab.alloc_string(bex_str::BexStr::concat(
            "a".repeat(1024).into(),
            "b".repeat(1024).into(),
        ));
        let big = vm.tlab.alloc_bigint(num_bigint::BigInt::from(123));
        let float = vm.tlab.alloc_float(2.5);
        let opaque: Arc<dyn std::any::Any + Send + Sync> = baml_builtins2::MediaValue::from_file(
            baml_type::MediaKind::Image,
            "/nonexistent/telemetry-must-not-read.png",
            None,
        );
        let weak = Arc::downgrade(&opaque);
        let first = vm.tlab.alloc_rust_data(opaque.clone());
        let second = vm.tlab.alloc_rust_data(opaque.clone());
        drop(opaque);
        let snapshot = capture(&[
            Value::NULL,
            Value::OMITTED_ARG,
            Value::TRUE,
            Value::int(-3),
            Value::object(string),
            Value::object(big),
            Value::object(float),
            Value::object(first),
            Value::object(second),
        ]);
        unsafe {
            vm.heap
                .collect_garbage_generational(&[], bex_heap::CollectionLevel::Major)
        };
        drop(vm);
        assert!(
            weak.upgrade().is_none(),
            "opaque source must not be retained"
        );
        let roots = snapshot.roots();
        assert!(matches!(roots[0], Val::Null));
        assert!(matches!(roots[1], Val::OmittedArg));
        assert!(matches!(roots[2], Val::Bool(true)));
        assert!(matches!(roots[3], Val::Int(-3)));
        let Val::String(string) = roots[4] else {
            panic!()
        };
        assert!(
            matches!(snapshot.string(string), bex_str::BexStr::Concat(_)),
            "capture retains the shared concat handle after content hashing"
        );
        assert!(matches!(roots[5], Val::Bigint(_)));
        assert!(matches!(roots[6], Val::Float(2.5)));
        assert_eq!(roots[7], roots[8]);
        assert!(matches!(
            captured_object(&snapshot, roots[8]),
            Obj::NonSnapshotableValue {}
        ));
    }
    #[test]
    fn concurrent_container_mutation_and_depth_limits_terminate() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let ptr = vm.tlab.alloc_array(ty(), vec![Value::int(1); 8]);
        // SAFETY: the fixture keeps the heap live and does not collect during capture.
        let Object::Array(array) = (unsafe { ptr.get() }) else {
            unreachable!()
        };
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                for i in 0..500 {
                    let mut a = array.lock_mut();
                    a.clear();
                    a.resize(8, Value::int(i));
                }
            });
            for _ in 0..50 {
                let s = capture(&[Value::object(ptr)]);
                let Obj::List { items, .. } = captured_object(&s, s.roots()[0]) else {
                    panic!()
                };
                assert_eq!(s.values(*items).len(), 8);
            }
            writer.join().unwrap();
        });
        let pool = btel_snapshot::SnapshotPool::new(
            1,
            btel_snapshot::Limits {
                max_depth: Some(0),
                ..Default::default()
            },
        );
        let snapshot = unsafe {
            Scratch::default().capture(
                pool.try_acquire().unwrap(),
                Input::FunctionArgs(&[Value::object(ptr)]),
            )
        };
        assert!(snapshot.stats().limited);
    }
    #[test]
    fn maps_instances_and_variants_keep_identity_and_fields_after_gc() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            name: bex_vm_types::DeclarationName::Declared(baml_type::TypeName::local(
                baml_type::Name::new("TestClass"),
            )),
            fields: vec![
                bex_vm_types::ClassField {
                    name: "x".to_string(),
                    field_type: baml_type::RuntimeTy::Int,
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                    must_exist: false,
                    stream_done: false,
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::default(),
                    skip: false,
                    runtime_type: None,
                },
                bex_vm_types::ClassField {
                    name: "y".to_string(),
                    field_type: baml_type::RuntimeTy::Int,
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                    must_exist: false,
                    stream_done: false,
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::default(),
                    skip: false,
                    runtime_type: None,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::default(),
            type_tag: baml_type::typetag::TypeTag::from_i64(100),
            stream_done: false,
            has_cleanup: false,
            generic_param_count: 1,
            owner: bex_vm_types::HeapPtr::null(),
        })));
        let enum_ptr = vm.tlab.alloc(Object::Enum(Box::new(bex_vm_types::Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            name: bex_vm_types::DeclarationName::Declared(baml_type::TypeName::local(
                baml_type::Name::new("Color"),
            )),
            variants: vec![
                bex_vm_types::EnumVariant {
                    name: "Red".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::default(),
                    skip: false,
                },
                bex_vm_types::EnumVariant {
                    name: "Green".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::default(),
                    skip: false,
                },
                bex_vm_types::EnumVariant {
                    name: "Blue".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::default(),
                    skip: false,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::default(),
            owner: bex_vm_types::HeapPtr::null(),
        })));
        let instance = vm.tlab.alloc_instance_with_type_args(
            class_ptr,
            Box::new([RealizedTy::Class(
                bex_vm_types::TypeHead::new(class_ptr, baml_type::typetag::TypeTag::from_i64(100)),
                Box::new([]),
            )]),
            vec![Value::int(10), Value::int(20)],
        );
        let variant = vm.tlab.alloc_variant(enum_ptr, 1);
        let map = vm.tlab.alloc_map(
            ty(),
            ty(),
            indexmap::IndexMap::from([
                (bex_str::BexStr::from("instance"), Value::object(instance)),
                (bex_str::BexStr::from("enum"), Value::object(variant)),
            ]),
        );
        let snapshot = capture(&[Value::object(map)]);
        unsafe {
            let Object::Instance(instance) = instance.get() else {
                unreachable!()
            };
            instance.store_field(0, Value::int(99));
            let Object::Map(map) = map.get() else {
                unreachable!()
            };
            map.lock_mut().clear();
            vm.heap
                .collect_garbage_generational(&[], bex_heap::CollectionLevel::Major);
        }
        drop(vm);
        let Obj::Map { entries, .. } = captured_object(&snapshot, snapshot.roots()[0]) else {
            panic!()
        };
        let entries = snapshot.entries(*entries);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key.as_str(), "instance");
        let Obj::Instance {
            declaration,
            fields,
            type_arguments,
            ..
        } = captured_object(&snapshot, entries[0].value)
        else {
            panic!()
        };
        let baml_type::RealizedTy::Class(head, ..) = &snapshot.type_arguments(*type_arguments)[0]
        else {
            panic!("lost class type argument");
        };
        assert!(matches!(head, btel_snapshot::TypeIdentity::Resolved(_)));
        let Obj::Declaration {
            name,
            tag,
            is_enum: false,
        } = snapshot.object(*declaration)
        else {
            panic!()
        };
        assert_eq!(name.item_name().as_str(), "TestClass");
        assert_eq!(*tag, baml_type::typetag::TypeTag::from_i64(100));
        let fields = snapshot.entries(*fields);
        assert_eq!(fields[0].key.as_str(), "x");
        assert!(matches!(fields[0].value, Val::Int(10)));
        let Val::Enum {
            declaration,
            variant: index,
            name,
        } = entries[1].value
        else {
            panic!()
        };
        assert_eq!(index, 1);
        assert_eq!(snapshot.string(name).as_str(), "Green");
        assert!(matches!(
            snapshot.object(declaration),
            Obj::Declaration { is_enum: true, .. }
        ));
    }
    #[test]
    fn function_args_preserve_omission_null_and_aliases_separately_from_value_roots() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let list = vm.tlab.alloc_array(ty(), vec![Value::int(4)]);
        let args = [
            Value::object(list),
            Value::OMITTED_ARG,
            Value::NULL,
            Value::object(list),
        ];
        let snapshot = capture(&args);
        let btel_snapshot::SnapshotRoot::FunctionArgs(root) = snapshot.root() else {
            panic!()
        };
        assert_eq!(root.parameter_count, 4);
        let slots = snapshot.arguments().unwrap();
        assert_eq!(slots[0], slots[3]);
        assert!(matches!(slots[1], Val::OmittedArg));
        assert!(matches!(slots[2], Val::Null));
        let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
        let result = unsafe {
            Scratch::default().capture(pool.try_acquire().unwrap(), Input::Value(Value::NULL))
        };
        assert!(result.arguments().is_none());
        assert!(matches!(result.value().unwrap(), Val::Null));
    }

    #[test]
    fn capture_matches_deep_copy_for_mutable_data_graph() {
        use crate::package_baml::{BamlPackageBaml, PackageBamlImpl};
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let bytes = vm.tlab.alloc_uint8array(vec![1, 2, 3]);
        let list = vm
            .tlab
            .alloc_array(ty(), vec![Value::object(bytes), Value::object(bytes)]);
        let Object::Array(array) = (unsafe { list.get() }) else {
            unreachable!()
        };
        array.lock_mut().push(Value::object(list));
        let map = vm.tlab.alloc_map(
            ty(),
            ty(),
            indexmap::IndexMap::from([
                (bex_str::BexStr::from("graph"), Value::object(list)),
                (bex_str::BexStr::from("alias"), Value::object(bytes)),
            ]),
        );
        let original = Value::object(map);
        let heap_copy = PackageBamlImpl::deep_copy(&mut vm, &original).unwrap();
        let snapshot = capture(&[original]);
        let copied_snapshot = capture(&[heap_copy]);
        // Both walks must preserve the same graph topology and values despite
        // distinct VM heap addresses; no source addresses appear in either graph.
        assert_eq!(format!("{snapshot:?}"), format!("{copied_snapshot:?}"));
        assert_eq!(snapshot.id(), copied_snapshot.id());
    }

    #[test]
    fn large_and_deep_captures_grow_beyond_ordinary_reuse_sizes() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let bytes = vm.tlab.alloc_uint8array(vec![42; 9 * 1024 * 1024]);
        let mut value = Value::object(bytes);
        for _ in 0..256 {
            value = Value::object(vm.tlab.alloc_array(ty(), vec![value]));
        }
        let snapshot = capture(&[value]);
        assert!(!snapshot.stats().limited);
        let mut node = snapshot.roots()[0];
        for _ in 0..256 {
            let Obj::List { items, .. } = captured_object(&snapshot, node) else {
                panic!()
            };
            node = snapshot.values(*items)[0];
        }
        let Obj::Bytes { data, original_len } = captured_object(&snapshot, node) else {
            panic!()
        };
        assert_eq!(*original_len, 9 * 1024 * 1024);
        assert_eq!(snapshot.bytes(*data).len(), *original_len);
        assert!(snapshot.bytes(*data).iter().all(|byte| *byte == 42));
    }

    #[test]
    fn type_values_and_container_types_survive_heap_teardown() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let list = vm
            .tlab
            .alloc_array(bex_vm_types::RealizedTy::string(), Vec::new());
        let type_value =
            vm.tlab
                .alloc(Object::Type(Box::new(bex_vm_types::types::TypeValue::new(
                    bex_vm_types::RealizedTy::list(bex_vm_types::RealizedTy::string()),
                ))));
        let snapshot = capture(&[Value::object(list), Value::object(type_value)]);
        drop(vm);
        let Obj::List { element_type, .. } = captured_object(&snapshot, snapshot.roots()[0]) else {
            panic!()
        };
        assert_eq!(
            snapshot.ty(*element_type),
            &btel_snapshot::OwnedType::string()
        );
        let Val::Type(ty) = snapshot.roots()[1] else {
            panic!()
        };
        assert_eq!(
            snapshot.ty(ty),
            &btel_snapshot::OwnedType::list(btel_snapshot::OwnedType::string())
        );
    }
    #[test]
    fn primitive_containers_have_inline_values_and_bounded_object_scratch() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let ints = vm
            .tlab
            .alloc_array(RealizedTy::int(), (0..10_000).map(Value::int).collect());
        let float_values = (0..10_000)
            .map(|i| Value::object(vm.tlab.alloc_float(f64::from(i))))
            .collect();
        let floats = vm.tlab.alloc_array(ty(), float_values);
        let map = vm.tlab.alloc_map(
            RealizedTy::string(),
            RealizedTy::int(),
            (0..10_000)
                .map(|i| (bex_str::BexStr::from(i.to_string()), Value::int(i)))
                .collect(),
        );
        let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
        let mut scratch = Scratch::default();
        for source in [ints, floats, map] {
            let snapshot = unsafe {
                scratch.capture(
                    pool.try_acquire().unwrap(),
                    Input::Value(Value::object(source)),
                )
            };
            assert_eq!(
                snapshot.object_count(),
                1,
                "primitive boxes must not become objects"
            );
            assert!(
                scratch.work.capacity() <= 4,
                "primitive elements must not enter work stack"
            );
            assert!(
                scratch.seen.capacity() <= 4,
                "floats must not populate identity map"
            );
            match captured_object(&snapshot, snapshot.value().unwrap()) {
                Obj::List { items, .. } => {
                    let values = snapshot.values(*items);
                    assert_eq!(values.len(), 10_000);
                    assert_eq!(std::mem::size_of_val(values), 160_000);
                    assert!(matches!(values[9999], Val::Int(9999) | Val::Float(9999.0)));
                    assert!(snapshot.live_arena_bytes() < 161_000);
                }
                Obj::Map { entries, .. } => {
                    let entries = snapshot.entries(*entries);
                    assert_eq!(entries.len(), 10_000);
                    assert_eq!(std::mem::size_of_val(entries), 720_000);
                    assert_eq!(entries[9999].key.as_str(), "9999");
                    assert_eq!(entries[9999].value, Val::Int(9999));
                }
                _ => panic!(),
            }
        }
    }
    #[test]
    fn distinct_objects_stay_distinct_and_repeated_leaves_only_share_storage() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let a = vm.tlab.alloc_array(ty(), vec![Value::int(7)]);
        let b = vm.tlab.alloc_array(ty(), vec![Value::int(7)]);
        let string = vm.tlab.alloc_string("shared immutable content");
        let snapshot = capture(&[
            Value::object(a),
            Value::object(b),
            Value::object(a),
            Value::object(string),
            Value::object(string),
        ]);
        let values = snapshot.roots();
        assert_ne!(values[0], values[1]);
        assert_eq!(values[0], values[2]);
        assert_eq!(values[3], values[4]);
        assert_eq!(snapshot.object_count(), 2);
        let Val::String(id) = values[3] else { panic!() };
        drop(vm);
        assert_eq!(snapshot.string(id).as_str(), "shared immutable content");
    }
    #[test]
    fn cell_cycles_and_distinct_opaque_objects_keep_identity_without_retaining_sources() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let cell = vm
            .tlab
            .alloc(Object::Cell(bex_vm_types::types::Cell::new(Value::NULL)));
        let Object::Cell(source) = (unsafe { cell.get() }) else {
            panic!()
        };
        source.store(Value::object(cell));
        let a = vm.tlab.alloc_rust_data(Arc::new(1_u64));
        let b = vm.tlab.alloc_rust_data(Arc::new(1_u64));
        let snapshot = capture(&[Value::object(cell), Value::object(a), Value::object(b)]);
        source.store(Value::int(5));
        drop(vm);
        let roots = snapshot.roots();
        assert_ne!(roots[1], roots[2]);
        assert!(matches!(captured_object(&snapshot,roots[0]),Obj::Cell(value) if *value==roots[0]));
        assert!(matches!(
            captured_object(&snapshot, roots[1]),
            Obj::NonSnapshotableValue {}
        ));
        assert!(matches!(
            captured_object(&snapshot, roots[2]),
            Obj::NonSnapshotableValue {}
        ));
    }
}
