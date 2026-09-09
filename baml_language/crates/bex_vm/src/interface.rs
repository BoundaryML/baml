//! Checked operations on interface views at the engine boundary.

use crate::{BexVm, package_baml::ImplResolver};
use bex_heap::TlabHolder;
use bex_vm_types::{BoundMethod, HeapPtr, Object, RealizedTy, TyTemplate, Value};

/// Bare result `Self` is exposed as this checked view. Projection bases keep
/// concrete Self so `Self.Output` still resolves to the implementation's pin.
/// The compiler rejects Self under invariant constructors before this point.
fn project_result_self(ty: &mut TyTemplate, interface: &RealizedTy) {
    match ty {
        TyTemplate::TypeArgRef(0) => *ty = TyTemplate::from(interface.clone()),
        TyTemplate::Union(members, _) => {
            for member in members {
                project_result_self(member, interface);
            }
        }
        _ => {}
    }
}

impl BexVm {
    /// Bind a direct class member with the instance's captured generic prefix.
    /// Interface bodies have no public function-name dispatch and are excluded
    /// by find_function_by_name; they use the checked obligation path instead.
    pub fn bind_concrete_inherent_method(
        &mut self,
        receiver: Value,
        class_name: &str,
        member: &str,
        method_args: &[RealizedTy],
    ) -> Result<HeapPtr, String> {
        let name = format!("{class_name}.{member}");
        let function = self
            .find_function_by_name(&name)
            .ok_or_else(|| format!("class has no inherent method `{member}`"))?;
        let Object::Function(callee) = self.get_object(function) else {
            return Err("class member is not a function".into());
        };
        if callee.param_names.first().map(String::as_str) != Some("self") {
            return Err(format!("class member `{member}` has no receiver"));
        }
        let bounds = callee.generic_param_bounds.clone();
        let mut frame = self.bound_method_curried_type_args(receiver).into_vec();
        if frame.len() + method_args.len() != bounds.len() {
            return Err(format!(
                "class method `{member}` has the wrong number of method type arguments"
            ));
        }
        frame.extend_from_slice(method_args);
        self.validate_runtime_generic_bounds(&name, &bounds, &frame)
            .map_err(|error| format!("{error:?}"))?;
        Ok(self.tlab_mut().alloc(Object::BoundMethod(BoundMethod {
            function,
            receiver,
            type_args: frame.into_boxed_slice(),
            interface_signature: None,
        })))
    }

    /// Translate checked caller slots (including self) into implementation
    /// slots. Required names are insignificant; optionals match by name and
    /// the implementation may declare more of them in a different order.
    pub fn interface_method_args(
        &self,
        method: &BoundMethod,
        args: &[Value],
    ) -> Result<Vec<Value>, String> {
        let Some(signature) = &method.interface_signature else {
            return Ok(args.to_vec());
        };
        let RealizedTy::Function { params, .. } = &**signature else {
            return Err("invalid interface function contract".into());
        };
        let Object::Function(callee) = self.get_object(method.function) else {
            return Err("interface implementation is not a function".into());
        };
        if args.len() != params.len() + 1 || callee.arity == 0 {
            return Err("interface method argument count does not match its contract".into());
        }
        let mut caller =
            baml_type::CallLayout::from_modes(params.iter().map(|p| (p.name.clone(), p.mode)));
        caller.0.insert(0, None);
        let mapping = caller.map_to(&callee.argument_layout())?;
        Ok(mapping
            .into_iter()
            .map(|slot| slot.map_or(Value::OMITTED_ARG, |index| args[index]))
            .collect())
    }

    /// Choose the receiver's implementation world while it is still live.
    pub fn interface_world(&self, receiver: Value) -> HeapPtr {
        let owner = self.value_runtime_package(receiver);
        if owner.is_null() {
            self.current_runtime_package()
        } else {
            owner
        }
    }

    /// Validate a complete existential, including exact associated bindings.
    pub fn check_interface_view(
        &self,
        receiver: Value,
        interface: &RealizedTy,
        world: HeapPtr,
    ) -> Result<(), String> {
        let RealizedTy::Interface(head, args, assoc, _) = interface else {
            return Err("expected a realized interface type".into());
        };
        let Object::Interface(declaration) = self.get_object(head.ptr()) else {
            return Err("interface type does not name an interface declaration".into());
        };
        if args.len() != declaration.args.len() {
            return Err(format!(
                "interface expects {} type arguments, got {}",
                declaration.args.len(),
                args.len()
            ));
        }
        let mut names = assoc.iter().map(|(name, _)| name).collect::<Vec<_>>();
        names.sort();
        let mut expected = declaration.associated_type_names.iter().collect::<Vec<_>>();
        expected.sort();
        if names != expected {
            return Err("interface view requires every associated binding exactly once".into());
        }
        let concrete = self
            .value_concrete_ty(receiver)
            .ok_or("receiver has no concrete BAML type")?;
        let resolver = ImplResolver::for_package(self, world);
        if !resolver.type_implements(&concrete.into(), *head, args, assoc) {
            return Err(
                "receiver does not implement the requested interface with these exact bindings"
                    .into(),
            );
        }
        Ok(())
    }

    /// Bind a method selected by the checked interface, preserving the
    /// implementation/default owner frame followed by method type arguments.
    pub fn bind_interface_method(
        &mut self,
        receiver: Value,
        interface: &RealizedTy,
        world: HeapPtr,
        name: &str,
        method_args: &[RealizedTy],
    ) -> Result<HeapPtr, String> {
        self.bind_checked_implementation_method(receiver, interface, world, name, method_args, true)
    }

