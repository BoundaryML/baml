//! Trace-owned immutable value snapshots.
//!
//! `TraceHeap` is intentionally separate from the moving BEX heap. Capture
//! hooks copy BAML-visible values into this graph under a heap permit; later
//! consumers read and release snapshots without retaining `HeapPtr`s or live
//! host-owned values.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use baml_builtins2::{MediaContent, MediaValue};
use bex_events::prof::backend::{Reservation, ValueLossReason};
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

    pub(crate) fn values(&self) -> impl Iterator<Item = &TraceValue> {
        self.values.iter()
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
        let root = builder
            .copy_value(heap, permit, value)
            .expect("unbounded trace snapshot copy cannot be memory-denied");
        self.insert_snapshot(builder.finish(root))
    }

    pub fn copy_value_bounded(
        heap: &BexHeap,
        permit: PermitProof<'_>,
        value: Value,
        reservation: &mut Reservation,
    ) -> Result<TraceSnapshot, ValueLossReason> {
        let mut builder = TraceSnapshotBuilder::bounded(reservation);
        let root = builder.copy_value(heap, permit, value)?;
        Ok(builder.finish(root))
    }

    pub fn copy_named_values_bounded(
        heap: &BexHeap,
        permit: PermitProof<'_>,
        entries: &[(String, Value)],
        reservation: &mut Reservation,
    ) -> Result<TraceSnapshot, ValueLossReason> {
        let mut builder = TraceSnapshotBuilder::bounded(reservation);
        let mut copied = builder.vec_with_capacity(entries.len())?;
        for (key, value) in entries {
            let key = builder.copy_str(key)?;
            let value = builder.copy_value(heap, permit, *value)?;
            copied.push((key, value));
        }
        let root = builder.alloc(TraceValue::Map(copied))?;
        Ok(builder.finish(root))
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
            .map(|value| {
                builder
                    .copy_value(heap, permit, value)
                    .expect("unbounded trace snapshot copy cannot be memory-denied")
            })
            .collect();
        let root = builder
            .alloc(TraceValue::Array(items))
            .expect("unbounded trace snapshot copy cannot be memory-denied");
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
            .map(|(key, value)| {
                (
                    key.clone(),
                    builder
                        .copy_value(heap, permit, *value)
                        .expect("unbounded trace snapshot copy cannot be memory-denied"),
                )
            })
            .collect();
        let root = builder
            .alloc(TraceValue::Map(entries))
            .expect("unbounded trace snapshot copy cannot be memory-denied");
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
struct TraceSnapshotBuilder<'a> {
    values: Vec<TraceValue>,
    in_progress: Vec<HeapPtr>,
    budget: Option<&'a mut Reservation>,
}

impl<'a> TraceSnapshotBuilder<'a> {
    fn bounded(reservation: &'a mut Reservation) -> Self {
        Self {
            values: Vec::new(),
            in_progress: Vec::new(),
            budget: Some(reservation),
        }
    }

    fn charge(&mut self, bytes: usize) -> Result<(), ValueLossReason> {
        let Some(reservation) = self.budget.as_deref_mut() else {
            return Ok(());
        };
        reservation
            .try_grow(u64::try_from(bytes).unwrap_or(u64::MAX))
            .map_err(|_| ValueLossReason::ValueMemoryExceeded)
    }

    fn reserve_values(&mut self) -> Result<(), ValueLossReason> {
        if self.values.len() < self.values.capacity() {
            return Ok(());
        }
        let next_capacity = self.values.capacity().max(8).saturating_mul(2);
        let additional = next_capacity.saturating_sub(self.values.capacity());
        self.charge(additional.saturating_mul(std::mem::size_of::<TraceValue>()))?;
        self.values
            .try_reserve_exact(additional)
            .map_err(|_| ValueLossReason::CopyFailed)
    }

    fn reserve_progress(&mut self) -> Result<(), ValueLossReason> {
        if self.in_progress.len() < self.in_progress.capacity() {
            return Ok(());
        }
        let next_capacity = self.in_progress.capacity().max(8).saturating_mul(2);
        let additional = next_capacity.saturating_sub(self.in_progress.capacity());
        self.charge(additional.saturating_mul(std::mem::size_of::<HeapPtr>()))?;
        self.in_progress
            .try_reserve_exact(additional)
            .map_err(|_| ValueLossReason::CopyFailed)
    }

