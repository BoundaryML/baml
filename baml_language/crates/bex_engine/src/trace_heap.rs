//! Trace-owned immutable value snapshots.
//!
//! `TraceHeap` is intentionally separate from the moving BEX heap. Capture
//! hooks copy BAML-visible values into this graph under a heap permit; later
//! consumers read and release snapshots without retaining `HeapPtr`s or live
//! host-owned values.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use baml_builtins2::{MediaContent, MediaValue};
use bex_external_types::{BexExternalAdt, BexExternalValue, MediaKind, try_convert_rust_data};
use bex_heap::{BexHeap, PermitProof};
use bex_vm_types::{HeapPtr, Object, Value, ValueKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceSnapshotHandle(u64);

impl TraceSnapshotHandle {
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn for_test(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceValueRef(usize);

impl TraceValueRef {
    #[must_use]
    pub fn raw(self) -> usize {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn for_test(raw: usize) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TraceValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bigint(String),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<TraceValueRef>),
    Map(Vec<(String, TraceValueRef)>),
    Media(TraceMediaValue),
    Instance {
        type_name: String,
        type_args: Vec<baml_type::RuntimeTy>,
        fields: Vec<(String, TraceValueRef)>,
    },
    Enum {
        type_name: String,
        variant: String,
    },
    Omitted(TraceOmissionDescriptor),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceMediaValue {
    pub kind: MediaKind,
    pub mime_type: Option<String>,
    pub content: TraceMediaContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceMediaContent {
    Url(String),
    Base64(String),
    File(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceOmissionDescriptor {
    pub reason: TraceOmissionReason,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceOmissionReason {
    OmittedArgument,
    UnsupportedValue,
    HostOwnedValue,
    InvalidRuntimeValue,
    CyclicReference,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceSnapshot {
    root: TraceValueRef,
    values: Vec<TraceValue>,
}

impl TraceSnapshot {
    #[must_use]
    pub fn root(&self) -> TraceValueRef {
        self.root
    }

    #[must_use]
    pub fn value(&self, value_ref: TraceValueRef) -> Option<&TraceValue> {
        self.values.get(value_ref.0)
    }

    #[cfg(test)]
    pub(crate) fn for_test(root: TraceValueRef, values: Vec<TraceValue>) -> Self {
        Self { root, values }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TraceHeap {
    inner: Arc<TraceHeapInner>,
}

#[derive(Debug, Default)]
struct TraceHeapInner {
    next_snapshot_id: AtomicU64,
    snapshots: Mutex<HashMap<TraceSnapshotHandle, TraceSnapshot>>,
}

impl TraceHeap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copy_value_from_bex_heap(
        &self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        value: Value,
    ) -> TraceSnapshotHandle {
        let mut builder = TraceSnapshotBuilder::default();
        let root = builder.copy_value(heap, permit, value);
        self.insert_snapshot(builder.finish(root))
    }

    pub fn copy_values_from_bex_heap(
        &self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        values: &[Value],
    ) -> TraceSnapshotHandle {
        let mut builder = TraceSnapshotBuilder::default();
        let items = values
            .iter()
            .copied()
            .map(|value| builder.copy_value(heap, permit, value))
            .collect();
        let root = builder.alloc(TraceValue::Array(items));
        self.insert_snapshot(builder.finish(root))
    }

    pub fn copy_named_values_from_bex_heap(
        &self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        entries: &[(String, Value)],
    ) -> TraceSnapshotHandle {
        let mut builder = TraceSnapshotBuilder::default();
        let entries = entries
            .iter()
            .map(|(key, value)| (key.clone(), builder.copy_value(heap, permit, *value)))
            .collect();
        let root = builder.alloc(TraceValue::Map(entries));
        self.insert_snapshot(builder.finish(root))
    }

    #[must_use]
    pub fn get(&self, handle: TraceSnapshotHandle) -> Option<TraceSnapshot> {
        self.inner
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&handle)
            .cloned()
    }

    pub fn release(&self, handle: TraceSnapshotHandle) -> Option<TraceSnapshot> {
        self.inner
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&handle)
    }

    #[must_use]
    pub fn retained_snapshot_count(&self) -> usize {
        self.inner
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn insert_snapshot(&self, snapshot: TraceSnapshot) -> TraceSnapshotHandle {
        let id = self.inner.next_snapshot_id.fetch_add(1, Ordering::Relaxed);
        let handle = TraceSnapshotHandle(id);
        self.inner
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(handle, snapshot);
        handle
    }

    #[cfg(test)]
    pub(crate) fn insert_for_test(&self, snapshot: TraceSnapshot) -> TraceSnapshotHandle {
        self.insert_snapshot(snapshot)
    }
}

#[derive(Default)]
struct TraceSnapshotBuilder {
    values: Vec<TraceValue>,
    in_progress: HashSet<HeapPtr>,
}

impl TraceSnapshotBuilder {
    fn finish(self, root: TraceValueRef) -> TraceSnapshot {
        TraceSnapshot {
            root,
            values: self.values,
        }
    }

    fn alloc(&mut self, value: TraceValue) -> TraceValueRef {
        let value_ref = TraceValueRef(self.values.len());
        self.values.push(value);
        value_ref
    }

    fn copy_value(
        &mut self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        value: Value,
    ) -> TraceValueRef {
        match value.kind() {
            ValueKind::Null => self.alloc(TraceValue::Null),
            ValueKind::Bool(value) => self.alloc(TraceValue::Bool(value)),
            ValueKind::Int(value) => self.alloc(TraceValue::Int(value)),
            ValueKind::OmittedArg => {
                self.omitted(TraceOmissionReason::OmittedArgument, "omitted argument")
            }
            ValueKind::Object(ptr) => self.copy_object(heap, permit, ptr),
        }
    }

    fn copy_object(
        &mut self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        ptr: HeapPtr,
    ) -> TraceValueRef {
        let object = unsafe { ptr.get() };
        let tracks_recursion = matches!(
            object,
            Object::Array(_) | Object::Map(_) | Object::Instance(_)
        );
        if tracks_recursion && !self.in_progress.insert(ptr) {
            return self.omitted(TraceOmissionReason::CyclicReference, "cyclic reference");
        }

        let value_ref = match object {
            Object::String(value) => self.alloc(TraceValue::String(value.to_string())),
            Object::Bigint(value) => self.alloc(TraceValue::Bigint(value.to_string())),
            Object::Float(value) => self.alloc(TraceValue::Float(*value)),
            Object::Uint8Array(bytes) => self.alloc(TraceValue::Bytes(bytes.to_vec())),
            Object::Array(array) => {
                let items = array
                    .to_vec()
                    .into_iter()
                    .map(|value| self.copy_value(heap, permit, value))
                    .collect();
                self.alloc(TraceValue::Array(items))
            }
            Object::Map(map) => {
                let entries = map
                    .to_index_map()
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), self.copy_value(heap, permit, value)))
                    .collect();
                self.alloc(TraceValue::Map(entries))
            }
            Object::Instance(instance) => {
                let class_object = unsafe { instance.class.get() };
                if let Object::Class(class) = class_object {
                    let fields = class
                        .fields
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(field, slot)| {
                            (
                                field.name.clone(),
                                self.copy_value(heap, permit, slot.load()),
                            )
                        })
                        .collect();
                    self.alloc(TraceValue::Instance {
                        // A trace is read by a person: a runtime-minted
                        // declaration appears under the name its source wrote,
                        // not the discriminator that keys it in the VM.
                        type_name: class.name.render_source_dotted(),
                        type_args: instance
                            .class_type_args
                            .iter()
                            .map(baml_type::RuntimeTy::from)
                            .collect(),
                        fields,
                    })
                } else {
                    self.omitted(
                        TraceOmissionReason::InvalidRuntimeValue,
                        "instance class pointer did not point at a class",
                    )
                }
            }
            Object::Variant(variant) => {
                let enum_object = unsafe { variant.enm.get() };
                let Object::Enum(enum_) = enum_object else {
                    return self.omitted(
                        TraceOmissionReason::InvalidRuntimeValue,
                        "variant enum pointer did not point at an enum",
                    );
                };
                let Some(variant_def) = enum_.variants.get(variant.index) else {
                    return self.omitted(
                        TraceOmissionReason::InvalidRuntimeValue,
                        "variant index was outside the enum definition",
                    );
                };
                self.alloc(TraceValue::Enum {
                    type_name: enum_.name.render_source_dotted(),
                    variant: variant_def.name.clone(),
                })
            }
            Object::Function(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Interface(_)
            | Object::Package(_)
            | Object::ImplRule(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::GenericFunction(_)
            | Object::Cell(_)
            | Object::Future(_)
            | Object::UnscheduledFuture(_)
            | Object::Collector(_)
            | Object::Type(_) => self.omitted(
                TraceOmissionReason::UnsupportedValue,
                unsupported_object_message(object),
            ),
            Object::HostClosure(_) => self.omitted(
                TraceOmissionReason::HostOwnedValue,
                unsupported_object_message(object),
            ),
            Object::RustData(data) => match try_convert_rust_data(data) {
                Some(value) => self.copy_external_value(value),
                None => self.omitted(
                    TraceOmissionReason::HostOwnedValue,
                    unsupported_object_message(object),
                ),
            },
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => self.omitted(
                TraceOmissionReason::InvalidRuntimeValue,
                unsupported_object_message(object),
            ),
        };
        if tracks_recursion {
            self.in_progress.remove(&ptr);
        }
        value_ref
    }

    fn copy_external_value(&mut self, value: BexExternalValue) -> TraceValueRef {
        match value {
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => self.copy_media(&media),
            BexExternalValue::HostValue(host) => self.omitted(
                TraceOmissionReason::HostOwnedValue,
                match host.kind {
                    bex_external_types::HostValueKind::Callable => "host-owned callable",
                    bex_external_types::HostValueKind::Opaque => "host-owned opaque value",
                },
            ),
            BexExternalValue::RustData(_) => {
                self.omitted(TraceOmissionReason::HostOwnedValue, "host-owned rust data")
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                self.omitted(TraceOmissionReason::UnsupportedValue, "prompt AST")
            }
            _ => self.omitted(
                TraceOmissionReason::UnsupportedValue,
                "unsupported rust data conversion",
            ),
        }
    }

    fn copy_media(&mut self, media: &MediaValue) -> TraceValueRef {
        let content = media.read_content(|content| match content {
            MediaContent::Url { url, .. } => TraceMediaContent::Url(url.clone()),
            MediaContent::Base64 { base64_data } => TraceMediaContent::Base64(base64_data.clone()),
            MediaContent::File { file, .. } => TraceMediaContent::File(file.clone()),
        });
        self.alloc(TraceValue::Media(TraceMediaValue {
            kind: media.kind,
            mime_type: media.mime_type(),
            content,
        }))
    }

    fn omitted(
        &mut self,
        reason: TraceOmissionReason,
        message: impl Into<String>,
    ) -> TraceValueRef {
        self.alloc(TraceValue::Omitted(TraceOmissionDescriptor {
            reason,
            message: message.into(),
        }))
    }
}

fn unsupported_object_message(object: &Object) -> String {
    match object {
        Object::Function(_) => "function",
        Object::Class(_) => "class",
        Object::Enum(_) => "enum",
        Object::Interface(_) => "interface",
        Object::Package(_) => "package",
        Object::ImplRule(_) => "impl rule",
        Object::Closure(_) => "closure",
        Object::BoundMethod(_) => "bound method",
        Object::GenericFunction(_) => "generic function",
        Object::HostClosure(_) => "host-owned callable",
        Object::Cell(_) => "cell",
        Object::Future(_) => "future",
        Object::UnscheduledFuture(_) => "unscheduled future",
        Object::RustData(_) => "host-owned rust data",
        Object::Collector(_) => "collector",
        Object::Type(_) => "type descriptor",
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => "heap sentinel",
        Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Array(_)
        | Object::Map(_)
        | Object::Instance(_)
        | Object::Variant(_)
        | Object::Float(_) => "unsupported value",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use baml_builtins2::MediaValue;
    use bex_external_types::MediaKind;
    use bex_heap::{BexHeap, HeapPermit as _, HeapPermitManager, Tlab, TlabHolder};
    use bex_vm_types::{Object, RootHaver, Value};

    use super::{TraceHeap, TraceMediaContent, TraceOmissionReason, TraceValue};

    struct EmptyRoots {
        tlab: Tlab,
    }

    impl RootHaver for EmptyRoots {
        fn collect_roots(&self, _roots: &mut Vec<bex_vm_types::HeapPtr>) {}
        fn forward_roots(
            &mut self,
            _forward: &HashMap<bex_vm_types::HeapPtr, bex_vm_types::HeapPtr>,
        ) {
        }
    }

    impl TlabHolder for EmptyRoots {
        fn tlab(&self) -> &Tlab {
            &self.tlab
        }

        fn tlab_mut(&mut self) -> &mut Tlab {
            &mut self.tlab
        }
    }

    #[tokio::test]
    async fn trace_heap_copies_values_into_releasable_snapshot() {
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        let trace_heap = TraceHeap::new();

        let handle = trace_heap.copy_values_from_bex_heap(
            &heap,
            permit.proof(),
            &[Value::int(7), Value::TRUE, Value::NULL],
        );
        let snapshot = trace_heap.get(handle).expect("snapshot retained");

        let TraceValue::Array(items) = snapshot.value(snapshot.root()).unwrap() else {
            panic!("root should be an array");
        };
        assert_eq!(snapshot.value(items[0]), Some(&TraceValue::Int(7)));
        assert_eq!(snapshot.value(items[1]), Some(&TraceValue::Bool(true)));
        assert_eq!(snapshot.value(items[2]), Some(&TraceValue::Null));

        assert!(trace_heap.release(handle).is_some());
        assert!(trace_heap.get(handle).is_none());
        assert!(trace_heap.release(handle).is_none());
    }

    #[tokio::test]
    async fn trace_heap_uses_omission_for_omitted_args_and_host_owned_objects() {
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let mut permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        let host_owned = permit
            .tlab_mut()
            .alloc(Object::RustData(std::sync::Arc::new(1_u64)));
        let trace_heap = TraceHeap::new();

        let handle = trace_heap.copy_values_from_bex_heap(
            &heap,
            permit.proof(),
            &[Value::OMITTED_ARG, Value::object(host_owned)],
        );
        let snapshot = trace_heap.release(handle).expect("snapshot retained");
        let TraceValue::Array(items) = snapshot.value(snapshot.root()).unwrap() else {
            panic!("root should be an array");
        };

        let Some(TraceValue::Omitted(omitted_arg)) = snapshot.value(items[0]) else {
            panic!("first item should be omitted");
        };
        assert_eq!(omitted_arg.reason, TraceOmissionReason::OmittedArgument);

        let Some(TraceValue::Omitted(host_owned)) = snapshot.value(items[1]) else {
            panic!("second item should be omitted");
        };
        assert_eq!(host_owned.reason, TraceOmissionReason::HostOwnedValue);
    }

    #[tokio::test]
    async fn trace_heap_snapshots_media_rust_data() {
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let mut permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        let media =
            MediaValue::from_base64(MediaKind::Image, "aW1hZ2UtYnl0ZXM=", Some("image/png"));
        let media_ptr = permit.tlab_mut().alloc(Object::RustData(media));
        let trace_heap = TraceHeap::new();

        let handle =
            trace_heap.copy_value_from_bex_heap(&heap, permit.proof(), Value::object(media_ptr));
        let snapshot = trace_heap.release(handle).expect("snapshot retained");

        let Some(TraceValue::Media(media)) = snapshot.value(snapshot.root()) else {
            panic!("root should be media");
        };
        assert_eq!(media.kind, MediaKind::Image);
        assert_eq!(media.mime_type.as_deref(), Some("image/png"));
        assert_eq!(
            media.content,
            TraceMediaContent::Base64("aW1hZ2UtYnl0ZXM=".to_string())
        );
    }

    #[tokio::test]
    async fn trace_heap_omits_cyclic_back_edges() {
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let mut permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        let array_ptr = permit
            .tlab_mut()
            .alloc_array(baml_type::RealizedTy::unknown(), vec![]);
        unsafe {
            *array_ptr.get_mut() = Object::Array(bex_vm_types::types::Array::new(
                baml_type::RealizedTy::unknown(),
                vec![Value::int(1), Value::object(array_ptr)],
            ));
        }
        let trace_heap = TraceHeap::new();

        let handle =
            trace_heap.copy_value_from_bex_heap(&heap, permit.proof(), Value::object(array_ptr));
        let snapshot = trace_heap.release(handle).expect("snapshot retained");

        let Some(TraceValue::Array(items)) = snapshot.value(snapshot.root()) else {
            panic!("root should be an array");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(snapshot.value(items[0]), Some(&TraceValue::Int(1)));
        let Some(TraceValue::Omitted(cycle)) = snapshot.value(items[1]) else {
            panic!("cycle back-edge should be omitted");
        };
        assert_eq!(cycle.reason, TraceOmissionReason::CyclicReference);
    }
}
