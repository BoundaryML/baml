//! Heap traversal stays in the VM. Frozen graphs contain only owned leaves and
//! snapshot-local indexes. Locks sample containers independently, not atomically.
use bex_vm_types::{HeapPtr, MediaContent, MediaValue, Object, Value, ValueKind};
use btel_snapshot::{
    Builder, Description, Leaves, Limit, MediaSource, ObjectId, Reserved, Shaper, Snapshot,
    SnapshotObject, SnapshotRoot, SnapshotValue,
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
    /// Objects named already, whose content is still to be captured.
    work: Vec<(Reserved, HeapPtr, usize)>,
    shaper: Shaper,
}
impl Scratch {
    /// The capturing thread's shaping scratch, for captures built outside
    /// [`Self::capture`].
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    pub(super) fn shaper(&mut self) -> &mut Shaper {
        &mut self.shaper
    }
    /// The value as the capture holds it. An object gets its identity here
    /// and its content later, so that what it holds can name it.
    fn add(&mut self, leaves: &mut Leaves<'_>, value: Value, depth: usize) -> SnapshotValue {
        if let Some(ptr) = value.as_object_ptr() {
            if let Some(value) = self.seen.get(&ptr) {
                return *value;
            }
        }
        if leaves.limits().max_depth.is_some_and(|max| depth > max) {
            return SnapshotValue::Truncated(Limit::Depth);
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
            Object::String(s) => leaves.string_value(s),
            Object::Bigint(n) => leaves.bigint(n),
            Object::Type(value) => SnapshotValue::Type(leaves.ty(owned_type(&value.ty))),
            Object::Variant(v) => {
                let declaration = match self.add(leaves, Value::object(v.enm), depth + 1) {
                    SnapshotValue::Object(id) => id,
                    SnapshotValue::Truncated(reason) => return SnapshotValue::Truncated(reason),
                    _ => unreachable!("enum declaration is an object"),
                };
                // SAFETY: enum declaration lives under the same heap permit.
                let Object::Enum(enm) = (unsafe { v.enm.get() }) else {
                    unreachable!("variant enum")
                };
                match leaves.label(&enm.variants[v.index].name.as_str().into()) {
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
                    // Media is the one host value captured by content. Others
                    // hold keys and cipher state, and stay opaque.
                    let object = match data.downcast_ref::<MediaValue>() {
                        Some(value) => media(leaves, value),
                        None => SnapshotObject::NonSnapshotableValue {},
                    };
                    let Some(id) = leaves.object(object) else {
                        return SnapshotValue::Truncated(Limit::Objects);
                    };
                    self.rust_seen.insert(identity, id);
                    id
                };
                SnapshotValue::Object(id)
            }
            _ => {
                let Some(slot) = leaves.reserve() else {
                    return SnapshotValue::Truncated(Limit::Objects);
                };
                let value = SnapshotValue::Object(slot.id());
                self.work.push((slot, ptr, depth));
                value
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
        let root = match input {
            Input::FunctionArgs(args) => {
                b.arguments(args.iter(), |leaves, value| self.add(leaves, *value, 0))
            }
            Input::Value(value) => SnapshotRoot::Value(self.add(&mut b.leaves(), value, 0)),
        };
        while let Some((slot, ptr, depth)) = self.work.pop() {
            // SAFETY: inherited heap permit, never relinquished during traversal.
            let object = match unsafe { ptr.get() } {
                Object::Uint8Array(data) => b.bytes(&data.lock()),
                Object::Array(data) => {
                    let element_type = b.leaves().ty(owned_type(&data.element_ty));
                    let data = data.lock();
                    b.list(element_type, data.iter(), |leaves, value| {
                        self.add(leaves, *value, depth + 1)
                    })
                }
                Object::Map(data) => {
                    let key_type = b.leaves().ty(owned_type(&data.key_ty));
                    let value_type = b.leaves().ty(owned_type(&data.value_ty));
                    let data = data.lock();
                    b.map(key_type, value_type, data.iter(), |leaves, (key, value)| {
                        (key.clone(), self.add(leaves, *value, depth + 1))
                    })
                }
                Object::Instance(instance) => {
                    let class = Value::object(instance.class);
                    match self.add(&mut b.leaves(), class, depth + 1) {
                        SnapshotValue::Object(declaration) => {
                            // SAFETY: class lives under the same heap permit.
                            let Object::Class(class) = (unsafe { instance.class.get() }) else {
                                unreachable!("instance class")
                            };
                            b.instance(
                                declaration,
                                instance.class_type_args.iter().map(owned_type),
                                (0..instance.field_len()).zip(&class.fields),
                                |leaves, (at, field)| {
                                    let value =
                                        self.add(leaves, instance.load_field(at), depth + 1);
                                    (field.name.as_str().into(), value)
                                },
                            )
                        }
                        SnapshotValue::Truncated(reason) => SnapshotObject::Truncated(reason),
                        _ => unreachable!("class declaration is an object"),
                    }
                }
                Object::Class(class) => b.declaration(&class.name, class.type_tag, false),
                Object::Enum(enm) => b.declaration(&enm.name, enm.type_tag, true),
                Object::Cell(cell) => {
                    SnapshotObject::Cell(self.add(&mut b.leaves(), cell.load(), depth + 1))
                }
                Object::Function(f) => SnapshotObject::Descriptive {
                    kind: Description::Function,
                    name: b.leaves().label(&f.name.as_str().into()),
                },
                Object::Closure(_) => describe(Description::Closure),
                Object::BoundMethod(_) => describe(Description::BoundMethod),
                Object::GenericFunction(_) => describe(Description::GenericFunction),
                Object::HostClosure(_) => describe(Description::HostFunction),
                Object::Future(_) => describe(Description::Future),
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
            b.fill(slot, object);
        }
        self.seen.clear();
        self.rust_seen.clear();
        self.work.clear();
        b.finish(root, &mut self.shaper)
    }
}
// Preserve immutable type metadata; replace every VM type head with owned identity.
fn owned_type(ty: &bex_vm_types::RealizedTy) -> btel_snapshot::OwnedType {
    ty.map_heads(&mut |head| match head.tagged_name() {
        Some(name) => btel_snapshot::TypeIdentity::Resolved(name),
        None => btel_snapshot::TypeIdentity::Unresolved(head.tag()),
    })
}
/// A media value as the runtime holds it: its kind, MIME type and source, and
/// loaded content as base64 text shared by handle. Reads no file and fetches
/// nothing. A part past the byte limit truncates the whole object.
fn media(leaves: &mut Leaves<'_>, value: &MediaValue) -> SnapshotObject {
    let Ok(mime_type) = value.read_mime_type(|mime| {
        mime.map(|mime| leaves.label(&mime.into()).ok_or(()))
            .transpose()
    }) else {
        return SnapshotObject::Truncated(Limit::Bytes);
    };
    let source = value.read_content(|content| {
        let loaded = |leaves: &mut Leaves<'_>, data: Option<&bex_str::BexStr>| match data {
            Some(data) => leaves.string(data).map(Some),
            None => Some(None),
        };
        Some(match content {
            MediaContent::Url { url, base64_data } => MediaSource::Url {
                url: leaves.label(&url.as_str().into())?,
                data: loaded(leaves, base64_data.as_ref())?,
            },
            MediaContent::File { file, base64_data } => MediaSource::File {
                path: leaves.label(&file.as_str().into())?,
                data: loaded(leaves, base64_data.as_ref())?,
            },
            MediaContent::Base64 { base64_data } => MediaSource::Base64 {
                data: leaves.string(base64_data)?,
            },
        })
    });
    match source {
        Some(source) => SnapshotObject::Media {
            kind: value.kind,
            mime_type,
            source,
        },
        None => SnapshotObject::Truncated(Limit::Bytes),
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
        let Obj::Uint8Array { data, .. } = captured_object(&snapshot, children[0]) else {
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
        let opaque: Arc<dyn std::any::Any + Send + Sync> = Arc::new(7_u64);
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
    fn media_is_captured_by_content_without_reading_files_or_retaining_sources() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        // Longer than the inline capacity, so the payload is heap-backed.
        let payload = bex_str::BexStr::from("iVBORw0K".repeat(64));
        let bex_str::BexStr::Flat(stored) = &payload else {
            panic!("expected a heap-backed payload, got {payload:?}")
        };
        let stored = Arc::clone(stored);
        let sources: [Arc<dyn std::any::Any + Send + Sync>; 3] = [
            MediaValue::from_file(
                baml_type::MediaKind::Image,
                "/nonexistent/telemetry-must-not-read.png",
                None,
            ),
            MediaValue::from_url(
                baml_type::MediaKind::Pdf,
                "https://example.test/report.pdf",
                Some("application/pdf"),
            ),
            MediaValue::from_base64(baml_type::MediaKind::Audio, payload, Some("audio/wav")),
        ];
        let weak: Vec<_> = sources.iter().map(Arc::downgrade).collect();
        // Two host wrappers of one media value are one captured object.
        let values: Vec<_> = [&sources[0], &sources[1], &sources[2], &sources[2]]
            .into_iter()
            .map(|source| Value::object(vm.tlab.alloc_rust_data(Arc::clone(source))))
            .collect();
        drop(sources);
        let snapshot = capture(&values);
        unsafe {
            vm.heap
                .collect_garbage_generational(&[], bex_heap::CollectionLevel::Major)
        };
        drop(vm);
        assert!(weak.iter().all(|weak| weak.upgrade().is_none()));
        let roots = snapshot.roots();
        assert_eq!(roots[2], roots[3]);
        assert_eq!(snapshot.object_count(), 3);
        let media = |value| match captured_object(&snapshot, value) {
            Obj::Media {
                kind,
                mime_type,
                source,
            } => (
                *kind,
                mime_type.map(|id| snapshot.label(id).as_str()),
                *source,
            ),
            other => panic!("expected media, got {other:?}"),
        };
        let (kind, mime_type, MediaSource::File { path, data: None }) = media(roots[0]) else {
            panic!("expected a file without content")
        };
        assert_eq!((kind, mime_type), (baml_type::MediaKind::Image, None));
        assert_eq!(
            snapshot.label(path).as_str(),
            "/nonexistent/telemetry-must-not-read.png"
        );
        let (kind, mime_type, MediaSource::Url { url, data: None }) = media(roots[1]) else {
            panic!("expected a URL without content")
        };
        assert_eq!(
            (kind, mime_type),
            (baml_type::MediaKind::Pdf, Some("application/pdf"))
        );
        assert_eq!(
            snapshot.label(url).as_str(),
            "https://example.test/report.pdf"
        );
        let (kind, mime_type, MediaSource::Base64 { data }) = media(roots[2]) else {
            panic!("expected base64 content")
        };
        assert_eq!(
            (kind, mime_type),
            (baml_type::MediaKind::Audio, Some("audio/wav"))
        );
        let bex_str::BexStr::Flat(captured) = snapshot.string(data) else {
            panic!("expected the payload's own handle")
        };
        assert!(Arc::ptr_eq(captured, &stored), "capture shares the payload");
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
        // The list is at the depth limit, so what it holds is cut.
        let Obj::List { items, .. } = captured_object(&snapshot, snapshot.roots()[0]) else {
            panic!()
        };
        let items = snapshot.values(*items);
        assert_eq!(items.len(), 8);
        assert!(
            items
                .iter()
                .all(|item| *item == Val::Truncated(Limit::Depth))
        );
    }
    #[test]
    fn a_value_too_large_to_hold_whole_is_captured_as_truncated() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let string =
            |vm: &mut crate::BexVm, length| Value::object(vm.tlab.alloc_string("s".repeat(length)));
        let bigint = |vm: &mut crate::BexVm, bits: u32| {
            Value::object(
                vm.tlab
                    .alloc_bigint(num_bigint::BigInt::from(1) << (bits - 1)),
            )
        };
        let bytes = |vm: &mut crate::BexVm, length| {
            Value::object(vm.tlab.alloc_uint8array(vec![7; length]))
        };
        let image = |vm: &mut crate::BexVm, length: usize| {
            let media: Arc<dyn std::any::Any + Send + Sync> = MediaValue::from_base64(
                baml_type::MediaKind::Image,
                bex_str::BexStr::from("A".repeat(length)),
                None,
            );
            Value::object(vm.tlab.alloc_rust_data(media))
        };
        // Each kind at the limit, then just over it.
        let values = [
            string(&mut vm, 64),
            string(&mut vm, 65),
            bigint(&mut vm, 64 * 8),
            bigint(&mut vm, 64 * 8 + 1),
            bytes(&mut vm, 64),
            bytes(&mut vm, 65),
            image(&mut vm, 64),
            image(&mut vm, 65),
        ];
        let pool = btel_snapshot::SnapshotPool::new(
            1,
            btel_snapshot::Limits {
                max_leaf_bytes: 64,
                ..Default::default()
            },
        );
        // SAFETY: the values stay live and nothing collects during capture.
        let snapshot = unsafe {
            Scratch::default().capture(pool.try_acquire().unwrap(), Input::FunctionArgs(&values))
        };
        drop(vm);
        let roots = snapshot.roots();
        assert!(matches!(roots[0], Val::String(_)));
        assert_eq!(roots[1], Val::Truncated(Limit::Bytes));
        assert!(matches!(roots[2], Val::Bigint(_)));
        assert_eq!(roots[3], Val::Truncated(Limit::Bytes));
        let Obj::Uint8Array { data } = captured_object(&snapshot, roots[4]) else {
            panic!("the bytes at the limit are held")
        };
        assert_eq!(snapshot.bytes(*data), [7; 64]);
        // The length stays; none of the content does.
        assert!(matches!(
            captured_object(&snapshot, roots[5]),
            Obj::Uint8ArrayTruncated { original_len: 65 }
        ));
        assert!(matches!(
            captured_object(&snapshot, roots[6]),
            Obj::Media { .. }
        ));
        assert!(matches!(
            captured_object(&snapshot, roots[7]),
            Obj::Truncated(Limit::Bytes)
        ));
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
            methods: indexmap::IndexMap::new(),
            generic_param_count: 1,
            owner: bex_vm_types::types::Owner::anonymous(),
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
            owner: bex_vm_types::types::Owner::anonymous(),
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
        let Some(baml_type::RealizedTy::Class(head, ..)) =
            snapshot.type_arguments(*type_arguments).next()
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
        assert_eq!(snapshot.name(*name).item_name().as_str(), "TestClass");
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
        assert_eq!(snapshot.label(name).as_str(), "Green");
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
        assert_eq!(snapshot.root_id(), copied_snapshot.root_id());
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
        let mut node = snapshot.roots()[0];
        for _ in 0..256 {
            let Obj::List { items, .. } = captured_object(&snapshot, node) else {
                panic!()
            };
            node = snapshot.values(*items)[0];
        }
        let Obj::Uint8Array { data } = captured_object(&snapshot, node) else {
            panic!()
        };
        assert_eq!(snapshot.bytes(*data).len(), 9 * 1024 * 1024);
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
