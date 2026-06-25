//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, HeapPermit, PermitProof, TlabHolder};
use ::bex_vm_types::{HeapPtr, Object, ObjectType, RootHaver, Value, ValueKind};
use baml_type::{Literal, Ty};
use bex_external_types::{BexExternalAdt, BexExternalValue, RuntimeTy, UnionMetadata};
use bex_vm::BexVm;

use crate::{BexEngine, EngineError};

// ============================================================================
// VM Value to External Conversion
// ============================================================================

impl BexEngine {
    /// Convert a VM Value to a `BexExternalValue` using the declared type.
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    pub(crate) fn convert_vm_value_to_external_with_type(
        &self,
        value: Value,
        declared_type: &RuntimeTy,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = resolve_effective_type(value, declared_type);

        let external = match value.kind() {
            ValueKind::OmittedArg => {
                return Err(EngineError::TypeMismatch {
                    message: "internal omitted argument escaped to external conversion".to_string(),
                });
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(idx) => {
                self.convert_heap_ptr_to_external_with_type(idx, effective_type, permit)?
            }
        };

        // Wrap in Union if declared type is a union
        maybe_wrap_union(external, declared_type)
    }

    /// Convert an object to a `BexExternalValue` using the effective (non-union) type.
    ///
    /// # Safety
    ///
    /// This method uses unsafe calls to dereference `HeapPtr`. It is safe because:
    /// - We only read objects, never write
    /// - The caller ensures the pointer is valid (from a handle which is a GC root)
    fn convert_heap_ptr_to_external_with_type(
        &self,
        ptr: HeapPtr,
        effective_type: &RuntimeTy,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::Float(f) => Ok(BexExternalValue::Float(*f)),
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // Get element type from declared type, falling back to Null when
                // the declared type doesn't resolve (e.g., builtin class arrays)

                let element_type = match effective_type {
                    RuntimeTy::List(elem_ty, _) => elem_ty.as_ref(),
                    _ => &RuntimeTy::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                };

                // Snapshot under the source's lock; the recursive
                // convert call may lock other containers.
                let snapshot = arr.to_vec();
                let items: Result<Vec<_>, _> = snapshot
                    .iter()
                    .map(|v| self.convert_vm_value_to_external_with_type(*v, element_type, permit))
                    .collect();
                Ok(BexExternalValue::Array {
                    element_type: element_type.clone(),
                    items: items?,
                })
            }

            Object::Map(map) => {
                // Get key and value types from declared type, falling back to
                // Null when the declared type doesn't resolve

                let (key_type, value_type) = match effective_type {
                    RuntimeTy::Map { key, value, .. } => (key.as_ref(), value.as_ref()),
                    _ => (
                        &RuntimeTy::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        &RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                    ),
                };

                let snapshot = map.to_index_map();
                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    snapshot
                        .iter()
                        .map(|(k, v)| {
                            Ok((
                                k.to_string(),
                                self.convert_vm_value_to_external_with_type(
                                    *v, value_type, permit,
                                )?,
                            ))
                        })
                        .collect();
                Ok(BexExternalValue::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries?,
                })
            }

            Object::Instance(instance) => {
                // Get class name and fields from the Class object
                let class_obj = unsafe { instance.class.get() };
                let Object::Class(class) = class_obj else {
                    panic!("Instance.class should point to a Class object")
                };

                // Lift `baml.llm.Stream` to an opaque ADT handle.  The four
                // child fields (_client/_acc/_sse/_cache) stay on the heap
                // behind the GC-rooted handle so the BAML interpreter can
                // walk them when running `Stream.next` / `Stream.final`
                // bodies on subsequent calls.  See plan 21b §"Phase 1a".
                //
                // `ty` is computed from the class FQN + `class_type_args`
                // once at lift time and carried inline on the variant so
                // the wire encoder doesn't need a heap permit. See plan
                // 23a §"Engine-side ripple effects".
                if class.name.display_name().as_str() == "baml.llm.Stream" {
                    let handle = self.heap.create_handle(ptr);
                    let ty = RuntimeTy::Class(
                        class.name.clone(),
                        instance.class_type_args.to_vec(),
                        baml_type::TyAttr::default(),
                    );
                    return Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                        ty,
                        heap_handle: handle,
                    }));
                }

                debug_assert_eq!(
                    class.fields.len(),
                    instance.fields.len(),
                    "Class '{}' has {} fields but instance has {} fields",
                    class.name,
                    class.fields.len(),
                    instance.fields.len(),
                );

                // Read field types directly from the Class object on the heap
                let fields: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    class
                        .fields
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(class_field, slot)| {
                            let value = slot.load();
                            let field_type = class_field
                                .field_template
                                .substitute(&instance.class_type_args);
                            Ok((
                                class_field.name.clone(),
                                self.convert_vm_value_to_external_with_type(
                                    value,
                                    &field_type,
                                    permit,
                                )?,
                            ))
                        })
                        .collect();

                Ok(BexExternalValue::Instance {
                    class_name: class.name.to_string(),
                    type_args: instance.class_type_args.to_vec(),
                    fields: fields?,
                })
            }

            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                let enum_obj = unsafe { variant.enm.get() };
                let Object::Enum(enm) = enum_obj else {
                    panic!("Variant.enm should point to an Enum object")
                };
                let variant_name = enm
                    .variants
                    .get(variant.index)
                    .map(|v| v.name.clone())
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "enum '{}' has {} variants but variant index is {}",
                            enm.name,
                            enm.variants.len(),
                            variant.index,
                        ),
                    })?;
                let enum_name = enm.name.to_string();

                Ok(BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                })
            }

            Object::Function(_) => Err(EngineError::CannotConvert {
                type_name: "function".to_string(),
            }),
            Object::Class(_) => Err(EngineError::CannotConvert {
                type_name: "class".to_string(),
            }),
            Object::Enum(_) => Err(EngineError::CannotConvert {
                type_name: "enum".to_string(),
            }),
            Object::Future(_) => Err(EngineError::CannotConvert {
                type_name: "future".to_string(),
            }),
            Object::UnscheduledFuture(_) => Err(EngineError::CannotConvert {
                type_name: "unscheduled_future".to_string(),
            }),
            Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
            Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
            Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type((**ty).clone()))),
            Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.to_vec())),
            Object::RustData(arc) => Ok(bex_external_types::try_convert_rust_data(arc)
                .unwrap_or_else(|| BexExternalValue::RustData(arc.clone()))),
            Object::Closure(_) => Err(EngineError::CannotConvert {
                type_name: "closure".to_string(),
            }),
            Object::BoundMethod(_) => Err(EngineError::CannotConvert {
                type_name: "bound_method".to_string(),
            }),
            Object::GenericFunction(_) => Err(EngineError::CannotConvert {
                type_name: "generic_function".to_string(),
            }),
            Object::HostClosure(_) => Err(EngineError::CannotConvert {
                type_name: "host_closure".to_string(),
            }),
            Object::Cell(_) => Err(EngineError::CannotConvert {
                type_name: "cell".to_string(),
            }),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Err(EngineError::CannotSnapshot {
                type_name: "sentinel".to_string(),
            }),
        }
    }
}

// ============================================================================
// External to VM Value Conversion
// ============================================================================