    /// Use the resolved implementation's concrete contract. In particular,
    /// retain concrete Self and a provided method's refined result/effect.
    /// The complete interface view still selects and validates the obligation.
    pub fn bind_concrete_interface_method(
        &mut self,
        receiver: Value,
        interface: &RealizedTy,
        world: HeapPtr,
        name: &str,
        method_args: &[RealizedTy],
    ) -> Result<HeapPtr, String> {
        self.bind_checked_implementation_method(
            receiver,
            interface,
            world,
            name,
            method_args,
            false,
        )
    }

    fn bind_checked_implementation_method(
        &mut self,
        receiver: Value,
        interface: &RealizedTy,
        world: HeapPtr,
        name: &str,
        method_args: &[RealizedTy],
        existential: bool,
    ) -> Result<HeapPtr, String> {
        self.check_interface_view(receiver, interface, world)?;
        let RealizedTy::Interface(head, args, _, _) = interface else {
            unreachable!()
        };
        let Object::Interface(declaration) = self.get_object(head.ptr()) else {
            unreachable!()
        };
        let dispatch = declaration
            .method_dispatch
            .get(name)
            .ok_or_else(|| format!("interface has no method `{name}`"))?;
        let bex_vm_types::types::InterfaceMethodDispatch::Resolved(target) = dispatch else {
            return Err(format!(
                "interface method `{name}` is ambiguous; qualify its declaring interface"
            ));
        };
        let concrete: RealizedTy = self
            .value_concrete_ty(receiver)
            .expect("view checked above")
            .into();
        let mut root_frame = vec![concrete.clone()];
        root_frame.extend_from_slice(args);
        let target = target
            .substitute(&root_frame, self)
            .map_err(|error| error.to_string())?;
        let RealizedTy::Interface(head, args, pins, _) = &target else {
            return Err("interface method target is not an interface constraint".into());
        };
        let resolver = ImplResolver::for_package(self, world);
        if !resolver.type_implements(&concrete, *head, args, pins) {
            return Err("receiver does not satisfy the method's declaring interface".into());
        }
        let Object::Interface(declaration) = self.get_object(head.ptr()) else {
            return Err("method target does not name an interface declaration".into());
        };
        let member = declaration
            .methods
            .iter()
            .find(|method| method.name.as_str() == name)
            .ok_or_else(|| format!("interface has no method `{name}`"))?;
        if !member.has_receiver {
            return Err(format!("method `{name}` has no receiver"));
        }
        if existential && !member.existential_callable {
            return Err(format!(
                "method `{name}` requires a concrete Self projection"
            ));
        }
        let signature = if existential {
            let mut signature = member.signature.clone();
            let TyTemplate::Function { ret, throws, .. } = &mut signature else {
                return Err("interface member has no function contract".into());
            };
            project_result_self(ret, interface);
            project_result_self(throws, interface);
            Some(signature)
        } else {
            None
        };
        let mut interface_frame = vec![concrete.clone()];
        interface_frame.extend_from_slice(args);
        interface_frame.extend_from_slice(method_args);
        // Declaration-owned evidence is authoritative even for a bodyless
        // method, or a generic parameter absent from its value signature.
        let declared_bounds = member.generic_param_bounds.clone();
        let declared_count = member.generic_params.len();
        let declared_name = format!("{}.{}", declaration.name, member.name);
        if declared_count != declared_bounds.len() || declared_count < 1 + args.len() {
            return Err("interface method has an invalid generic frame".into());
        }
        let required = declared_count - 1 - args.len();
        if method_args.len() != required {
            return Err(format!(
                "method `{name}` expects {required} type arguments, got {}",
                method_args.len()
            ));
        }
        self.validate_runtime_generic_bounds(&declared_name, &declared_bounds, &interface_frame)
            .map_err(|error| format!("{error:?}"))?;
        let resolver = ImplResolver::for_package(self, world);
        let (rule, bindings) = resolver
            .resolve_implements_rule(&concrete, *head, args)
            .ok_or("checked implementation could not be resolved")?;
        let method = resolver
            .rule_method_impl(&rule, name)
            .ok_or("checked implementation has no method target")?
            .method;
        let function = method.fqn;
        let mut frame = resolver
            .realize_frame(&method.frame, &bindings)
            .map_err(|e| format!("{e:?}"))?;
        let Object::Function(callee) = self.get_object(function) else {
            return Err("interface target is not a function".into());
        };
        let required = callee
            .generic_param_bounds
            .len()
            .saturating_sub(frame.len());
        if method_args.len() != required {
            return Err(format!(
                "method `{name}` expects {required} type arguments, got {}",
                method_args.len()
            ));
        }
        let bounds = callee.generic_param_bounds.clone();
        let callee_name = callee.name.clone();
        frame.extend_from_slice(method_args);
        self.validate_runtime_generic_bounds(&callee_name, &bounds, &frame)
            .map_err(|e| format!("{e:?}"))?;
        let signature = signature
            .map(|signature| signature.substitute(&interface_frame, self).map(Box::new))
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(self.tlab_mut().alloc(Object::BoundMethod(BoundMethod {
            function,
            receiver,
            type_args: frame.into_boxed_slice(),
            interface_signature: signature,
        })))
    }
}
