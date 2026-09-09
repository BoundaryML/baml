//! Retained, checked interface values and method projection.

use crate::{BexEngine, BexExternalValue, EngineError, FunctionCallContext};
use bex_external_types::{Handle, InterfaceReceiver, InterfaceValue, RuntimeTy};
use bex_heap::{HeapPermit, PermitProof};
use bex_vm::BexVm;
use bex_vm_types::{HeapPtr, Object, RealizedTy, TypeHead, Value, ValueKind};
use std::{collections::BTreeMap, sync::Arc};

fn mismatch(message: impl Into<String>) -> EngineError {
    EngineError::TypeMismatch {
        message: message.into(),
    }
}

impl BexEngine {
    pub(crate) fn retain_interface(
        &self,
        receiver: Value,
        interface: &bex_vm_types::RuntimeTy,
        vm: &BexVm,
        proof: PermitProof<'_>,
    ) -> Result<Arc<InterfaceValue>, EngineError> {
        let normalized = baml_type::normalize::normalize(interface.as_ty(), vm);
        let interface = RealizedTy::try_from(&normalized)
            .map_err(|_| mismatch("interface view type is not fully realized"))?;
        let world = vm.interface_world(receiver);
        vm.check_interface_view(receiver, &interface, world)
            .map_err(mismatch)?;
        let mut declarations = BTreeMap::new();
        let interface = interface.try_map_heads(&mut |head: &TypeHead| {
            let name = head
                .to_tagged_name()
                .map_err(|_| mismatch("invalid interface declaration head"))?;
            declarations
                .entry(head.tag())
                .or_insert_with(|| self.heap.create_handle(head.ptr()));
            Ok::<_, EngineError>(name)
        })?;
        let receiver = match receiver.kind() {
            ValueKind::Null => InterfaceReceiver::Null,
            ValueKind::Int(value) => InterfaceReceiver::Int(value),
            ValueKind::Bool(value) => InterfaceReceiver::Bool(value),
            ValueKind::Object(ptr) => InterfaceReceiver::Heap(self.heap.create_handle(ptr)),
            ValueKind::OmittedArg => {
                return Err(mismatch("omitted argument is not an interface receiver"));
            }
        };
        // All pointers were read and rooted before releasing this permit.
        let _ = proof;
        Ok(Arc::new(InterfaceValue {
            receiver,
            interface,
            declarations: Arc::new(declarations),
            world: (!world.is_null()).then(|| self.heap.create_handle(world)),
        }))
    }

    pub(crate) fn resolve_interface(
        &self,
        view: &InterfaceValue,
        vm: &BexVm,
        proof: PermitProof<'_>,
    ) -> Result<(Value, RealizedTy, HeapPtr), EngineError> {
        let resolve = |handle: &Handle| {
            self.resolve_handle(proof, handle).ok_or_else(|| {
                mismatch("interface reference belongs to another runtime or is closed")
            })
        };
        let interface = view.interface.try_map_heads(&mut |name| {
            let handle = view
                .declarations
                .get(&name.tag())
                .ok_or_else(|| mismatch("interface declaration root is missing"))?;
            let ptr = resolve(handle)?;
            let tag = match vm.get_object(ptr) {
                Object::Class(value) => value.type_tag,
                Object::Enum(value) => value.type_tag,
                Object::Interface(value) => value.type_tag,
                Object::TypeAlias(value) => value.type_tag,
                _ => return Err(mismatch("interface root is not a type declaration")),
            };
            if tag != name.tag() {
                return Err(mismatch("interface declaration identity mismatch"));
            }
            Ok(TypeHead::new(ptr, tag))
        })?;
        let receiver = match &view.receiver {
            InterfaceReceiver::Null => Value::NULL,
            InterfaceReceiver::Int(value) => {
                Value::try_int(*value).ok_or_else(|| mismatch("invalid integer receiver"))?
            }
            InterfaceReceiver::Bool(value) => Value::bool(*value),
            InterfaceReceiver::Heap(handle) => Value::object(resolve(handle)?),
        };
        let world = view
            .world
            .as_ref()
            .map(resolve)
            .transpose()?
            .unwrap_or_else(HeapPtr::null);
        if !world.is_null() && !matches!(vm.get_object(world), Object::Package(_)) {
            return Err(mismatch("interface world is not a runtime package"));
        }
        vm.check_interface_view(receiver, &interface, world)
            .map_err(mismatch)?;
        Ok((receiver, interface, world))
    }