impl BexEngine {
    /// Synthesize the concrete `RuntimeTy` an inbound argument *value* inhabits
    /// (01a step 1), for driving call-site generic inference. Pure bottom-up:
    /// with union-arm routing out of scope (00b3 G5), no `expected`-guided
    /// disambiguation is needed. A generic instance's type is read from its
    /// wire-supplied `type_args` (the value-level source of truth); class/enum
    /// `TypeName`s are resolved against the engine registry so the binding
    /// survives Gate B's name match. Opaque host values — and anything else with
    /// no readable BAML type, including empty containers — are `HostOnly`
    /// (Case 2, → `rust_type`).
    pub(crate) fn synth_ty_from_value(&self, value: &BexExternalValue) -> SynthTy {
        let attr = baml_type::TyAttr::default;
        match value {
            BexExternalValue::Int(_) => SynthTy::Known(RuntimeTy::int()),
            BexExternalValue::Bigint(_) => SynthTy::Known(RuntimeTy::Bigint { attr: attr() }),
            BexExternalValue::Float(_) => SynthTy::Known(RuntimeTy::float()),
            BexExternalValue::Bool(_) => SynthTy::Known(RuntimeTy::bool()),
            // A `String` value widens to `string`, never a `Literal` (00b3
            // T2/T45 — `identity("hi")` binds `T = string`).
            BexExternalValue::String(_) => SynthTy::Known(RuntimeTy::string()),
            BexExternalValue::Uint8Array(_) => {
                SynthTy::Known(RuntimeTy::Uint8Array { attr: attr() })
            }
            // A bare `null` carries NO inference evidence (03b §I/§H, rule 4):
            // a `null`-only actual gives the value position no concrete leaf, so
            // we do NOT bind `T = null` (and do NOT null-strip a `T?` formal to
            // bind `T = null`). It rides host-only ⇒ the var defaults to
            // `rust_type` and the value round-trips unchanged (`identity(None)
            // == None`). This is a *runtime* leaf decision; the shared
            // compile-time unifier still binds `null` faithfully.
            BexExternalValue::Null => SynthTy::HostOnly,
            BexExternalValue::Array { items, .. } => {
                // An array always inhabits a list. When its elements carry no
                // evidence (an empty `[]`, or elements that are themselves
                // evidence-free), the element type is the host-only `rust_type`
                // rather than no-evidence — `identity<T>([])` binds
                // `T = rust_type[]`, never leaving `T` for Gate A.
                let elem = synth_collection_element(self, items.iter())
                    .unwrap_or_else(|| RuntimeTy::RustType { attr: attr() });
                SynthTy::Known(RuntimeTy::List(Box::new(elem), attr()))
            }
            BexExternalValue::Map { entries, .. } => {
                // A map always inhabits `map<string, _>`: every wire entry is
                // string-keyed, so the key is always `string`. When the values
                // carry no evidence (a genuinely empty `{}`, or values that are
                // themselves evidence-free), the value type is the host-only
                // `rust_type` — an empty map binds `Map<string, rust_type>`,
                // never leaving the value TypeVar for Gate A.
                let value_ty = synth_collection_element(self, entries.values())
                    .unwrap_or_else(|| RuntimeTy::RustType { attr: attr() });
                SynthTy::Known(RuntimeTy::Map {
                    key: Box::new(RuntimeTy::string()),
                    value: Box::new(value_ty),
                    attr: attr(),
                })
            }
            // A fully-bound generic instance carries its concrete args on the
            // wire (`GenericBox[int]` → `[int]`). An *unbound* generic instance —
            // a generic class whose wire type-args are EMPTY (Pydantic lets you
            // build `GenericBox(value=5)` without `[int]`) — carries no readable
            // BAML type, so it is host-only (03b G2/G3): the var binds `rust_type`
            // and the instance rides opaquely through the `OpaqueExternalValue`
            // carrier, staying distinct from a properly-bound instance (G4). A
            // *non*-generic class legitimately has empty args and types normally.
            // (Where a forcing formal `GenericPair<int,T>` should recover `T` from
            // an unbound instance's FIELDS instead — 03b G1 — that is handled
            // formal-aware in `synth_inference_actual`, not here.) An unresolvable
            // class name is host-only too.
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            } => match self.resolve_class_type_name(class_name) {
                Some(tn) => {
                    if type_args.is_empty() && self.class_generic_arity(class_name) > 0 {
                        SynthTy::HostOnly
                    } else {
                        SynthTy::Known(RuntimeTy::Class(tn, type_args.clone(), attr()))
                    }
                }
                None => SynthTy::HostOnly,
            },
            BexExternalValue::Variant { enum_name, .. } => {
                // A resolvable enum binds its concrete type; an unresolved enum
                // name (no such enum registered) is an opaque carrier with no
                // BAML type ⇒ host-only (`rust_type`), mirroring the unresolved
                // `Instance` arm above.
                match self.resolve_enum_type_name(enum_name) {
                    Some(tn) => SynthTy::Known(RuntimeTy::Enum(tn, attr())),
                    None => SynthTy::HostOnly,
                }
            }
            // A reflected type passed as a value inhabits the `type` metatype.
            BexExternalValue::Adt(BexExternalAdt::Type(_)) => {
                SynthTy::Known(RuntimeTy::Type { attr: attr() })
            }
            // A typed heap handle already carries its concrete `RuntimeTy`.
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, .. }) => {
                SynthTy::Known(ty.clone())
            }
            // Media is a concrete leaf BAML type (`image`/`audio`/...): its kind
            // is readable from the value, so `identity<T>(img)` binds T to the
            // real media type rather than the host-only `rust_type` catch-all.
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
                SynthTy::Known(RuntimeTy::Media(media.kind, attr()))
            }
            // A collector inhabits the concrete `Resource` leaf type, and a
            // rendered prompt inhabits `PromptAst` — bind T to those rather than
            // falling into the host-only catch-all below.
            BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
                SynthTy::Known(RuntimeTy::resource())
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                SynthTy::Known(RuntimeTy::prompt_ast())
            }
            // Peel the FFI union wrapper and synthesize the carried value.
            BexExternalValue::Union { value, .. } => self.synth_ty_from_value(value),
            // Opaque host values (Case 2) and other non-BAML-typed carriers.
            // (Every `Adt` variant is handled explicitly above, so there is no
            // `Adt(_)` fallthrough — a new `BexExternalAdt` variant will surface
            // here as a non-exhaustive-match error, forcing a synth decision.)
            BexExternalValue::Handle(_)
            | BexExternalValue::RustData(_)
            | BexExternalValue::HostValue(_)
            | BexExternalValue::FunctionRef { .. } => SynthTy::HostOnly,
        }
    }

    /// Resolve a class FQN (as it arrives on an inbound `Instance`) to the
    /// engine-registered `TypeName`, so a synthesized `Class` type matches the
    /// declared slot's name in Gate B. `None` if no such class is registered.
    fn resolve_class_type_name(&self, class_name: &str) -> Option<baml_type::TypeName> {
        let ptr = self
            .resolved_class_names
            .get(class_name)
            .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))?;
        // SAFETY: registered class names point to compile-time Class objects.
        match unsafe { ptr.get() } {
            Object::Class(class) => Some(class.name.clone()),
            _ => None,
        }
    }

    /// Resolve an enum FQN to its engine-registered `TypeName`. See
    /// [`Self::resolve_class_type_name`].
    fn resolve_enum_type_name(&self, enum_name: &str) -> Option<baml_type::TypeName> {
        let ptr = self
            .resolved_enum_names
            .get(enum_name)
            .or_else(|| resolve_named_object(&self.resolved_enum_names, enum_name))?;
        // SAFETY: registered enum names point to compile-time Enum objects.
        match unsafe { ptr.get() } {
            Object::Enum(enum_obj) => Some(enum_obj.name.clone()),
            _ => None,
        }
    }

    /// Run `f` against the resolved compile-time [`Object::Class`] for `class_name`.
    fn with_resolved_class<R>(
        &self,
        class_name: &str,
        f: impl FnOnce(&bex_vm_types::Class) -> R,
    ) -> Option<R> {
        let ptr = self
            .resolved_class_names
            .get(class_name)
            .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))?;
        // SAFETY: registered class names point to compile-time Class objects.
        match unsafe { ptr.get() } {
            Object::Class(class) => Some(f(class)),
            _ => None,
        }
    }

    /// The number of class-level generic parameters `class_name` declares, read
    /// from the highest `TypeArgRef(N)` index across its field templates (`N+1`).
    /// `0` for a non-generic class — used to tell an *unbound generic* instance
    /// (empty wire args on a generic class ⇒ host-only) from a plain non-generic
    /// instance (empty args is correct). See the `Instance` synth arm.
    fn class_generic_arity(&self, class_name: &str) -> usize {
        self.with_resolved_class(class_name, |class| {
            class
                .fields
                .iter()
                .filter_map(|field| template_max_type_arg_ref(&field.field_template))
                .max()
                .map_or(0, |max| max as usize + 1)
        })
        .unwrap_or(0)
    }

    /// Reconstruct the type-args of an *unbound* generic instance from its field
    /// VALUES, by synthesizing the value of each field whose template is a direct
    /// `TypeArgRef(N)` into slot `N`. Slots with no directly-typed field stay
    /// `BuiltinUnknown` (the unifier skips them). Returns `None` for a
    /// non-generic / unresolvable class. This is the call-time recovery a forcing
    /// formal needs to bind a var from an unbound instance (03b G1); it does NOT
    /// run for bound instances (they read the wire args) or bare-`T` formals
    /// (those keep the host-only `rust_type` synth).
    fn reconstruct_unbound_instance_args(
        &self,
        class_name: &str,
        fields: &indexmap::IndexMap<String, BexExternalValue>,
    ) -> Option<Vec<RuntimeTy>> {
        let arity = self.class_generic_arity(class_name);
        if arity == 0 {
            return None;
        }
        let unknown = || RuntimeTy::BuiltinUnknown {
            attr: baml_type::TyAttr::default(),
        };
        let mut args: Vec<RuntimeTy> = (0..arity).map(|_| unknown()).collect();
        self.with_resolved_class(class_name, |class| {
            for field in &class.fields {
                let slot = match &field.field_template {
                    baml_type::TyTemplate::TypeArgRef(n)
                    | baml_type::TyTemplate::TypeArgRefOrWildcard(n) => *n as usize,
                    _ => continue,
                };
                if let Some(value) = fields.get(&field.name) {
                    if let Some(arg) = args.get_mut(slot) {
                        *arg = self.synth_ty_from_value(value).into_runtime_ty();
                    }
                }
            }
        })?;
        Some(args)
    }

    /// Synthesize the *actual* `RuntimeTy` for one inference pair, given the
    /// declared `formal`. Almost always this is just [`Self::synth_ty_from_value`].
    /// The one formal-aware exception: an **unbound** generic `Instance` (empty
    /// wire args) met by a generic-**class** formal. The wire carries no
    /// type-args, but the formal directs inference into specific slots, so we
    /// reconstruct the instance's args from its field values (03b G1:
    /// `second_of<T>(p: GenericPair<int,T>)` recovers `T=string`). A bare-`T`
    /// formal is not a `Class`, so it falls through to the host-only `rust_type`
    /// synth (G2/G3), keeping the unbound instance opaque on round-trip.
    pub(crate) fn synth_inference_actual(
        &self,
        formal: &RuntimeTy,
        value: &BexExternalValue,
    ) -> RuntimeTy {
        if let (
            RuntimeTy::Class(..),
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            },
        ) = (formal, value)
        {
            if type_args.is_empty() {
                if let (Some(tn), Some(args)) = (
                    self.resolve_class_type_name(class_name),
                    self.reconstruct_unbound_instance_args(class_name, fields),
                ) {
                    return RuntimeTy::Class(tn, args, baml_type::TyAttr::default());
                }
            }
        }
        self.synth_ty_from_value(value).into_runtime_ty()
    }

    /// Convert a `BexExternalValue` result from sys ops back to a VM Value.
    ///
    /// Returns `EngineError::TypeMismatch` for malformed external values
    /// (unknown class/enum names, missing required fields, …) so that bad
    /// external input — from `--json-args`, language bindings, or buggy
    /// sys ops — surfaces as a graceful error instead of crashing the
    /// process.
    pub(crate) fn convert_external_to_vm_value<T: RootHaver + TlabHolder>(
        &self,
        holder: &mut impl HeapPermit<T>,
        external: BexExternalValue,
    ) -> Result<Value, EngineError> {
        // Default: no declared-type context. Inbound `HostValue` arguments
        // need the declared `RuntimeTy::Function` to materialize an
        // `Object::HostClosure` — callers that thread the type in should
        // use `convert_external_to_vm_value_with_ty`.
        self.convert_external_to_vm_value_with_ty(holder, external, None)
    }

    /// Like [`Self::convert_external_to_vm_value`], but threads the declared
    /// parameter `RuntimeTy` for the top-level value so a `BexExternalValue::HostValue`
    /// can be bound to its function signature as an [`Object::HostClosure`].
    ///
    /// `expected_ty` is honoured only at the top level — nested array
    /// elements / map values / instance fields fall back to the untyped path
    /// (`None`). Adding type-driven element handling here would require
    /// re-traversing the declared `RuntimeTy` in lockstep with the value; we don't
    /// yet support host callables in collection positions, so the
    /// type-context is dropped on entry into containers and any nested
    /// `HostValue` is rejected with `EngineError::CannotConvert`.
    pub(crate) fn convert_external_to_vm_value_with_ty<T: RootHaver + TlabHolder>(
        &self,
        holder: &mut impl HeapPermit<T>,
        external: BexExternalValue,
        expected_ty: Option<&RuntimeTy>,
    ) -> Result<Value, EngineError> {
        // Structural host-only stash (03b §F/§G): when the declared slot resolves
        // to `RustType` (the generic var bound to `rust_type`) but the value is a
        // structural `BexExternalValue` (e.g. an unbound generic instance), ride
        // it through the VM verbatim as an opaque `Object::RustData` instead of
        // materializing an introspectable object. `try_convert_rust_data` re-emits
        // it unchanged on the way out, preserving its identity (an unbound
        // `GenericBox(value=5)` stays != a bound `GenericBox[int]`, G4). `HostValue`
        // keeps its dedicated arm below (it carries a release-keyed handle).
        if expected_ty.and_then(peel_to_rust_type).is_some() && is_structural_host_only(&external) {
            let arc: std::sync::Arc<dyn std::any::Any + Send + Sync> =
                std::sync::Arc::new(bex_external_types::OpaqueExternalValue(external));
            return Ok(Value::object(
                holder.holder_mut().tlab_mut().alloc_rust_data(arc),
            ));
        }
        Ok(match external {
            BexExternalValue::Handle(handle) => Value::object(
                self.resolve_handle(holder.proof(), &handle)
                    .expect("Handle should be valid - object was returned to external code"),
            ),
            BexExternalValue::Null => Value::NULL,
            // The host integer is an `i64`, but the VM integer is `i63` (`Value`
            // reserves the low bit as a tag). Reject values outside the i63
            // range here rather than letting `Value::int` wrap them — a
            // release-build silent truncation, a debug-build panic. This also
            // gates the `bigint → int` FFI narrowing, whose `i64::try_from` only
            // bounds to the full i64 range.
            BexExternalValue::Int(i) => {
                Value::try_int(i).ok_or_else(|| EngineError::TypeMismatch {
                    message: format!(
                        "integer {i} is outside the BAML integer range [{}, {}]",
                        Value::INT_MIN,
                        Value::INT_MAX
                    ),
                })?
            }
            BexExternalValue::Bigint(bi) => {
                // Defense-in-depth: every upstream decoder (FFI hex,
                // SAP, Node.js/Python bridges) already caps oversized
                // bigints, but `Tlab::alloc_bigint` itself is the raw
                // allocator with no bound check. Guard here so any
                // future entry point that bypasses an upstream cap
                // still surfaces a graceful error instead of allocating
                // hundreds of MB on the VM heap.
                let bits = bi.bits();
                if bits > baml_type::MAX_BIGINT_BITS {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "bigint value requires {bits} bits (limit: {})",
                            baml_type::MAX_BIGINT_BITS
                        ),
                    });
                }
                Value::object(holder.holder_mut().tlab_mut().alloc_bigint(bi))
            }
            BexExternalValue::Float(f) => {
                Value::object(holder.holder_mut().tlab_mut().alloc(Object::Float(f)))
            }
            BexExternalValue::Bool(b) => Value::bool(b),
            BexExternalValue::String(s) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_string(s))
            }
            BexExternalValue::Array {
                element_type,
                items,
            } => {
                let values = items
                    .into_iter()
                    .map(|v| self.convert_external_to_vm_value(holder, v))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_array(element_type, values),
                )
            }
            BexExternalValue::Map {
                key_type,
                value_type,
                entries,
            } => {
                let values = entries
                    .into_iter()
                    .map(|(k, v)| {
                        self.convert_external_to_vm_value(holder, v)
                            .map(|v| (bex_vm_types::BexStr::from(k.as_str()), v))
                    })
                    .collect::<Result<indexmap::IndexMap<bex_vm_types::BexStr, Value>, _>>()?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_map(key_type, value_type, values),
                )
            }
            BexExternalValue::Uint8Array(bytes) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_uint8array(bytes))
            }
            BexExternalValue::RustData(data) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(data))
            }
            // Allocate instance by looking up class and converting fields.
            // The wire-supplied class `type_args` are landed into the VM
            // `Object::Instance::class_type_args` via `alloc_instance_with_type_args`.
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            } => {
                let class_ptr = self
                    .resolved_class_names
                    .get(&class_name)
                    .or_else(|| resolve_named_object(&self.resolved_class_names, &class_name))
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown class `{class_name}` in external Instance value"),
                    })?;

                // SAFETY: class_ptr points to a compile-time Class object
                let class_fields = match unsafe { class_ptr.get() } {
                    Object::Class(class) => &class.fields,
                    _ => {
                        return Err(EngineError::TypeMismatch {
                            message: format!(
                                "Resolved name `{class_name}` does not point to a class"
                            ),
                        });
                    }
                };

                // Build field values in the order defined by the class.
                // Each field's declared `RuntimeTy` is passed as the conversion
                // context so type-polymorphic external values (notably
                // `BexExternalValue::HostValue`, which can land in either a
                // function-typed slot as a callable or a `$rust_type` slot
                // as an opaque handle) can branch on the declared shape.
                let mut values = Vec::with_capacity(class_fields.len());
                for class_field in class_fields {
                    let ext = fields.get(&class_field.name).ok_or_else(|| {
                        EngineError::TypeMismatch {
                            message: format!(
                                "Missing field `{}` in external Instance for class `{class_name}`",
                                class_field.name
                            ),
                        }
                    })?;
                    values.push(self.convert_external_to_vm_value_with_ty(
                        holder,
                        ext.clone(),
                        Some(&class_field.field_type),
                    )?);
                }
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_instance_with_type_args(*class_ptr, type_args, values),
                )
            }
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                let enum_ptr = self
                    .resolved_enum_names
                    .get(&enum_name)
                    .or_else(|| resolve_named_object(&self.resolved_enum_names, &enum_name))
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown enum `{enum_name}` in external Variant value"),
                    })?;
                #[allow(unsafe_code)]
                let bex_vm_types::Object::Enum(enum_obj) = (unsafe { enum_ptr.get() }) else {
                    return Err(EngineError::TypeMismatch {
                        message: format!("Resolved name `{enum_name}` does not point to an enum"),
                    });
                };
                let index = enum_obj
                    .variants
                    .iter()
                    .position(|v| v.name == variant_name)
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown variant `{variant_name}` in enum `{enum_name}`"),
                    })?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_variant(*enum_ptr, index),
                )
            }
            BexExternalValue::Union { value, .. } => {
                return self.convert_external_to_vm_value_with_ty(holder, *value, expected_ty);
            }
            BexExternalValue::Adt(BexExternalAdt::Collector(c)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_collector(c))
            }
            BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_type(ty))
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                return Err(EngineError::CannotConvert {
                    type_name: "PromptAst".to_string(),
                });
            }
            BexExternalValue::Adt(BexExternalAdt::Media(arc)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(arc))
            }
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) => {
                Value::object(self.resolve_handle(holder.proof(), &heap_handle).expect(
                    "TaggedHeapHandle should be valid - object was returned to external code",
                ))
            }
            BexExternalValue::FunctionRef { global_index } => {
                // `convert_external_to_vm_value` runs while `holder`'s
                // active heap permit is held, so we route through the
                // permit-proof-gated `SharedGlobals` API. The slice's
                // lifetime is tied to the proof, which is bounded by
                // `holder.proof()`'s borrow of `holder` — both end at the
                // close of this function call.
                let proof = holder.proof();
                let len = self.globals.len(proof);
                let slice = self.globals.as_slice(proof);
                if global_index >= len {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "FunctionRef global_index {global_index} out of \
                             bounds (globals len {len})"
                        ),
                    });
                }
                slice[global_index]
            }
            BexExternalValue::HostValue(arc) => {
                // A `HostValue` lands in one of two declared shapes:
                //
                // - `RuntimeTy::RustType` (opaque `$rust_type` field, e.g. the
                //   `_handle` slot on `baml.errors.HostCallable`): wrap the
                //   arc in `Object::RustData` so the BAML→host decoder can
                //   later downcast it back to a `HostValueArc`. No
                //   function signature involved.
                // - `RuntimeTy::Function` (host callable passed as a function
                //   argument): build a `HostClosure` bound to the declared
                //   signature so the call site can invoke it.
                let ty = expected_ty.ok_or_else(|| EngineError::CannotConvert {
                    type_name: "host_value (no declared type in context)".to_string(),
                })?;
                if matches!(peel_to_rust_type(ty), Some(())) {
                    let dyn_arc: std::sync::Arc<dyn std::any::Any + Send + Sync> = arc;
                    return Ok(Value::object(
                        holder.holder_mut().tlab_mut().alloc_rust_data(dyn_arc),
                    ));
                }
                // Peel through Optional / Union to land on the function type.
                let function_ty =
                    peel_function_ty(ty).ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "host callable cannot be passed where the declared type \
                             is `{ty}`; expected a function type or `$rust_type`",
                        ),
                    })?;
                let (params, ret, throws) = match function_ty {
                    RuntimeTy::Function {
                        params,
                        ret,
                        throws,
                        ..
                    } => (params, ret.as_ref().clone(), throws.as_ref().clone()),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!(
                                "host callable cannot be passed where the declared type \
                                 is `{other}`; expected a function type",
                            ),
                        });
                    }
                };
                // The host's returned value is validated against `ret` when the
                // call completes. A generic return type erases to
                // `BuiltinUnknown` at runtime, which the return validator treats
                // as "accept anything" — letting the host inject a value of any
                // type into a position BAML treats as the instantiated type
                // variable. Reject such a callable at bind time rather than
                // admit an unvalidatable return. (This also rejects a genuine
                // bare `-> void` host callable; such a callable must declare a
                // concrete return type.)
                if ret_ty_has_unvalidatable_position(&ret) {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host callable cannot be bound: its return type `{ret}` is generic \
                             or void, so the host's returned value cannot be validated; host \
                             callables require a concrete return type",
                        ),
                    });
                }
                // `throws` is the callable's declared error contract `E`
                // (`call_host_value<T, E>`). When the parameter pins no
                // concrete error type the throws lowers to a bottom/unit
                // shape: an omitted `throws` becomes `Never` (the function-type
                // lowering's default) and a bare `-> void` throws becomes
                // `Void`. Neither names an error the host is obligated to
                // honor — and the host is foreign code that may surface a
                // native exception regardless (materialized as
                // `baml.errors.HostCallable`). Normalize both to
                // `BuiltinUnknown` so such a throw is accepted opaquely and an
                // in-BAML `catch` can match it, rather than being rejected as a
                // `HostContractViolation`. Concrete throws (e.g.
                // `throws ParseError`) pass through unchanged and stay enforced.
                let normalized_throws = match throws {
                    RuntimeTy::Void { attr } | RuntimeTy::Never { attr } => {
                        RuntimeTy::BuiltinUnknown { attr }
                    }
                    other => other,
                };
                let host_closure = bex_vm_types::HostClosure {
                    handle: arc,
                    ret_ty: Box::new(ret),
                    throws_ty: Box::new(normalized_throws),
                    arity: params.len(),
                    // Capture the declared params (names + optionality) so the VM
                    // can split the call args into positional + supplied-optional
                    // (by name) on dispatch, for the per-bridge argument reshape.
                    params: Box::new(params.clone()),
                };
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc(bex_vm_types::Object::HostClosure(host_closure)),
                )
            }
        })
    }
}