    fn vec_with_capacity<T>(&mut self, capacity: usize) -> Result<Vec<T>, ValueLossReason> {
        self.charge(capacity.saturating_mul(std::mem::size_of::<T>()))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| ValueLossReason::CopyFailed)?;
        Ok(values)
    }

    fn copy_str(&mut self, value: &str) -> Result<String, ValueLossReason> {
        self.charge(value.len())?;
        let mut output = String::new();
        output
            .try_reserve_exact(value.len())
            .map_err(|_| ValueLossReason::CopyFailed)?;
        output.push_str(value);
        Ok(output)
    }
    fn finish(self, root: TraceValueRef) -> TraceSnapshot {
        TraceSnapshot {
            root,
            values: self.values,
        }
    }

    fn alloc(&mut self, value: TraceValue) -> Result<TraceValueRef, ValueLossReason> {
        self.reserve_values()?;
        let value_ref = TraceValueRef(self.values.len());
        self.values.push(value);
        Ok(value_ref)
    }

    fn copy_value(
        &mut self,
        heap: &BexHeap,
        permit: PermitProof<'_>,
        value: Value,
    ) -> Result<TraceValueRef, ValueLossReason> {
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
    ) -> Result<TraceValueRef, ValueLossReason> {
        let object = unsafe { ptr.get() };
        let tracks_recursion = matches!(
            object,
            Object::Array(_) | Object::Map(_) | Object::Instance(_)
        );
        if tracks_recursion && self.in_progress.contains(&ptr) {
            return self.omitted(TraceOmissionReason::CyclicReference, "cyclic reference");
        }
        if tracks_recursion {
            self.reserve_progress()?;
            self.in_progress.push(ptr);
        }

        let value_ref = match object {
            Object::String(value) => {
                let value = self.copy_str(value.as_str())?;
                self.alloc(TraceValue::String(value))
            }
            Object::Bigint(value) => {
                let maximum_decimal_bytes = usize::try_from(value.bits())
                    .unwrap_or(usize::MAX)
                    .saturating_add(2);
                self.charge(maximum_decimal_bytes)?;
                self.alloc(TraceValue::Bigint(value.to_string()))
            }
            Object::Float(value) => self.alloc(TraceValue::Float(*value)),
            Object::Uint8Array(bytes) => {
                let bytes = bytes.lock();
                self.charge(bytes.len())?;
                let mut copied = Vec::new();
                copied
                    .try_reserve_exact(bytes.len())
                    .map_err(|_| ValueLossReason::CopyFailed)?;
                copied.extend_from_slice(&bytes);
                self.alloc(TraceValue::Bytes(copied))
            }
            Object::Array(array) => {
                let array = array.lock();
                let mut items = self.vec_with_capacity(array.len())?;
                for value in array.iter().copied() {
                    items.push(self.copy_value(heap, permit, value)?);
                }
                self.alloc(TraceValue::Array(items))
            }
            Object::Map(map) => {
                let map = map.data.lock();
                let mut entries = self.vec_with_capacity(map.len())?;
                for (key, value) in map.iter() {
                    let key = self.copy_str(key.as_str())?;
                    let value = self.copy_value(heap, permit, *value)?;
                    entries.push((key, value));
                }
                self.alloc(TraceValue::Map(entries))
            }
            Object::Instance(instance) => {
                let class_object = unsafe { instance.class.get() };
                if let Object::Class(class) = class_object {
                    let mut fields = self.vec_with_capacity(class.fields.len())?;
                    for (field, slot) in class.fields.iter().zip(instance.fields.iter()) {
                        let name = self.copy_str(&field.name)?;
                        let value = self.copy_value(heap, permit, slot.load())?;
                        fields.push((name, value));
                    }
                    let mut type_args = self.vec_with_capacity(instance.class_type_args.len())?;
                    for type_arg in &instance.class_type_args {
                        self.charge(realized_ty_allocation_bound(type_arg))?;
                        type_args.push(baml_type::RuntimeTy::from(type_arg));
                    }
                    // A trace is read by a person: a runtime-minted
                    // declaration appears under the name its source wrote,
                    // not the discriminator that keys it in the VM.
                    let type_name = self.copy_str(&class.name.render_source_dotted())?;
                    self.alloc(TraceValue::Instance {
                        type_name,
                        type_args,
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
                let type_name = self.copy_str(&enum_.name.render_source_dotted())?;
                let variant = self.copy_str(&variant_def.name)?;
                self.alloc(TraceValue::Enum { type_name, variant })
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
            debug_assert_eq!(self.in_progress.pop(), Some(ptr));
        }
        value_ref
    }

    fn copy_external_value(
        &mut self,
        value: BexExternalValue,
    ) -> Result<TraceValueRef, ValueLossReason> {
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

    fn copy_media(&mut self, media: &MediaValue) -> Result<TraceValueRef, ValueLossReason> {
        let content = media.read_content(|content| match content {
            MediaContent::Url { url, .. } => self.copy_str(url).map(TraceMediaContent::Url),
            MediaContent::Base64 { base64_data } => {
                self.copy_str(base64_data).map(TraceMediaContent::Base64)
            }
            MediaContent::File { file, .. } => self.copy_str(file).map(TraceMediaContent::File),
        })?;
        let mime_type = media
            .read_mime_type(|mime_type| mime_type.map(|value| self.copy_str(value)))
            .transpose()?;
        self.alloc(TraceValue::Media(TraceMediaValue {
            kind: media.kind,
            mime_type,
            content,
        }))
    }

    fn omitted(
        &mut self,
        reason: TraceOmissionReason,
        message: &str,
    ) -> Result<TraceValueRef, ValueLossReason> {
        let message = self.copy_str(message)?;
        self.alloc(TraceValue::Omitted(TraceOmissionDescriptor {
            reason,
            message,
        }))
    }
}

fn realized_ty_allocation_bound(ty: &baml_type::RealizedTy) -> usize {
    use baml_type::{Literal, RealizedTy};

    fn add(left: usize, right: usize) -> usize {
        left.saturating_add(right)
    }

    fn name_bound(name: &baml_type::Name) -> usize {
        std::mem::size_of::<baml_type::Name>().saturating_add(name.as_str().len())
    }

    fn type_name_bound(name: &baml_type::TypeName) -> usize {
        let namespace = name
            .namespace()
            .iter()
            .fold(0usize, |total, part| add(total, name_bound(part)));
        add(
            add(name_bound(name.package()), name_bound(name.name())),
            namespace,
        )
    }

    fn literal_bound(literal: &Literal) -> usize {
        match literal {
            Literal::String(value) | Literal::Float(value) => value.len(),
            Literal::Bigint(value) => {
                usize::try_from(value.bits())
                    .unwrap_or(usize::MAX)
                    .saturating_add(7)
                    / 8
            }
            Literal::Int(_) | Literal::Bool(_) => 0,
        }
    }

    let node = std::mem::size_of::<baml_type::RuntimeTy>();
    let nested = match ty {
        RealizedTy::Literal(value, _, _) => literal_bound(value),
        RealizedTy::Class(name, args, _) => {
            args.iter().fold(type_name_bound(name), |total, arg| {
                add(total, realized_ty_allocation_bound(arg))
            })
        }
        RealizedTy::Interface(name, args, bindings, _) => {
            let args = args.iter().fold(type_name_bound(name), |total, arg| {
                add(total, realized_ty_allocation_bound(arg))
            });
            bindings.iter().fold(args, |total, (name, ty)| {
                add(
                    add(total, name_bound(name)),
                    realized_ty_allocation_bound(ty),
                )
            })
        }
        RealizedTy::Enum(name, _) | RealizedTy::TypeAlias(name, _) => type_name_bound(name),
        RealizedTy::EnumVariant(name, variant, _) => {
            add(type_name_bound(name), name_bound(variant))
        }
        RealizedTy::List(inner, _) => realized_ty_allocation_bound(inner),
        RealizedTy::Map { key, value, .. } | RealizedTy::Future(key, value, _) => add(
            realized_ty_allocation_bound(key),
            realized_ty_allocation_bound(value),
        ),
        RealizedTy::Union(members, _) => members.iter().fold(0usize, |total, member| {
            add(total, realized_ty_allocation_bound(member))
        }),
        RealizedTy::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let params = params.iter().fold(0usize, |total, param| {
                let name = param.name.as_ref().map_or(0, name_bound);
                add(
                    add(
                        total,
                        std::mem::size_of::<baml_type::RuntimeFunctionParamTy>(),
                    ),
                    add(name, realized_ty_allocation_bound(&param.ty)),
                )
            });
            add(
                add(params, realized_ty_allocation_bound(ret)),
                realized_ty_allocation_bound(throws),
            )
        }
        RealizedTy::Int { .. }
        | RealizedTy::Bigint { .. }
        | RealizedTy::Float { .. }
        | RealizedTy::String { .. }
        | RealizedTy::Bool { .. }
        | RealizedTy::Null { .. }
        | RealizedTy::Uint8Array { .. }
        | RealizedTy::Media(_, _)
        | RealizedTy::RustType { .. }
        | RealizedTy::Type { .. }
        | RealizedTy::Resource { .. }
        | RealizedTy::PromptAst { .. }
        | RealizedTy::Void { .. }
        | RealizedTy::BuiltinUnknown { .. }
        | RealizedTy::Never { .. } => 0,
    };
    add(node, nested)
}

fn unsupported_object_message(object: &Object) -> &'static str {
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