    /// Explicit projection of an existing value. Generated argument codecs use
    /// the same check; native shape alone never establishes implementation.
    pub async fn project_interface(
        self: &Arc<Self>,
        value: BexExternalValue,
        expected: RuntimeTy,
    ) -> Result<Arc<InterfaceValue>, EngineError> {
        self.project_interface_with_type_argument(
            value,
            bex_external_types::TypeArgument::Named(expected),
        )
        .await
    }

    /// Select a view using retained type evidence, including runtime-created
    /// associated bindings. A live type must belong to this engine; it never
    /// falls back to lookup by display name or portable reconstruction.
    pub async fn project_interface_with_type_argument(
        self: &Arc<Self>,
        value: BexExternalValue,
        expected: bex_external_types::TypeArgument,
    ) -> Result<Arc<InterfaceValue>, EngineError> {
        let mut thread = self
            .new_root_thread(tokio_util::sync::CancellationToken::new())
            .await;
        let (wire, retained) = self.resolve_type_argument(&mut thread, expected)?;
        let expected = match retained {
            Some(value) => value.ty.into(),
            None => crate::conversion::anchor_wire_ty(&thread.vm, &wire)?,
        };
        let value = self.convert_external_to_vm_value_with_ty(&mut thread, value, None)?;
        self.retain_interface(value, &expected, &thread.vm, thread.proof())
    }

    /// Produce an owned callable for a legal interface method. The receiver,
    /// default body and realized type frame survive independently of the view.
    pub async fn bind_interface_method(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<RuntimeTy>,
    ) -> Result<Handle, EngineError> {
        self.bind_interface_method_with_type_arguments(
            view,
            method,
            type_args
                .into_iter()
                .map(bex_external_types::TypeArgument::Named)
                .collect(),
        )
        .await
    }