// ============================================================================
// SysOp Argument Conversion
// ============================================================================

/// If `ptr` references an `Object::Float`, return its `BexExternalValue::Float`
/// projection so callers can surface a primitive instead of an opaque handle.
///
/// # Safety
///
/// Caller must hold a GC permit (the `HeapPtr` deref invariant). Mirrors the
/// surrounding accessors that take `PermitProof` and dereference `HeapPtr`.
unsafe fn unbox_float_object(ptr: HeapPtr) -> Option<BexExternalValue> {
    if let Object::Float(f) = unsafe { ptr.get() } {
        Some(BexExternalValue::Float(*f))
    } else {
        None
    }
}

impl BexEngine {
    pub(crate) fn vm_arg_to_bex_value(&self, value: Value) -> BexExternalValue {
        match value.kind() {
            ValueKind::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(ptr) => {
                // SAFETY: caller holds the engine permit (heap is borrowed).
                if let Some(v) = unsafe { unbox_float_object(ptr) } {
                    return v;
                }
                let handle = self.heap.create_handle(ptr);
                BexExternalValue::Handle(handle)
            }
        }
    }

    /// Convert a VM value to a fully owned `BexExternalValue` (deep copy).
    ///
    /// Unlike `vm_arg_to_bex_value` which creates `Handle` references for objects,
    /// this method deep-copies heap objects into standalone values. Use this for
    /// trace event payloads that escape the engine scope (e.g. event collectors).
    pub(crate) fn vm_value_to_owned(
        &self,
        permit: PermitProof<'_>,
        value: Value,
    ) -> BexExternalValue {
        match value.kind() {
            ValueKind::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(ptr) => {
                // SAFETY: `permit` witnesses GC liveness for the deref.
                if let Some(v) = unsafe { unbox_float_object(ptr) } {
                    return v;
                }
                BexValue::HeapPtr(&ptr)
                    .as_owned_for_trace(&self.heap, permit)
                    .unwrap_or_else(|e| {
                        // Remaining errors here (InvalidHandle, TypeMismatch,
                        // FieldNotFound) indicate engine-level invariant
                        // violations — they shouldn't happen in normal operation.
                        // Surface via structured tracing rather than stderr so
                        // they're visible in logs without polluting CLI output,
                        // and embed the error in the trace payload so it shows
                        // up wherever traces are consumed.
                        tracing::error!(error = %e, "trace payload deep-copy failed");
                        BexExternalValue::String(format!("<trace-error: {e}>").into())
                    })
            }
        }
    }

    /// Convert VM values to `BexExternalValues` for sys ops.
    ///
    /// This is simpler than `vm_value_to_external` because sys ops only receive
    /// primitives, strings, arrays, maps, and resources - not instances/variants.
    #[allow(unused)]
    pub(crate) fn vm_args_to_external(vm: &BexVm, args: &[Value]) -> Vec<BexExternalValue> {
        args.iter().map(|v| vm_arg_to_external(vm, *v)).collect()
    }
}

// ============================================================================
// Helper Functions (standalone, no &self needed)
// ============================================================================

/// Wrap a value in Union metadata if the declared type is a union.
pub(crate) fn maybe_wrap_union(
    value: BexExternalValue,
    declared_type: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    match declared_type {
        // A nullable union (`T?` == `T | null`) is optionality, not a tagged
        // union: a null value is bare `Null`, and a single non-null member is
        // unwrapped to its bare value (recursing in case that member is itself a
        // real union). This preserves the pre-desugaring behavior where optional
        // values carried no union metadata.
        RuntimeTy::Union(members, attr) if members.iter().any(RuntimeTy::is_null) => {
            if matches!(value, BexExternalValue::Null) {
                return Ok(BexExternalValue::Null);
            }
            let non_null: Vec<RuntimeTy> =
                members.iter().filter(|m| !m.is_null()).cloned().collect();
            match non_null.len() {
                0 => Ok(value),
                1 => maybe_wrap_union(value, &non_null[0]),
                _ => {
                    // Keep `null` in the recorded union type so the value stays
                    // marked optional (preserving the nullable FFI wire shape);
                    // select the matching non-null arm.
                    let selected = find_matching_member(&value, &non_null)?;
                    let metadata = UnionMetadata::new(
                        RuntimeTy::Union(members.clone(), attr.clone()),
                        selected,
                    );
                    Ok(BexExternalValue::Union {
                        value: Box::new(value),
                        metadata,
                    })
                }
            }
        }
        RuntimeTy::Union(members, _) => {
            let selected = find_matching_member(&value, members)?;
            let metadata = UnionMetadata::new(declared_type.clone(), selected);
            Ok(BexExternalValue::Union {
                value: Box::new(value),
                metadata,
            })
        }
        _ => Ok(value),
    }
}

/// Recover `TypeVar(name) -> concrete` bindings by walking a declared type and
/// the matching concrete type in parallel.
///
/// Used to recover a generic method's class type arguments from the *actual*
/// `self` value at a host call: the declared `self` type still mentions the
/// class's type variables (e.g. `Stream<TStream, TFinal>`), while the inbound
/// receiver carries them concretely (e.g. `Stream<null | string, string>`).
/// Zipping the two yields `{TStream -> null | string, TFinal -> string}`, which
/// [`substitute_type_vars`] then applies to the method's declared return type so
/// the host-return conversion sees concrete arms instead of bare type variables.
pub(crate) fn collect_type_var_bindings(
    declared: &RuntimeTy,
    concrete: &RuntimeTy,
    out: &mut indexmap::IndexMap<String, RuntimeTy>,
) {
    match (declared, concrete) {
        // A type-var position binds to whatever concrete type sits opposite it.
        // First binding wins (a type var should be consistent across positions).
        (RuntimeTy::TypeVar(name, _), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, da, _), RuntimeTy::Class(_, ca, _)) => {
            for (d, c) in da.iter().zip(ca.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::List(d, _), RuntimeTy::List(c, _)) => collect_type_var_bindings(d, c, out),
        (
            RuntimeTy::Map {
                key: dk, value: dv, ..
            },
            RuntimeTy::Map {
                key: ck, value: cv, ..
            },
        ) => {
            collect_type_var_bindings(dk, ck, out);
            collect_type_var_bindings(dv, cv, out);
        }
        (RuntimeTy::Union(dm, _), RuntimeTy::Union(cm, _)) => {
            for (d, c) in dm.iter().zip(cm.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::Future(dv, de, _), RuntimeTy::Future(cv, ce, _)) => {
            collect_type_var_bindings(dv, cv, out);
            collect_type_var_bindings(de, ce, out);
        }
        _ => {}
    }
}

/// Replace `TypeVar` leaves named in `bindings` with their concrete types,
/// recursing through container/aggregate positions. Type variables absent from
/// `bindings` (e.g. a method's own, unbound type params) are left as-is.
///
/// This is the fix for the host-driven streaming `TStream`-typevar bug: a
/// generic method's declared return type (e.g. `Stream.next`'s
/// `TStream | StreamFinished`) reaches the FFI return conversion with `TStream`
/// unsubstituted, so a concrete partial value matched no union member and the
/// conversion panicked. Substituting from the receiver's bound type args (see
/// [`collect_type_var_bindings`]) makes the concrete arm present.
pub(crate) fn substitute_type_vars(
    ty: &RuntimeTy,
    bindings: &indexmap::IndexMap<String, RuntimeTy>,
) -> RuntimeTy {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        RuntimeTy::TypeVar(name, _) => bindings
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        RuntimeTy::Class(tn, args, attr) => RuntimeTy::Class(
            tn.clone(),
            args.iter()
                .map(|a| substitute_type_vars(a, bindings))
                .collect(),
            attr.clone(),
        ),
        RuntimeTy::List(inner, attr) => RuntimeTy::List(
            Box::new(substitute_type_vars(inner, bindings)),
            attr.clone(),
        ),
        RuntimeTy::Map { key, value, attr } => RuntimeTy::Map {
            key: Box::new(substitute_type_vars(key, bindings)),
            value: Box::new(substitute_type_vars(value, bindings)),
            attr: attr.clone(),
        },
        RuntimeTy::Union(members, attr) => RuntimeTy::Union(
            members
                .iter()
                .map(|m| substitute_type_vars(m, bindings))
                .collect(),
            attr.clone(),
        ),
        RuntimeTy::Future(value, error, attr) => RuntimeTy::Future(
            Box::new(substitute_type_vars(value, bindings)),
            Box::new(substitute_type_vars(error, bindings)),
            attr.clone(),
        ),
        RuntimeTy::WatchAccessor(inner, attr) => RuntimeTy::WatchAccessor(
            Box::new(substitute_type_vars(inner, bindings)),
            attr.clone(),
        ),
        // A function-typed parameter (`f: (T) -> R`) carries call type-vars in its
        // params/return/throws. Substitute them so an explicitly-bound closure
        // param materializes against concrete types (J13: `apply[int,int]` binds
        // `f` as `(int) -> int`, not the unvalidatable `(T) -> R`).
        RuntimeTy::Function {
            params,
            ret,
            throws,
            attr,
        } => RuntimeTy::Function {
            params: params
                .iter()
                .map(|p| baml_type::RuntimeFunctionParamTy {
                    name: p.name.clone(),
                    ty: substitute_type_vars(&p.ty, bindings),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(substitute_type_vars(ret, bindings)),
            throws: Box::new(substitute_type_vars(throws, bindings)),
            attr: attr.clone(),
        },
        // Other positions (leaves, opaque handles, Interface/projection) don't
        // carry call type-vars in these paths, so they pass through unchanged.
        _ => ty.clone(),
    }
}

/// Return the name of the first unbound `TypeVar` reachable in `ty` (walking the
/// same aggregate positions [`substitute_type_vars`] does), or `None` if the
/// type is fully concrete. After substituting a generic callee's bindings, any
/// `TypeVar` that remains is unbound — the host failed to supply it. The
/// engine rejects such calls (the wire must be fully bound).
pub(crate) fn first_unbound_type_var(ty: &RuntimeTy) -> Option<String> {
    match ty {
        RuntimeTy::TypeVar(name, _) => Some(name.to_string()),
        RuntimeTy::Class(_, args, _) => args.iter().find_map(first_unbound_type_var),
        RuntimeTy::List(inner, _) | RuntimeTy::WatchAccessor(inner, _) => {
            first_unbound_type_var(inner)
        }
        RuntimeTy::Map { key, value, .. } => {
            first_unbound_type_var(key).or_else(|| first_unbound_type_var(value))
        }
        RuntimeTy::Union(members, _) => members.iter().find_map(first_unbound_type_var),
        RuntimeTy::Future(value, error, _) => {
            first_unbound_type_var(value).or_else(|| first_unbound_type_var(error))
        }
        _ => None,
    }
}

/// Whether `ty` mentions any `TypeVar` (i.e. the callee is generic at this
/// position). Thin wrapper over [`first_unbound_type_var`].
pub(crate) fn contains_type_var(ty: &RuntimeTy) -> bool {
    first_unbound_type_var(ty).is_some()
}

/// The concrete `RuntimeTy` carried by a typed heap-handle argument (e.g. a
/// `Stream` receiver passed as `self`), if any. The handle's `ty` is canonically
/// `Class { name, args }` with the instance's bound type args — the concrete
/// side that [`collect_type_var_bindings`] zips against the declared `self` type.
pub(crate) fn tagged_handle_runtime_ty(value: &BexExternalValue) -> Option<&RuntimeTy> {
    match value {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, .. }) => Some(ty),
        _ => None,
    }
}

// ===========================================================================
// Inbound value-inference (01a/01b). The runtime engine drives call-site
// `TypeVar` solving from argument *values* (synthesized by `synth_ty_from_value`)
// instead of typed expressions. The unifier itself is the SHARED one in
// `baml_type_runtime`: the two seams below widen the engine's `RuntimeTy`
// inputs up to `Ty`, run `infer_value_bindings` / `union_ty`, and narrow each
// result back. This replaces the hand-maintained `RuntimeTy` port that used to
// duplicate the TIR's `infer_bindings_inner` / `union_ty` arm-for-arm (see
// `01c-inbound-inference-reuse.md`). Narrowing never fails: every inferred
// binding is a subterm (or union) of a `RuntimeTy`-derived actual, so it can
// carry no `tir`-axis variant.
// ===========================================================================

/// Combine two types into a normalized union — the `RuntimeTy` seam over the
/// shared [`baml_type_runtime::union_ty`]. Used when the same `TypeVar` is
/// inferred from several arguments (`choose(5, "a")` → `T = int | string`) or a
/// host-only sibling (`choose(5, host_obj)` → `int | rust_type`).
pub(crate) fn union_runtime_ty(a: &RuntimeTy, b: &RuntimeTy) -> RuntimeTy {
    RuntimeTy::try_from(baml_type_runtime::union_ty(&Ty::from(a), &Ty::from(b)))
        .expect("union of runtime types is a runtime type")
}

/// Recover `TypeVar(name) -> concrete` bindings from a `(formal, actual)` pair,
/// **union-merging** repeat bindings of the same variable across calls. The
/// `RuntimeTy` seam over the shared [`baml_type_runtime::infer_value_bindings`]
/// — the runtime variant of the TIR unifier, which binds a `Class` arm only when
/// the formal and actual name the same class.
///
/// Contrast [`collect_type_var_bindings`], the *first-wins* self-receiver path,
/// which is intentionally a separate, simpler walk.
///
/// Superseded at the call seam by [`infer_bindings_runtime_checked`] (which
/// solves all arguments together with variance tracking); retained as a
/// best-effort per-pair primitive exercised by the unit tests below.
#[cfg(test)]
pub(crate) fn infer_bindings_runtime(
    formal: &RuntimeTy,
    actual: &RuntimeTy,
    out: &mut indexmap::IndexMap<String, RuntimeTy>,
) {
    let mut bindings: rustc_hash::FxHashMap<baml_type::Name, Ty> = rustc_hash::FxHashMap::default();
    baml_type_runtime::infer_value_bindings(&Ty::from(formal), &Ty::from(actual), &mut bindings);
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        let Ok(rt) = RuntimeTy::try_from(ty) else {
            continue;
        };
        out.entry(name.to_string())
            .and_modify(|existing| *existing = union_runtime_ty(existing, &rt))
            .or_insert(rt);
    }
}

/// Variance-aware inference over *all* arguments of a generic call at once
/// (`02d`/`02e`). Accumulates every `(formal, actual)` pair into one
/// [`baml_type_runtime::InferenceConstraints`] and solves with variance
/// tracking, so a `TypeVar` used at conflicting variances across arguments —
/// `pair<T>(a: T[], b: T[])` over `int[]`/`string[]`, or `glue<T>(bare: T, arr:
/// T[])` over `int`/`string[]` — is *rejected* (returns `Err`) rather than
/// silently union-merged into an unsound binding. The error string is the
/// solver's "no consistent `T`" reason, surfaced by the caller as a Gate-A
/// `TypeMismatch`.
///
/// Contrast [`infer_bindings_runtime`], the per-argument best-effort merge kept
/// for the self-receiver and callable-summary paths.
pub(crate) fn infer_bindings_runtime_checked(
    pairs: &[(RuntimeTy, RuntimeTy)],
) -> Result<indexmap::IndexMap<String, RuntimeTy>, String> {
    let mut cons = baml_type_runtime::InferenceConstraints::new();
    for (formal, actual) in pairs {
        cons.record(&Ty::from(formal), &Ty::from(actual));
    }
    let bindings = cons.solve().map_err(|e| e.message)?;
    let mut out = indexmap::IndexMap::new();
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        if let Ok(rt) = RuntimeTy::try_from(ty) {
            out.insert(name.to_string(), rt);
        }
    }
    Ok(out)
}

/// Where each `TypeVar` occurs across a generic call's *parameter* types, used
/// to classify still-unbound vars after inference (03c `03c-impl-guide` rules
/// 2 & 4):
///
/// - `value_position` — the var appears in a parameter at a position **not**
///   inside a function-typed sub-term (a bare arg, container element, class/map
///   arg, union member). Such a var, if it came up unbound (empty collection,
///   `null` actual, union-sibling-absorbed, unbound generic), **defaults to
///   `RustType`** and rides opaquely (rule 4).
/// - `closure` — the var appears **inside a function-typed parameter's
///   signature** (its params/return/throws, any depth). A host callable is
///   opaque to BAML, so the var cannot be inferred from it *or* validated
///   against it; it is **poisoned** and must be specified explicitly (rule 2),
///   overriding any value-position evidence it might also have.
///
/// A var in neither set has no parameter occurrence at all (return-only /
/// body-only) ⇒ must-specify (rule 3), left for Gate A.
///
/// `ambiguous_union` is a further must-specify carve-out: a var that is a direct
/// member of a union alongside **another** free `TypeVar` (`f<T,U>(x: T | U |
/// int)`). Such a union has no principled way to split the actual between its
/// free vars, so an unbound one is rejected rather than defaulted to `RustType`
/// (03b J12, distinct from §H's single-`TypeVar`-beside-concrete case).
#[derive(Default)]
pub(crate) struct ParamVarPositions {
    pub value_position: std::collections::HashSet<String>,
    pub closure: std::collections::HashSet<String>,
    pub ambiguous_union: std::collections::HashSet<String>,
}

/// The highest `TypeArgRef(N)` index appearing anywhere in a field template, or
/// `None` if the template references no class type-arg. Used to compute a
/// generic class's arity from its fields.
fn template_max_type_arg_ref(t: &baml_type::TyTemplate) -> Option<u32> {
    use baml_type::TyTemplate as T;
    match t {
        T::Concrete(_) | T::Wildcard => None,
        T::TypeArgRef(n) | T::TypeArgRefOrWildcard(n) => Some(*n),
        T::Array(inner) => template_max_type_arg_ref(inner),
        T::Map(k, v) => template_max_type_arg_ref(k)
            .into_iter()
            .chain(template_max_type_arg_ref(v))
            .max(),
        T::Union(members) => members.iter().filter_map(template_max_type_arg_ref).max(),
        T::Class(_, args) => args.iter().filter_map(template_max_type_arg_ref).max(),
        T::Interface(_, args, assoc) => args
            .iter()
            .chain(assoc.iter().map(|(_, t)| t))
            .filter_map(template_max_type_arg_ref)
            .max(),
    }
}

/// Whether a value is a *structural* host-only carrier — an inline
/// `BexExternalValue` (instance / map / list / variant / bytes) as opposed to an
/// opaque handle, host-value, or already-`RustData`. When such a value lands in
/// a `RustType` slot it is stashed verbatim (`OpaqueExternalValue`) rather than
/// materialized into an introspectable VM object, so it round-trips unchanged
/// (03b G2/G3/G4). Handles / host-values / rust-data already have their own
/// opaque round-trip and are left to their existing arms.
fn is_structural_host_only(value: &BexExternalValue) -> bool {
    matches!(
        value,
        BexExternalValue::Instance { .. }
            | BexExternalValue::Map { .. }
            | BexExternalValue::Array { .. }
            | BexExternalValue::Variant { .. }
            | BexExternalValue::Uint8Array(_)
    )
}

/// Classify every `TypeVar` occurring in the declared parameter types into
/// [`ParamVarPositions`]. See that type's docs for the rule mapping.
pub(crate) fn classify_param_var_positions(params: &[RuntimeTy]) -> ParamVarPositions {
    let mut out = ParamVarPositions::default();
    for p in params {
        walk_var_positions(p, false, &mut out);
    }
    out
}

fn walk_var_positions(ty: &RuntimeTy, in_closure: bool, out: &mut ParamVarPositions) {
    match ty {
        RuntimeTy::TypeVar(name, _) => {
            if in_closure {
                out.closure.insert(name.to_string());
            } else {
                out.value_position.insert(name.to_string());
            }
        }
        // Descending into a function-typed position poisons every var reached
        // through it — params, return, and throws alike.
        RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                walk_var_positions(&p.ty, true, out);
            }
            walk_var_positions(ret, true, out);
            walk_var_positions(throws, true, out);
        }
        RuntimeTy::List(inner, _) | RuntimeTy::WatchAccessor(inner, _) => {
            walk_var_positions(inner, in_closure, out);
        }
        RuntimeTy::Map { key, value, .. } => {
            walk_var_positions(key, in_closure, out);
            walk_var_positions(value, in_closure, out);
        }
        RuntimeTy::Union(members, _) => {
            // A union with ≥2 direct `TypeVar` members is un-inferrable (no
            // principled split). Mark those direct members must-specify so the
            // rule-4 default skips them (03b J12).
            let direct_tvs: Vec<&str> = members
                .iter()
                .filter_map(|m| match m {
                    RuntimeTy::TypeVar(name, _) => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            if direct_tvs.len() >= 2 {
                for name in &direct_tvs {
                    out.ambiguous_union.insert(name.to_string());
                }
            }
            for m in members {
                walk_var_positions(m, in_closure, out);
            }
        }
        RuntimeTy::Class(_, args, _) => {
            for a in args {
                walk_var_positions(a, in_closure, out);
            }
        }
        RuntimeTy::Future(value, error, _) => {
            walk_var_positions(value, in_closure, out);
            walk_var_positions(error, in_closure, out);
        }
        _ => {}
    }
}