    pub async fn bind_interface_method_with_type_arguments(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<Handle, EngineError> {
        self.bind_checked_implementation_method(view, method, type_args, true)
            .await
    }

    /// Select a concrete implementation method from a complete obligation.
    /// The receiver and generic frame come from the retained value, never a
    /// host-supplied concrete Self annotation. This is distinct from calling
    /// through an existential interface contract.
    pub async fn bind_concrete_interface_method(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<Handle, EngineError> {
        self.bind_checked_implementation_method(view, method, type_args, false)
            .await
    }

    /// Generated concrete-class callers name the static declaration they were
    /// generated for and an implementation obligation from the compiler graph.
    /// Variables in the obligation are class slots, instantiated exclusively
    /// from the retained instance. Method arguments have a separate frame.
    pub async fn bind_concrete_class_interface_method(
        self: &Arc<Self>,
        receiver: Handle,
        class: &baml_type::QualifiedTypeName,
        interface_pattern: RuntimeTy,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<Handle, EngineError> {
        let mut thread = self
            .new_root_thread(tokio_util::sync::CancellationToken::new())
            .await;
        let (receiver, class_args) = self.concrete_class_receiver(&thread, &receiver, class)?;
        let pattern = crate::conversion::anchor_wire_ty(&thread.vm, &interface_pattern)?;
        // Rewrite live-headed types directly: projecting the captured arguments
        // to display names would lose runtime-created declaration identity.
        let substituted = baml_type::unify::rewrite_ty(pattern.as_ty(), &mut |node| match node {
            baml_type::Ty::TypeVar(param, _) => class_args
                .get(param.index() as usize)
                .map(|ty| ty.as_runtime_ty().as_ty().clone()),
            _ => None,
        });
        let normalized = baml_type::normalize::normalize(&substituted, &thread.vm);
        let interface = RealizedTy::try_from(&normalized).map_err(|_| {
            mismatch("concrete interface obligation has unresolved class arguments")
        })?;
        let world = thread.vm.interface_world(receiver);
        let args = self.resolve_method_type_arguments(&mut thread, type_args)?;
        let bound = thread
            .vm
            .bind_concrete_interface_method(receiver, &interface, world, method, &args)
            .map_err(mismatch)?;
        Ok(self.heap.create_handle(bound))
    }

    pub async fn bind_concrete_class_inherent_method(
        self: &Arc<Self>,
        receiver: Handle,
        class: &baml_type::QualifiedTypeName,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<Handle, EngineError> {
        let mut thread = self
            .new_root_thread(tokio_util::sync::CancellationToken::new())
            .await;
        let (receiver, _) = self.concrete_class_receiver(&thread, &receiver, class)?;
        let args = self.resolve_method_type_arguments(&mut thread, type_args)?;
        let bound = thread
            .vm
            .bind_concrete_inherent_method(receiver, &class.to_string(), method, &args)
            .map_err(mismatch)?;
        Ok(self.heap.create_handle(bound))
    }

    fn concrete_class_receiver(
        &self,
        thread: &bex_heap::ActiveHeapPermit<crate::thread::BexThread>,
        receiver: &Handle,
        class: &baml_type::QualifiedTypeName,
    ) -> Result<(Value, Box<[RealizedTy]>), EngineError> {
        let ptr = self
            .resolve_handle(thread.proof(), receiver)
            .ok_or_else(|| mismatch("concrete receiver belongs to another runtime or is closed"))?;
        let Object::Instance(instance) = thread.vm.get_object(ptr) else {
            return Err(mismatch("concrete method receiver is not a class instance"));
        };
        let expected = thread
            .vm
            .declaration_head(class)
            .ok_or_else(|| mismatch("concrete caller class is unavailable"))?;
        if expected.ptr() != instance.class {
            return Err(mismatch(
                "concrete caller does not match the receiver's declaration",
            ));
        }
        Ok((Value::object(ptr), instance.class_type_args.clone()))
    }

    fn resolve_method_type_arguments(
        &self,
        thread: &mut bex_heap::ActiveHeapPermit<crate::thread::BexThread>,
        type_args: Vec<bex_external_types::TypeArgument>,
    ) -> Result<Vec<RealizedTy>, EngineError> {
        type_args
            .into_iter()
            .map(|argument| {
                let (wire, value) = self.resolve_type_argument(thread, argument)?;
                let anchored = match value {
                    Some(value) => value.ty.into(),
                    None => crate::conversion::anchor_wire_ty(&thread.vm, &wire)?,
                };
                RealizedTy::try_from(anchored)
                    .map_err(|_| mismatch("method type argument must be fully realized"))
            })
            .collect()
    }

    async fn bind_checked_implementation_method(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
        existential: bool,
    ) -> Result<Handle, EngineError> {
        let mut thread = self
            .new_root_thread(tokio_util::sync::CancellationToken::new())
            .await;
        let (receiver, interface, world) =
            self.resolve_interface(&view, &thread.vm, thread.proof())?;
        let args = self.resolve_method_type_arguments(&mut thread, type_args)?;
        let bound = if existential {
            thread
                .vm
                .bind_interface_method(receiver, &interface, world, method, &args)
        } else {
            thread
                .vm
                .bind_concrete_interface_method(receiver, &interface, world, method, &args)
        }
        .map_err(mismatch)?;
        Ok(self.heap.create_handle(bound))
    }

    pub async fn call_concrete_interface_named(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
        args: indexmap::IndexMap<String, BexExternalValue>,
        context: FunctionCallContext,
    ) -> Result<BexExternalValue, EngineError> {
        if !context.type_args.is_empty() {
            return Err(mismatch(
                "method type arguments belong to the checked method target",
            ));
        }
        let callable = self
            .bind_concrete_interface_method(view, method, type_args)
            .await?;
        self.call_callable_keywords(callable, args, context, true)
            .await
    }

    pub async fn call_interface_named(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
        args: indexmap::IndexMap<String, BexExternalValue>,
        context: FunctionCallContext,
    ) -> Result<BexExternalValue, EngineError> {
        if !context.type_args.is_empty() {
            return Err(mismatch(
                "method type arguments belong to the checked method target",
            ));
        }
        let callable = self
            .bind_interface_method_with_type_arguments(view, method, type_args)
            .await?;
        self.call_callable_keywords(callable, args, context, true)
            .await
    }

    /// Invoke through the same checked callable entry used for host callbacks.
    pub async fn call_interface(
        self: &Arc<Self>,
        view: Arc<InterfaceValue>,
        method: &str,
        type_args: Vec<RuntimeTy>,
        args: Vec<BexExternalValue>,
        context: FunctionCallContext,
    ) -> Result<BexExternalValue, EngineError> {
        let callable = self.bind_interface_method(view, method, type_args).await?;
        self.call_callable(callable, args, context, true).await
    }
}