/// The outcome of synthesizing a `RuntimeTy` from a runtime argument value —
/// the value→type bridge the TIR never needs (it sees typed expressions, not
/// values). Every value yields a `RuntimeTy`: a concrete BAML type when one can
/// be read, else the host-only `rust_type` fallback.
pub(crate) enum SynthTy {
    /// A concrete BAML type was synthesized (Case 1).
    Known(RuntimeTy),
    /// An opaque host value with no BAML type (Case 2) — binds `rust_type` and
    /// rides through as an `Object::RustData` heap handle.
    HostOnly,
}

impl SynthTy {
    /// The `RuntimeTy` to feed [`infer_bindings_runtime`]. `HostOnly` lowers to
    /// `RustType` (the recommended host-only default — round-trips as
    /// `Object::RustData` with no materializer change; see
    /// `convert_external_to_vm_value_with_ty`).
    pub(crate) fn into_runtime_ty(self) -> RuntimeTy {
        match self {
            SynthTy::Known(ty) => ty,
            SynthTy::HostOnly => RuntimeTy::RustType {
                attr: baml_type::TyAttr::default(),
            },
        }
    }
}

/// Union-fold the element/value synths of a collection into a single element
/// type. `None` only when the collection is empty (every value synthesizes to a
/// concrete type or the host-only `rust_type`, so non-empty collections always
/// yield evidence).
fn synth_collection_element<'a>(
    engine: &BexEngine,
    items: impl Iterator<Item = &'a BexExternalValue>,
) -> Option<RuntimeTy> {
    let mut acc: Option<RuntimeTy> = None;
    for item in items {
        let ty = engine.synth_ty_from_value(item).into_runtime_ty();
        acc = Some(match acc {
            None => ty,
            Some(prev) => union_runtime_ty(&prev, &ty),
        });
    }
    acc
}

/// Peel `Optional` and singleton-`Union` wrappers off `ty`, returning the
/// underlying `RuntimeTy::Function` if there is one. Returns `None` if the type is
/// not a function (after peeling).
///
/// Used by `convert_external_to_vm_value_with_ty` to find the function
/// signature behind, e.g., `(int) -> int` and `((int) -> int)?` so that an
/// inbound `BexExternalValue::HostValue` can be bound to it as an
/// `Object::HostClosure`.
/// Returns `Some(())` if `ty` is the runtime representation of
/// `$rust_type` — i.e. `RuntimeTy::RustType` — possibly
/// wrapped in a `Union` (the post-`RuntimeTy::Optional`-removal encoding of
/// `T?` is `RuntimeTy::Union([T, Null], _)`, so nullable forms flow through
/// the union arm). Mirrors [`peel_function_ty`] for the `$rust_type`
/// field shape that a `HostValue` argument can land in.
pub(crate) fn peel_to_rust_type(ty: &RuntimeTy) -> Option<()> {
    if matches!(ty, RuntimeTy::RustType { .. }) {
        return Some(());
    }
    match ty {
        RuntimeTy::Union(members, _) => {
            let mut found = false;
            for m in members {
                if peel_to_rust_type(m).is_some() {
                    if found {
                        return None;
                    }
                    found = true;
                }
            }
            if found { Some(()) } else { None }
        }
        _ => None,
    }
}

pub(crate) fn peel_function_ty(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    match ty {
        RuntimeTy::Function { .. } => Some(ty),
        RuntimeTy::Union(members, _) => {
            // Find the single function member, if any. If there are multiple
            // function members or none, we can't pick deterministically.
            let mut found: Option<&RuntimeTy> = None;
            for m in members {
                if let Some(f) = peel_function_ty(m) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(f);
                }
            }
            found
        }
        _ => None,
    }
}

/// Whether a host-callable's declared return type contains a position the
/// host-return validator treats as "accept anything": a `RuntimeTy::Void` (the runtime
/// form of an erased generic type variable, and also a bare `-> void`) or a
/// `RuntimeTy::BuiltinUnknown`. Recurses through `Optional` / `List` / `Map`-value /
/// `Union` / `Class`-generic-args so a nested erased position (`(T)[]`,
/// `Box<T>`) is caught too. A host callable with such a return type cannot have
/// its returned value validated, so binding one is rejected.
fn ret_ty_has_unvalidatable_position(ty: &RuntimeTy) -> bool {
    match ty {
        // Unvalidatable: the host's opaque returned value cannot be checked
        // against these declared types (the host-return validator has no
        // positive discriminator for them), so a host could inject a value that
        // violates the declared type. Reject binding such a callable.
        //   - `Void`/`BuiltinUnknown`: accept-anything tops.
        //   - `TypeVar`/`AssociatedTypeProjection`: faithful (un-erased) generic
        //     positions whose instantiation can't be validated.
        //   - `Interface`: implementation can't be checked at the FFI boundary.
        //   - `EnumVariant`: a single variant can't be checked (the validator
        //     only checks enum identity).
        //   - `Future`: the host cannot produce a VM future, and nothing
        //     validates one.
        RuntimeTy::Void { .. }
        | RuntimeTy::BuiltinUnknown { .. }
        | RuntimeTy::TypeVar(..)
        | RuntimeTy::AssociatedTypeProjection { .. }
        | RuntimeTy::Interface(..)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::Future(..) => true,

        // Container positions are validated structurally; recurse so a nested
        // unvalidatable position (`(T)[]`, `Box<T>`, `int | T`) is caught too.
        RuntimeTy::List(elem, _) => ret_ty_has_unvalidatable_position(elem),
        RuntimeTy::Map { value, .. } => ret_ty_has_unvalidatable_position(value),
        RuntimeTy::Union(members, _) => members.iter().any(ret_ty_has_unvalidatable_position),
        RuntimeTy::Class(_, generic_args, _) => {
            generic_args.iter().any(ret_ty_has_unvalidatable_position)
        }

        // Directly validated by the host-return validator.
        RuntimeTy::Null { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Int { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::Bigint { .. }
        | RuntimeTy::String { .. }
        | RuntimeTy::Uint8Array { .. }
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(..)
        | RuntimeTy::Media(..)
        | RuntimeTy::Function { .. } => false,

        // Opaque runtime handles: the declared type is itself opaque, so the
        // host's value has no concrete contract to violate. (Most cannot be a
        // host-callable return type in practice; an unexpanded `TypeAlias` here
        // would be a prior-stage bug, and `-> never` is a callable that only
        // ever throws.)
        RuntimeTy::RustType { .. }
        | RuntimeTy::Type { .. }
        | RuntimeTy::Resource { .. }
        | RuntimeTy::PromptAst { .. }
        | RuntimeTy::WatchAccessor(..)
        | RuntimeTy::TypeAlias(..)
        | RuntimeTy::Never { .. } => false,
    }
}

/// Find which union member matches a value.
///
/// `BuiltinUnknown` arms match any value (see `value_matches_type`) and are
/// considered last so a more-specific arm wins. This keeps the union
/// metadata's `selected_option` faithful when concrete arms (e.g.
/// `StreamFinished` in `BuiltinUnknown | StreamFinished`) actually fit.
fn find_matching_member(
    value: &BexExternalValue,
    members: &[RuntimeTy],
) -> Result<RuntimeTy, EngineError> {
    for member in members {
        if !matches!(member, RuntimeTy::BuiltinUnknown { .. }) && value_matches_type(value, member)
        {
            return Ok(member.clone());
        }
    }
    for member in members {
        if matches!(member, RuntimeTy::BuiltinUnknown { .. }) {
            return Ok(member.clone());
        }
    }
    // This indicates a type system inconsistency - the value should match one of the members
    Err(EngineError::TypeMismatch {
        message: format!(
            "Value of type '{}' does not match any member of union {:?}",
            value.type_name(),
            members
        ),
    })
}

fn type_name_matches_external_name(external_name: &str, type_name: &baml_type::TypeName) -> bool {
    external_name == type_name.display_name().as_str()
        || external_name == type_name.render_dotted(false)
}

fn resolve_named_object<'a>(
    objects: &'a indexmap::IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<&'a HeapPtr> {
    // Direct hit (engine FQN, e.g., "user.lorem.MyLorem" or
    // "baml.http.Response").
    if let Some(found) = objects.get(name) {
        return Some(found);
    }
    // MIR uses `display_name`, which strips the `user.` prefix for
    // user-package types, so an `Instance` arriving
    // from `coerce_arg_to_declared_type` may carry `lorem.MyLorem` while
    // the engine registered `user.lorem.MyLorem`. Try the `user.`
    // prefix as a fallback before giving up. Builtin/vendor types keep
    // their full FQN so this only fires for user-package classes.
    let user_qualified = format!("user.{name}");
    if let Some(found) = objects.get(&user_qualified) {
        return Some(found);
    }
    None
}

/// Check if a value matches a declared type.
fn value_matches_type(value: &BexExternalValue, ty: &RuntimeTy) -> bool {
    match (value, ty) {
        // `BuiltinUnknown` is the engine's "any value matches" sentinel
        // (TypeScript `unknown` semantics — see `baml_type::RuntimeTy::BuiltinUnknown`).
        // Used by the stdlib generics hardcode in `baml_compiler2_mir::lower`
        // so e.g. `Stream<TStream, TFinal>.next() -> TStream | StreamFinished`
        // accepts any partial-stream payload as the `TStream` arm.
        (_, RuntimeTy::BuiltinUnknown { .. }) => true,
        (BexExternalValue::Null, RuntimeTy::Null { .. }) => true,
        (BexExternalValue::Int(_), RuntimeTy::Int { .. }) => true,
        (BexExternalValue::Bigint(_), RuntimeTy::Bigint { .. }) => true,
        (BexExternalValue::Float(_), RuntimeTy::Float { .. }) => true,
        (BexExternalValue::Bool(_), RuntimeTy::Bool { .. }) => true,
        (BexExternalValue::String(_), RuntimeTy::String { .. }) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(_), RuntimeTy::Literal(Literal::Int(_), _, _)) => true,
        (BexExternalValue::Bigint(_), RuntimeTy::Literal(Literal::Bigint(_), _, _)) => true,
        (BexExternalValue::Float(_), RuntimeTy::Literal(Literal::Float(_), _, _)) => true,
        (BexExternalValue::Uint8Array(_), RuntimeTy::Uint8Array { .. }) => true,
        (BexExternalValue::String(_), RuntimeTy::Literal(Literal::String(_), _, _)) => true,
        (BexExternalValue::Bool(_), RuntimeTy::Literal(Literal::Bool(_), _, _)) => true,
        (BexExternalValue::Array { .. }, RuntimeTy::List(_, _)) => true,
        (BexExternalValue::Map { .. }, RuntimeTy::Map { .. }) => true,
        // A host-encoded object arrives as a bare `Map` (the JS encoder emits
        // every non-builtin object as `map_value`, no FQN), so a `Map`
        // matches a `Class` slot at the FFI boundary — it is promoted to an
        // `Instance` during materialization. This lets a host-built class
        // value satisfy a union's class member (e.g. `T | string`).
        (BexExternalValue::Map { .. }, RuntimeTy::Class(..)) => true,
        // `BexExternalValue::Instance` now carries its wire-supplied class
        // type args, so we can disambiguate `Foo<int>` from `Foo<string>` at
        // the FFI boundary instead of name-only. The comparison is name-only
        // where the *declared* `Class`'s args are absent/erased (a non-generic
        // class, or args still bare TypeVars); against a concrete generic slot
        // the wire instance must supply matching args — see
        // `class_type_args_compatible`.
        (
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            },
            RuntimeTy::Class(tn, expected_args, _),
        ) => {
            type_name_matches_external_name(class_name, tn)
                && class_type_args_compatible(type_args, expected_args)
        }
        (BexExternalValue::Variant { enum_name, .. }, RuntimeTy::Enum(tn, _)) => {
            type_name_matches_external_name(enum_name, tn)
        }
        (BexExternalValue::Adt(BexExternalAdt::Collector(_)), _) => false,
        (BexExternalValue::Adt(BexExternalAdt::Type(_)), RuntimeTy::Type { .. }) => true,
        (BexExternalValue::Union { value, .. }, ty) => value_matches_type(value, ty),
        // Handle nested unions (including nullable `T | null`) in the type.
        (value, RuntimeTy::Union(members, _)) => {
            members.iter().any(|m| value_matches_type(value, m))
        }
        _ => false,
    }
}

/// Compare a generic instance's wire-supplied class type args against a declared
/// `Class`'s args at the FFI boundary. Strict where the declared type is
/// *concrete*: a generic instance must arrive fully bound (Phase 2/3), so an
/// instance that omits the args a concrete generic slot requires — or whose
/// arity disagrees — is a positive mismatch, not a shape surprise to wave
/// through.
///
/// - `expected_args` empty → compatible: the declared class is non-generic, so
///   there is nothing to check (fall back to name-only).
/// - `wire_args` empty against a non-empty `expected_args`: reject only if the
///   expected args are concrete. If they are still erased/unconcretized
///   wildcards (`TypeVar`/`BuiltinUnknown` — e.g. an instance method's class
///   param that couldn't be bound, lowered to runtime `unknown`), there is
///   nothing concrete to contradict, so stay lenient.
/// - differing (non-zero) arity → reject.
/// - per-arg: an expected `TypeVar`/`BuiltinUnknown` is a wildcard; otherwise the
///   wire arg must be compatible with the expected arg.
fn class_type_args_compatible(wire_args: &[RuntimeTy], expected_args: &[RuntimeTy]) -> bool {
    if expected_args.is_empty() {
        return true;
    }
    if wire_args.is_empty() {
        return expected_args.iter().all(is_wildcard_ty);
    }
    if wire_args.len() != expected_args.len() {
        return false;
    }
    wire_args
        .iter()
        .zip(expected_args)
        .all(|(wire, expected)| runtime_ty_compatible(wire, expected))
}

/// A type-arg position that imposes no concrete constraint: an unsubstituted
/// `TypeVar` or the `unknown` sentinel. Such a position can't positively
/// contradict a wire arg, so the structural matcher treats it as a wildcard.
fn is_wildcard_ty(ty: &RuntimeTy) -> bool {
    matches!(
        ty,
        RuntimeTy::TypeVar(..) | RuntimeTy::BuiltinUnknown { .. }
    )
}

/// Structural compatibility of a wire-supplied type against a declared
/// (substituted) type, used to disambiguate generic instances at the FFI
/// boundary without depending on `TyAttr` equality or full subtyping. Lenient:
/// only a *positive* leaf/shape mismatch returns `false`.
///
/// - an expected `TypeVar`/`BuiltinUnknown` is a wildcard;
/// - two primitives must be the same primitive (this is what separates
///   `Foo<int>` from `Foo<string>`);
/// - matching containers recurse; class names must match, then args recurse;
/// - anything else is treated as compatible (don't over-reject).
fn runtime_ty_compatible(wire: &RuntimeTy, expected: &RuntimeTy) -> bool {
    use RuntimeTy as T;
    match (wire, expected) {
        (_, T::TypeVar(..) | T::BuiltinUnknown { .. }) => true,
        (T::TypeVar(..) | T::BuiltinUnknown { .. }, _) => true,
        (T::Int { .. }, T::Int { .. })
        | (T::String { .. }, T::String { .. })
        | (T::Bool { .. }, T::Bool { .. })
        | (T::Float { .. }, T::Float { .. })
        | (T::Null { .. }, T::Null { .. })
        | (T::Bigint { .. }, T::Bigint { .. })
        | (T::Uint8Array { .. }, T::Uint8Array { .. }) => true,
        // Distinct primitives: a positive mismatch.
        (
            T::Int { .. }
            | T::String { .. }
            | T::Bool { .. }
            | T::Float { .. }
            | T::Null { .. }
            | T::Bigint { .. }
            | T::Uint8Array { .. },
            T::Int { .. }
            | T::String { .. }
            | T::Bool { .. }
            | T::Float { .. }
            | T::Null { .. }
            | T::Bigint { .. }
            | T::Uint8Array { .. },
        ) => false,
        (T::List(w, _), T::List(e, _)) => runtime_ty_compatible(w, e),
        (
            T::Map {
                key: wk, value: wv, ..
            },
            T::Map {
                key: ek, value: ev, ..
            },
        ) => runtime_ty_compatible(wk, ek) && runtime_ty_compatible(wv, ev),
        (T::Class(wn, wa, _), T::Class(en, ea, _)) => {
            wn == en && class_type_args_compatible(wa, ea)
        }
        // Everything else (unions, enums, aliases, opaque, mixed kinds) is
        // treated leniently as compatible.
        _ => true,
    }
}

/// Structurally check a generic call's **instance** argument against its
/// now-concrete expected parameter type, returning a human-readable error on a
/// positive mismatch. Scoped to `BexExternalValue::Instance` — the value kind
/// whose wire-supplied `type_args` carry generic information that the signature
/// can check (e.g. a `GenericBox<string>` arriving at a `GenericBox<int>`
/// param). Opaque handles/Adts/host values and bare maps keep the permissive
/// path (their types aren't comparable at this boundary). Also rejects an
/// instance smuggling an unbound `TypeVar` in its wire args (the wire must be
/// fully bound).
/// `caller_specified_types` selects the mode (see the call site): in explicit /
/// partial mode (`true`) the wire must be fully bound, so an *unbound* instance
/// (empty wire args) is rejected; in pure-inference mode (`false`) BAML has
/// already recovered such an instance's args from its field values (03b G1), so
/// the missing wire args are admitted.
pub(crate) fn check_generic_arg(
    value: &BexExternalValue,
    expected: &RuntimeTy,
    caller_specified_types: bool,
) -> Result<(), String> {
    let BexExternalValue::Instance { type_args, .. } = value else {
        return Ok(());
    };
    if let Some(name) = type_args.iter().find_map(first_unbound_type_var) {
        return Err(format!(
            "generic argument carries an unbound type variable `{name}`; the host must \
             send fully-bound generic instances"
        ));
    }
    // Pure-inference mode: an *unbound* generic instance (no wire args) was
    // already reconstructed from its fields and consumed by inference (G1).
    // Admit it rather than demanding wire args the inference path doesn't need.
    if !caller_specified_types && type_args.is_empty() {
        return Ok(());
    }
    // Compare against the expected type, peeling optional/union wrappers so an
    // instance bound for a `T?`/`T | ...` slot still validates against the
    // class member. Lenient where the expected type isn't a concrete class.
    if expected_admits_instance(value, expected) {
        Ok(())
    } else {
        Err(format!(
            "argument of runtime type `{}` does not satisfy the expected type `{expected:?}`",
            value.type_name(),
        ))
    }
}

/// Whether an `Instance` value is admitted by `expected`, peeling `Union`
/// (covers `Optional`, which lowers to `T | null`) so a class instance matches
/// its union/optional member. Defers the actual class+args comparison to
/// [`value_matches_type`].
fn expected_admits_instance(value: &BexExternalValue, expected: &RuntimeTy) -> bool {
    match expected {
        RuntimeTy::Union(members, _) => members.iter().any(|m| expected_admits_instance(value, m)),
        // Only a `Class` slot can definitively reject an instance; against any
        // other expected shape (TypeVar/unknown/opaque) stay lenient.
        RuntimeTy::Class(..) => value_matches_type(value, expected),
        _ => true,
    }
}

impl BexEngine {
    /// Schema-aware strict validation of a host-callable's returned value
    /// against its declared return type.
    ///
    /// This is the engine-side complement to the bridges' shared
    /// `bex_external_types::validate_host_return` guard. The shared guard runs
    /// at the FFI boundary and enforces everything checkable without a schema
    /// (scalar discrimination including `int` ≠ `float`, container recursion,
    /// enum identity, class-*name* identity). This method adds the one check
    /// the shared guard cannot perform — class *field types* — by resolving
    /// the declared class against the engine's compiled schema
    /// (`resolved_class_names`) and recursively validating each declared
    /// field's value against its instantiated field type.
    ///
    /// Returns `Err(message)` describing the first mismatch; the caller maps
    /// it to a `VmBamlError::HostCallable` so it surfaces as a catchable
    /// `root.errors.HostCallable`.
    pub(crate) fn validate_host_return_schema(
        &self,
        value: &BexExternalValue,
        expected: &RuntimeTy,
    ) -> Result<(), String> {
        match expected {
            // `unknown` / opaque-any: accept (defensive — concrete at the FFI
            // boundary).
            RuntimeTy::BuiltinUnknown { .. } => Ok(()),

            // Union (including nullable `T | null`): must satisfy at least one
            // member (schema-aware).
            RuntimeTy::Union(members, _) => {
                let inner = match value {
                    BexExternalValue::Union { value: inner, .. } => inner.as_ref(),
                    other => other,
                };
                if members
                    .iter()
                    .any(|m| self.validate_host_return_schema(inner, m).is_ok())
                {
                    Ok(())
                } else {
                    Err(format!(
                        "host callable returned a value of type `{}` that does not match the \
                         declared return type `{expected}`",
                        inner.type_name(),
                    ))
                }
            }

            // A `Union`-wrapped value against a non-union declared type:
            // validate the inner value.
            _ if matches!(value, BexExternalValue::Union { .. }) => {
                let BexExternalValue::Union { value: inner, .. } = value else {
                    unreachable!("guarded by the matches! above")
                };
                self.validate_host_return_schema(inner, expected)
            }

            RuntimeTy::List(inner, _) => match value {
                BexExternalValue::Array { items, .. } => {
                    for item in items {
                        self.validate_host_return_schema(item, inner)?;
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where a list was declared",
                    other.type_name(),
                )),
            },

            RuntimeTy::Map { value: v_ty, .. } => match value {
                BexExternalValue::Map { entries, .. } => {
                    for v in entries.values() {
                        self.validate_host_return_schema(v, v_ty)?;
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where a map was declared",
                    other.type_name(),
                )),
            },

            // The schema-aware part: validate each declared field's value
            // against its declared field type. A bare `Map` does NOT satisfy a
            // class type here: result materialization is value-driven, so a
            // `Map` becomes an `Object::Map`, never an instance of the declared
            // class — accepting it would hand back a value that cannot inhabit
            // the declared return type. A host returning a class must encode it
            // as a class value (→ `Instance`), not a plain map.
            RuntimeTy::Class(tn, expected_args, _) => match value {
                BexExternalValue::Instance {
                    class_name, fields, ..
                } => {
                    if !type_name_matches_external_name(class_name, tn) {
                        return Err(format!(
                            "host callable returned an instance of `{class_name}` where class \
                             `{tn}` was declared",
                        ));
                    }
                    let Some(class_ptr) = self
                        .resolved_class_names
                        .get(class_name)
                        .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))
                    else {
                        // Unknown class: leave it to the engine's
                        // `convert_external_to_vm_value`, which errors with a
                        // clear "Unknown class" message.
                        return Ok(());
                    };
                    // SAFETY: class_ptr points to a compile-time Class object
                    // (a GC root for the program's lifetime).
                    #[expect(
                        unsafe_code,
                        reason = "reading a compile-time Class object via its GC-rooted pointer"
                    )]
                    let Object::Class(class) = (unsafe { class_ptr.get() }) else {
                        return Ok(());
                    };
                    for class_field in &class.fields {
                        if let Some(field_value) = fields.get(&class_field.name) {
                            let field_ty = class_field.field_template.substitute(expected_args);
                            self.validate_host_return_schema(field_value, &field_ty)?;
                        }
                        // Missing fields are reported by
                        // `convert_external_to_vm_value` at materialization.
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where class `{tn}` was declared",
                    other.type_name(),
                )),
            },

            // Enum identity: a `Variant` must name the declared enum, and the
            // variant must exist on that enum (the latter is also enforced by
            // `convert_external_to_vm_value`).
            RuntimeTy::Enum(tn, _) => match value {
                BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                } => {
                    if !type_name_matches_external_name(enum_name, tn) {
                        return Err(format!(
                            "host callable returned a variant of enum `{enum_name}` where enum \
                             `{tn}` was declared",
                        ));
                    }
                    if let Some(enum_ptr) = self
                        .resolved_enum_names
                        .get(enum_name)
                        .or_else(|| resolve_named_object(&self.resolved_enum_names, enum_name))
                    {
                        #[expect(
                            unsafe_code,
                            reason = "reading a compile-time Enum object via its GC-rooted pointer"
                        )]
                        if let Object::Enum(enum_obj) = unsafe { enum_ptr.get() } {
                            if !enum_obj.variants.iter().any(|v| &v.name == variant_name) {
                                return Err(format!(
                                    "host callable returned unknown variant `{variant_name}` of \
                                     enum `{enum_name}`",
                                ));
                            }
                        }
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where enum `{tn}` was declared",
                    other.type_name(),
                )),
            },

            // A function-typed return position is not supported yet. The host
            // call result is materialized by `convert_external_to_vm_value`
            // *without* a declared type, so a returned `HostValue` (the only
            // value that inhabits a function type) cannot be bound to an
            // `Object::HostClosure` and would otherwise fail downstream as a raw
            // `EngineError::CannotConvert`. Reject it here so it surfaces as a
            // structured, catchable `HostCallable` instead. This arm covers a
            // top-level function return and — via the `List` / `Map` / `Class`
            // recursion above — any nested function position (`(() -> int)[]`,
            // `class { cb: () -> int }`, …).
            RuntimeTy::Function { .. } => Err(format!(
                "host callable returned a value typed `{expected}`; returning a \
                 callable (a function-typed value) is not supported",
            )),

            // Scalars and everything else: defer to the schema-free shape
            // check (int ≠ float, exact tags, literal equality, media).
            _ => {
                bex_external_types::validate_host_return(value, expected).map_err(|e| e.to_string())
            }
        }
    }
}

/// For union types, find which member matches the actual runtime value.
///
/// If the declared type is not a union, returns it unchanged.
fn resolve_effective_type(value: Value, declared_type: &RuntimeTy) -> &RuntimeTy {
    match declared_type {
        RuntimeTy::Union(members, _) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        _ => declared_type,
    }
}

/// Find the union member that matches the runtime value's type.
fn find_matching_union_member(value: Value, members: &[RuntimeTy]) -> Option<&RuntimeTy> {
    match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => members.iter().find(|m| matches!(m, RuntimeTy::Null { .. })),
        ValueKind::Int(_) => members.iter().find(|m| {
            matches!(
                m,
                RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _)
            )
        }),
        ValueKind::Bool(_) => members.iter().find(|m| {
            matches!(
                m,
                RuntimeTy::Bool { .. } | RuntimeTy::Literal(Literal::Bool(_), _, _)
            )
        }),
        ValueKind::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(_) => members.iter().find(|m| {
                    matches!(
                        m,
                        RuntimeTy::Float { .. } | RuntimeTy::Literal(Literal::Float(_), _, _)
                    )
                }),
                Object::String(_) => members.iter().find(|m| {
                    matches!(
                        m,
                        RuntimeTy::String { .. } | RuntimeTy::Literal(Literal::String(_), _, _)
                    )
                }),
                Object::Instance(inst) => {
                    let class_obj = unsafe { inst.class.get() };
                    if let Object::Class(class) = class_obj {
                        // Compare both class name and type args.  When the
                        // union member has empty type args (e.g. a bare `Foo`)
                        // it matches any instance of `Foo` regardless of its
                        // class_type_args — preserving first-match semantics
                        // for `Foo<int> | Foo`.  When type args are present
                        // on the union member they must equal the instance's
                        // class_type_args exactly.
                        members.iter().find(|m| {
                            matches!(m, RuntimeTy::Class(tn, expected_args, _)
                                if *tn == class.name
                                && (expected_args.is_empty()
                                    || expected_args[..] == inst.class_type_args[..]))
                        })
                    } else {
                        None
                    }
                }
                Object::Variant(variant) => {
                    let enum_obj = unsafe { variant.enm.get() };
                    if let Object::Enum(enm) = enum_obj {
                        members
                            .iter()
                            .find(|m| matches!(m, RuntimeTy::Enum(tn, _) if *tn == enm.name))
                    } else {
                        None
                    }
                }
                Object::Array(elements) => {
                    // For arrays, check first element to determine which List type
                    if let Some(first) = elements.get(0) {
                        members.iter().find(|m| {
                            if let RuntimeTy::List(elem_ty, _) = m {
                                find_matching_union_member(first, &[elem_ty.as_ref().clone()])
                                    .is_some()
                            } else {
                                false
                            }
                        })
                    } else {
                        // Empty array - match any List type
                        members.iter().find(|m| matches!(m, RuntimeTy::List(_, _)))
                    }
                }
                Object::Map(_) => members.iter().find(|m| matches!(m, RuntimeTy::Map { .. })),
                Object::Uint8Array(_) => members
                    .iter()
                    .find(|m| matches!(m, RuntimeTy::Uint8Array { .. })),
                Object::Bigint(_) => members.iter().find(|m| {
                    matches!(
                        m,
                        RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _)
                    )
                }),
                // Types that don't participate in union discrimination.
                Object::Function(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::GenericFunction(_)
                | Object::HostClosure(_)
                | Object::Cell(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::Future(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Type(_) => None,
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => None,
            }
        }
    }
}

/// Convert a VM value to a `BexExternalValue` for sys op arguments.
///
/// This is simpler than `vm_value_to_external` because sys ops only receive
/// primitives, strings, arrays, maps, and resources - not instances/variants.
pub(crate) fn vm_arg_to_external(vm: &BexVm, value: Value) -> BexExternalValue {
    match value.kind() {
        ValueKind::OmittedArg => {
            panic!("Cannot convert omitted argument sentinel to BexExternalValue")
        }
        ValueKind::Null => BexExternalValue::Null,
        ValueKind::Int(i) => BexExternalValue::Int(i),
        ValueKind::Bool(b) => BexExternalValue::Bool(b),
        ValueKind::Object(idx) => {
            let obj = vm.get_object(idx);
            match obj {
                Object::Float(f) => BexExternalValue::Float(*f),
                Object::String(s) => BexExternalValue::String(s.clone()),
                Object::Array(arr) => {
                    let snap = arr.to_vec();
                    let items: Vec<BexExternalValue> =
                        snap.iter().map(|v| vm_arg_to_external(vm, *v)).collect();
                    BexExternalValue::Array {
                        element_type: bex_external_types::RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        items,
                    }
                }
                Object::Map(map) => {
                    let snap = map.to_index_map();
                    let entries: indexmap::IndexMap<String, BexExternalValue> = snap
                        .iter()
                        .map(|(k, v)| (k.to_string(), vm_arg_to_external(vm, *v)))
                        .collect();
                    BexExternalValue::Map {
                        key_type: bex_external_types::RuntimeTy::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        value_type: bex_external_types::RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        entries,
                    }
                }
                Object::Instance(instance) => {
                    // Get class name from the class object
                    let class_obj = vm.get_object(instance.class);
                    let class_name = match class_obj {
                        Object::Class(class) => class.name.to_string(),
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    // Get field names from class and convert fields
                    let class_fields = match class_obj {
                        Object::Class(class) => &class.fields,
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    let fields: indexmap::IndexMap<String, BexExternalValue> = class_fields
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(class_field, slot)| {
                            (
                                class_field.name.clone(),
                                vm_arg_to_external(vm, slot.load()),
                            )
                        })
                        .collect();

                    BexExternalValue::Instance {
                        class_name,
                        type_args: instance.class_type_args.to_vec(),
                        fields,
                    }
                }
                Object::Bigint(bi) => BexExternalValue::Bigint((**bi).clone()),
                Object::Uint8Array(bytes) => BexExternalValue::Uint8Array(bytes.to_vec()),
                Object::Variant(variant) => {
                    let enum_obj = vm.get_object(variant.enm);
                    let Object::Enum(enm) = enum_obj else {
                        panic!("variant.enm doesn't point to an Enum");
                    };
                    let variant_name = enm
                        .variants
                        .get(variant.index)
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| format!("<variant {}>", variant.index));
                    BexExternalValue::Variant {
                        enum_name: enm.name.to_string(),
                        variant_name,
                    }
                }
                // These types should not appear as sys op arguments.
                Object::Function(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::GenericFunction(_)
                | Object::HostClosure(_)
                | Object::Cell(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::Future(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Type(_) => {
                    panic!(
                        "Cannot convert object type to BexExternalValue for sys op: {:?}",
                        ObjectType::of(obj)
                    )
                }
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => {
                    panic!("Cannot convert sentinel to BexExternalValue")
                }
            }
        }
    }
}

/// Coerce a host-encoded **incoming** value to match the declared param type.
///
/// Handles two layers of bridge mismatch:
///
/// 1. **Class / enum naming:** host encoders carry an informational class or
///    variant name (e.g. `root.lorem.MyLorem`); rewrite it to the
///    engine-registered FQN (`user.lorem.MyLorem`) so VM heap lookups hit. A
///    plain `Map` arriving at a *non-generic* class slot is also promoted to
///    `Instance`; against a *generic* class slot the promotion is rejected (a
///    bare map can't supply the required class type args).
/// 2. **Numeric / optional / union coercion:** see `coerce_numeric_to_declared_type`.
///
/// Nested container types (arrays/maps with mismatched element types) are not
/// walked; host-side schema-aware encoders own that shaping.
pub(crate) fn coerce_arg_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    match (value, ty) {
        // ── Class / enum naming (incoming only) ──────────────────────────
        (BexExternalValue::Map { entries, .. }, RuntimeTy::Class(type_name, class_args, _)) => {
            // Promoting a bare `Map` to an `Instance` fabricates a class value
            // with NO class type args (a map carries none). That's only sound
            // for a *non-generic* class slot. Against a generic class the slot
            // demands concrete type args the map can't supply, so promotion
            // would manufacture an under-specified generic instance — reject
            // instead and require the host to send a fully-bound instance.
            if !class_args.is_empty() {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "cannot coerce a host map into the generic class `{type_name}`: a bare \
                         map carries no class type arguments. Send a fully-bound generic \
                         instance instead."
                    ),
                });
            }
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                type_args: vec![],
                fields: entries,
            })
        }
        (
            BexExternalValue::Instance {
                fields, type_args, ..
            },
            RuntimeTy::Class(type_name, _, _),
        ) => {
            // FQN rewrite only — carry the wire-supplied class type args through
            // unchanged (Phase 3 checks them against `expected_args`).
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                type_args,
                fields,
            })
        }
        (BexExternalValue::Variant { variant_name, .. }, RuntimeTy::Enum(type_name, _)) => {
            Ok(BexExternalValue::Variant {
                enum_name: type_name.to_string(),
                variant_name,
            })
        }

        // ── Union with a class member (incoming only) ────────────────────
        // A host-encoded object arrives as a bare `Map` (the JS encoder emits
        // every non-builtin object as `map_value`, with no FQN). Against a
        // union it would otherwise fail `value_matches_type` ("Value of type
        // 'map' does not match any member of union [...]"). Route it to the
        // union's class-typed member (unwrapping `Optional`) and promote it to
        // an `Instance`. The wire value carries no class name, so we pick the
        // first class arm — sufficient while a union has at most one class
        // member; numeric/string arms are left to the existing routing.
        (
            value @ (BexExternalValue::Map { .. } | BexExternalValue::Instance { .. }),
            RuntimeTy::Union(members, _),
        ) => {
            if let Some(class_arm) = members.iter().find_map(union_class_arm) {
                coerce_arg_to_declared_type(value, class_arm)
            } else {
                Ok(value)
            }
        }

        // ── Numeric / optional / union ───────────────────────────────────
        (v, ty) => coerce_numeric_to_declared_type(v, ty),
    }
}

/// If `ty` is a class (directly, or inside an `Optional`), return that class
/// `RuntimeTy`. Used to route a host-encoded object value to a union's class member.
fn union_class_arm(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    match ty {
        RuntimeTy::Class(..) => Some(ty),
        _ => None,
    }
}

/// Coerce an **outgoing** return value to match the declared return type.
///
/// Handles int↔bigint conversion at the FFI boundary. These conversions exist
/// **only** at the host boundary — the type system is purely structural and
/// does not relate `int` and `bigint` (see
/// `baml_compiler2_tir::normalize::is_subtype_of`). `int → bigint` widens
/// unconditionally; `bigint → int` succeeds when the value fits in i64,
/// erroring on overflow rather than silently truncating. Also performs optional
/// unwrap and numeric-singleton union routing.
///
/// Class / enum naming is intentionally *not* rewritten here — the
/// engine-side FQN (e.g. `user.lorem.MyLorem`) is the authoritative
/// output and stripping it back to the bare display name would break
/// host-side type lookups.
pub(crate) fn coerce_return_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    coerce_numeric_to_declared_type(value, ty)
}

/// Shared numeric / optional / union coercion used by both arg and return
/// paths.
///
/// These conversions exist only at the FFI boundary. The compile-time subtype
/// relation (`baml_compiler2_tir::normalize::is_subtype_of`,
/// `baml_type::RuntimeTy::is_subtype_of`) is purely structural and does **not** widen
/// `int` to `bigint`; the arms below add that widening (plus a checked
/// `bigint → int` narrowing) only when crossing the host boundary.
fn coerce_numeric_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    match (value, ty) {
        // Int → Bigint widening (FFI boundary only — `int` is not a subtype of
        // `bigint` in the type system).
        (
            BexExternalValue::Int(i),
            RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _),
        ) => Ok(BexExternalValue::Bigint(num_bigint::BigInt::from(i))),

        // Bigint → Int narrowing: host-supplied bigint must fit in i64, otherwise
        // there is no safe representation in the `int` slot and we reject the
        // call rather than silently truncate.
        (
            BexExternalValue::Bigint(bi),
            RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _),
        ) => i64::try_from(&bi)
            .map(BexExternalValue::Int)
            .map_err(|_| EngineError::TypeMismatch {
                message: format!("bigint value {bi} does not fit in i64"),
            }),

        // Union with exactly one of {Int, Bigint}: route to that member.
        // Nullable numeric unions (`int | null`) flow through here too — a
        // `Null` value coerced against the chosen numeric member falls to the
        // catch-all `(v, _) => Ok(v)` and is preserved.
        // Unions containing both are left alone; `find_matching_union_member`
        // picks by value shape at the VM boundary.
        (v, RuntimeTy::Union(members, _)) => {
            let has_int = members.iter().any(|m| {
                matches!(
                    m,
                    RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _)
                )
            });
            let has_bigint = members.iter().any(|m| {
                matches!(
                    m,
                    RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _)
                )
            });
            if has_int == has_bigint {
                Ok(v)
            } else if let Some(target) = members.iter().find(|m| {
                matches!(
                    m,
                    RuntimeTy::Int { .. }
                        | RuntimeTy::Bigint { .. }
                        | RuntimeTy::Literal(Literal::Int(_) | Literal::Bigint(_), _, _)
                )
            }) {
                coerce_numeric_to_declared_type(v, target)
            } else {
                Ok(v)
            }
        }

        (v, _) => Ok(v),
    }
}

/// Convert a compiled `TestArgValue` to a `BexExternalValue` for function calls.
pub fn test_arg_to_external(v: &bex_vm_types::TestArgValue) -> BexExternalValue {
    match v {
        bex_vm_types::TestArgValue::Null => BexExternalValue::Null,
        bex_vm_types::TestArgValue::Int(i) => BexExternalValue::Int(*i),
        bex_vm_types::TestArgValue::Float(f) => BexExternalValue::Float(*f),
        bex_vm_types::TestArgValue::Bool(b) => BexExternalValue::Bool(*b),
        bex_vm_types::TestArgValue::String(s) => BexExternalValue::String(s.as_str().into()),
        bex_vm_types::TestArgValue::Array {
            element_type,
            items,
        } => BexExternalValue::Array {
            element_type: element_type.clone(),
            items: items.iter().map(test_arg_to_external).collect(),
        },
        bex_vm_types::TestArgValue::Map {
            key_type,
            value_type,
            entries,
        } => BexExternalValue::Map {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
            entries: entries
                .iter()
                .map(|(k, v)| (k.clone(), test_arg_to_external(v)))
                .collect(),
        },
    }
}

#[cfg(test)]
mod peel_to_rust_type_tests {
    use baml_type::TyAttr;

    use super::*;

    /// `RuntimeTy::RustType` — the canonical `$rust_type` shape.
    fn rust_type() -> RuntimeTy {
        RuntimeTy::RustType {
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn direct_rust_type_matches() {
        assert_eq!(peel_to_rust_type(&rust_type()), Some(()));
    }

    #[test]
    fn optional_rust_type_peels_through() {
        // `RuntimeTy::optional(RustType)` lowers to `RuntimeTy::Union([RustType, Null])`
        // post-`RuntimeTy::Optional`-removal; the union arm in `peel_to_rust_type`
        // picks the single RustType member.
        let ty = RuntimeTy::optional(rust_type());
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn nested_optional_rust_type_peels_through() {
        // `T??` collapses to `T?` per `RuntimeTy::optional`'s idempotence rule
        // (a union already containing `null` is returned unchanged), so
        // this is effectively the same shape as the single-optional case
        // — still a single non-null member that peels.
        let ty = RuntimeTy::optional(RuntimeTy::optional(rust_type()));
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn singleton_union_with_rust_type_and_null_matches() {
        // `RustType | null` — only one non-null arm so the peel
        // unambiguously picks `RustType`.
        let ty = RuntimeTy::Union(
            vec![
                rust_type(),
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn union_with_rust_type_plus_non_rust_arm_still_unique_matches() {
        // `RustType | string` — there's still exactly one `RustType` arm,
        // and `peel_to_rust_type` only cares about uniqueness of *that*
        // shape (non-`RustType` arms count as "doesn't match" and don't
        // contribute to the duplicate-count).
        let ty = RuntimeTy::Union(
            vec![
                rust_type(),
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn union_with_two_rust_type_arms_is_ambiguous() {
        // `RustType | RustType` — two arms peel to the target. The
        // function rejects to avoid silently picking one.
        let ty = RuntimeTy::Union(vec![rust_type(), rust_type()], TyAttr::default());
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn plain_string_does_not_match() {
        assert_eq!(
            peel_to_rust_type(&RuntimeTy::String {
                attr: TyAttr::default()
            }),
            None,
        );
    }

    #[test]
    fn unrelated_opaque_does_not_match() {
        // A different opaque leaf type — e.g. `baml.llm.PromptAst` — must
        // not be confused with `$rust_type`.
        let ty = RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        };
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn optional_of_unrelated_type_does_not_match() {
        let ty = RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn union_with_no_rust_type_arm_does_not_match() {
        let ty = RuntimeTy::Union(
            vec![
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), None);
    }
}

#[cfg(test)]
mod peel_function_ty_tests {
    use baml_type::{RuntimeFunctionParamTy, TyAttr};

    use super::*;

    /// `(int) -> string` — the canonical concrete function shape.
    fn fn_ty() -> RuntimeTy {
        RuntimeTy::Function {
            params: vec![RuntimeFunctionParamTy::required(
                None,
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            )],
            ret: Box::new(RuntimeTy::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(RuntimeTy::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    /// A second, distinct function shape — `() -> int` — used to verify the
    /// uniqueness rule rejects two function members in a union.
    fn other_fn_ty() -> RuntimeTy {
        RuntimeTy::Function {
            params: vec![],
            ret: Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(RuntimeTy::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn direct_function_returns_itself() {
        let ty = fn_ty();
        let peeled = peel_function_ty(&ty).expect("must peel a direct Function");
        assert!(matches!(peeled, RuntimeTy::Function { .. }));
    }

    #[test]
    fn optional_function_peels_through() {
        // `RuntimeTy::optional(fn)` lowers to `RuntimeTy::Union([fn, Null])`; the union
        // arm in `peel_function_ty` picks the single function member.
        let ty = RuntimeTy::optional(fn_ty());
        let peeled = peel_function_ty(&ty).expect("Union<fn, Null> must peel");
        assert!(matches!(peeled, RuntimeTy::Function { .. }));
    }

    #[test]
    fn nested_optional_function_peels_through() {
        // `RuntimeTy::optional` is idempotent — `T??` collapses to `T?` — so this
        // is effectively the same shape as the single-optional case.
        let ty = RuntimeTy::optional(RuntimeTy::optional(fn_ty()));
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_single_function_arm_peels_through() {
        // `((int) -> string) | null` — only one function member.
        let ty = RuntimeTy::Union(
            vec![
                fn_ty(),
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_function_plus_non_function_arm_peels_through() {
        // `((int) -> string) | string` — exactly one function member.
        let ty = RuntimeTy::Union(
            vec![
                fn_ty(),
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_two_distinct_function_arms_is_ambiguous() {
        // `((int) -> string) | (() -> int)` — two function members.
        // The peel rejects to avoid silently picking one. Pins the
        // determinism contract of the helper.
        let ty = RuntimeTy::Union(vec![fn_ty(), other_fn_ty()], TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn plain_string_does_not_match() {
        let ty = RuntimeTy::String {
            attr: TyAttr::default(),
        };
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn optional_of_non_function_does_not_match() {
        let ty = RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn union_with_no_function_arm_does_not_match() {
        let ty = RuntimeTy::Union(
            vec![
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn empty_union_does_not_match() {
        let ty = RuntimeTy::Union(vec![], TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }
}

#[cfg(test)]
mod inference_unifier_tests {
    //! Unit tests for the `RuntimeTy` generic unifier (01a/01b): `union_runtime_ty`
    //! and `infer_bindings_runtime`. These mirror the TIR's `union_ty` /
    //! `infer_bindings_inner` semantics — union-merge, `null`-strip, no arm
    //! routing (00b3 G5).
    use baml_type::{Name, TyAttr};

    use super::*;

    fn tv(name: &str) -> RuntimeTy {
        RuntimeTy::TypeVar(Name::new(name), TyAttr::default())
    }
    fn int() -> RuntimeTy {
        RuntimeTy::int()
    }
    fn string() -> RuntimeTy {
        RuntimeTy::string()
    }
    fn null() -> RuntimeTy {
        RuntimeTy::null()
    }
    fn rust() -> RuntimeTy {
        RuntimeTy::RustType {
            attr: TyAttr::default(),
        }
    }
    fn never() -> RuntimeTy {
        RuntimeTy::Never {
            attr: TyAttr::default(),
        }
    }
    fn list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }
    fn map(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(string()),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }
    fn union(members: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Union(members, TyAttr::default())
    }
    fn class(name: &str, args: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Class(
            baml_type::TypeName::local(Name::new(name)),
            args,
            TyAttr::default(),
        )
    }
    fn infer(formal: &RuntimeTy, actual: &RuntimeTy) -> indexmap::IndexMap<String, RuntimeTy> {
        let mut out = indexmap::IndexMap::new();
        infer_bindings_runtime(formal, actual, &mut out);
        out
    }

    // ── union_runtime_ty ──────────────────────────────────────────────────
    #[test]
    fn union_dedups_to_bare() {
        assert_eq!(union_runtime_ty(&int(), &int()), int());
    }
    #[test]
    fn union_distinct_members() {
        assert_eq!(
            union_runtime_ty(&int(), &string()),
            union(vec![int(), string()])
        );
    }
    #[test]
    fn union_flattens_nested() {
        assert_eq!(
            union_runtime_ty(&int(), &union(vec![string(), RuntimeTy::bool()])),
            union(vec![int(), string(), RuntimeTy::bool()])
        );
    }
    #[test]
    fn union_drops_never() {
        assert_eq!(union_runtime_ty(&never(), &int()), int());
    }
    #[test]
    fn union_host_only_with_known() {
        // 00b3 T25: choose(5, host_obj) ⇒ int | rust_type.
        assert_eq!(
            union_runtime_ty(&int(), &rust()),
            union(vec![int(), rust()])
        );
    }

    // ── infer_bindings_runtime ────────────────────────────────────────────
    #[test]
    fn infer_bare_var() {
        let out = infer(&tv("T"), &int());
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("T"), Some(&int()));
    }
    #[test]
    fn infer_union_merges_repeat_bindings() {
        // choose<T>(left: T, right: T) with (int, string) ⇒ T = int | string.
        let mut out = indexmap::IndexMap::new();
        infer_bindings_runtime(&tv("T"), &int(), &mut out);
        infer_bindings_runtime(&tv("T"), &string(), &mut out);
        assert_eq!(out.get("T"), Some(&union(vec![int(), string()])));
    }
    #[test]
    fn infer_skips_typevar_actual() {
        assert!(infer(&tv("T"), &tv("U")).is_empty());
    }
    #[test]
    fn infer_binds_top_type_actual() {
        // The runtime unifier no longer special-cases the top type `unknown`
        // (the `skip_top` knob was removed): it binds like any concrete actual,
        // matching compile-time inference. In practice the synth layer never
        // feeds `unknown` here — a no-evidence value becomes host-only
        // `rust_type` instead — so this binding does not arise end-to-end.
        let unknown = RuntimeTy::unknown();
        assert_eq!(infer(&tv("T"), &unknown).get("T"), Some(&unknown));
    }
    #[test]
    fn infer_through_list() {
        assert_eq!(infer(&list(tv("T")), &list(int())).get("T"), Some(&int()));
    }
    #[test]
    fn infer_through_map_value() {
        assert_eq!(
            infer(&map(tv("T")), &map(RuntimeTy::bool())).get("T"),
            Some(&RuntimeTy::bool())
        );
    }
    #[test]
    fn infer_through_class_args() {
        // second_of<T>(p: GenericPair<int, T>) with GenericPair<int, string> ⇒ T = string.
        let formal = class("GenericPair", vec![int(), tv("T")]);
        let actual = class("GenericPair", vec![int(), string()]);
        assert_eq!(infer(&formal, &actual).get("T"), Some(&string()));
    }
    #[test]
    fn infer_nullable_strips_and_binds() {
        // maybe_id<T>(x: T?) with int ⇒ T = int (00b3 §I, T32).
        assert_eq!(
            infer(&union(vec![tv("T"), null()]), &int()).get("T"),
            Some(&int())
        );
    }
    #[test]
    fn infer_nullable_null_actual_binds_null() {
        // maybe_id(None) ⇒ T = null (TIR-faithful, T33).
        assert_eq!(
            infer(&union(vec![tv("T"), null()]), &null()).get("T"),
            Some(&null())
        );
    }
    #[test]
    fn infer_union_with_concrete_sibling_routes_to_typevar() {
        // 02a reverses 00b3 G5/§H: `T | string | null` vs `int` now routes the
        // residual (int, not absorbed by the string/null siblings) to `T`.
        assert_eq!(
            infer(&union(vec![tv("T"), string(), null()]), &int()).get("T"),
            Some(&int())
        );
    }
    #[test]
    fn infer_union_concrete_sibling_absorbs_actual_is_noop() {
        // The string sibling absorbs a string actual ⇒ T stays unbound (Gate A
        // then governs); only the *residual* routes to T.
        assert!(!infer(&union(vec![tv("T"), string(), null()]), &string()).contains_key("T"));
    }
    #[test]
    fn infer_equal_length_union_with_direct_typevars_is_ambiguous() {
        // Regression: a same-length union actual must NOT be positionally zipped
        // into direct TypeVar members. `A | B` vs `int | string` has no
        // principled split (>1 TypeVar member) ⇒ bind nothing (the equal-length
        // zip arm previously bound A=int, B=string by accidental ordering).
        let formal = union(vec![tv("A"), tv("B")]);
        let actual = union(vec![int(), string()]);
        let out = infer(&formal, &actual);
        assert!(!out.contains_key("A"));
        assert!(!out.contains_key("B"));
    }
    #[test]
    fn infer_single_typevar_union_routes_residual_not_positional() {
        // Regression: `T | int` vs `int | string` (equal length) must route the
        // unmatched `string` atom to T, not positionally bind T = int.
        let formal = union(vec![tv("T"), int()]);
        let actual = union(vec![int(), string()]);
        let out = infer(&formal, &actual);
        assert_eq!(out.get("T"), Some(&string()));
    }
}
