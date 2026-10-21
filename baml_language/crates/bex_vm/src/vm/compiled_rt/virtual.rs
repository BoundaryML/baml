//! Interface dispatch through the interpreter's resolver and its per-site
//! cache of static implementations.

use bex_vm_types::{
    HeapPtr, Object, ObjectType, RealizedTy, TypeHead, Value,
    types::{BoundMethod, Closure},
};

use crate::{
    errors::{VmError, VmInternalError},
    package_baml::ImplResolver,
    vm::{BexVm, StaticVirtualCallKey, StaticVirtualCallTarget},
};

impl BexVm {
    /// Resolve an interface method for `receiver` and push the
    /// implementation's then the method's type arguments for the next call.
    /// `site` is the call's bytecode instruction index, when known; a known
    /// site shares the interpreter's cache entry for the same `VirtualCall`.
    pub fn virtual_target(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
        site: Option<u32>,
    ) -> Result<HeapPtr, Box<VmError>> {
        let head = self.interface_head(iface)?;
        let key = site.and_then(|site| self.virtual_call_key(receiver, head, iface_args, site));
        if let Some(target) = key
            .as_ref()
            .and_then(|key| self.static_virtual_call_cache.get(key))
        {
            self.compiled
                .call_type_args
                .extend_from_slice(&target.frame_type_args);
            self.compiled
                .call_type_args
                .extend_from_slice(method_type_args);
            return Ok(target.callee);
        }
        let self_ty = self.dispatch_self_ty(receiver)?;
        let resolver = ImplResolver::for_value(self, receiver);
        let implementation = resolver
            .resolve_implementation(&self_ty, head, iface_args)
            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                method: method.to_string(),
            })?;
        let (callee, frame) = resolver.implementation_method(&implementation, method)?;
        // Later-loaded implementations can replace structural defaults.
        if implementation.is_static()
            && let Some(key) = key
        {
            self.static_virtual_call_cache.insert(
                key,
                StaticVirtualCallTarget {
                    callee,
                    frame_type_args: frame.clone(),
                },
            );
        }
        self.compiled.call_type_args.extend(frame);
        self.compiled
            .call_type_args
            .extend_from_slice(method_type_args);
        Ok(callee)
    }

    /// The interpreter's cache key for the virtual call at bytecode
    /// instruction `site` of the innermost (compiled) frame.
    fn virtual_call_key(
        &self,
        receiver: Value,
        interface_head: TypeHead,
        iface_args: &[RealizedTy],
        site: u32,
    ) -> Option<StaticVirtualCallKey> {
        let top = self.top_frame()?;
        // SAFETY: a compiled frame's function lives in the compile-time heap;
        // see `load_function`.
        #[allow(unsafe_code)]
        let function = unsafe { self.frame_function(top) }.ok()?;
        let call_pc = function
            .bytecode
            .compact
            .as_ref()?
            .offset_of(site as usize)?;
        Some(StaticVirtualCallKey {
            caller_function_addr: std::ptr::from_ref(function) as usize,
            call_pc,
            receiver: self.static_virtual_receiver_key(receiver)?,
            interface_head,
            interface_args: iface_args.into(),
        })
    }

    /// Call an eligible inline callee, leaving its frame on error for the unwinder.
    #[inline]
    pub fn call_inline(
        &mut self,
        caller: &super::FrameRecord,
        callee: HeapPtr,
        args: &[Value],
        pc: u32,
    ) -> Result<Option<Value>, Box<VmError>> {
        if !self.compiled.call_type_args.is_empty() {
            return Ok(None);
        }
        let Object::Function(function) = self.get_object(callee) else {
            return Ok(None);
        };
        if function.arity != args.len() || function.trace_passthrough {
            return Ok(None);
        }
        let Some(body) = super::compiled_fn(function) else {
            return Ok(None);
        };
        if self.direct_calls()
            && !function
                .compiled_body
                .is_some_and(bex_vm_types::types::CompiledBody::framed)
        {
            return self
                .call_record_dispatched(caller, callee, body, Vec::new(), args, pc)
                .map(Some);
        }
        self.call_compiled(caller, callee, body, &[], args, pc)
            .map(Some)
    }

    /// Map interface arguments to callee slots and return the argument count.
    pub fn push_virtual_args(
        &mut self,
        callee: HeapPtr,
        layout: Option<&[Option<&str>]>,
        args: &[Value],
    ) -> Result<u32, Box<VmError>> {
        match self.lay_out_virtual_args(callee, layout, args) {
            Ok(count) => Ok(count),
            Err(error) => {
                // Discard type arguments for the failed call.
                self.compiled.call_type_args.clear();
                Err(error)
            }
        }
    }

    /// Map named arguments to callee slots while reserving a bound method's receiver slot.
    pub fn push_indirect_args(
        &mut self,
        callee: HeapPtr,
        layout: &[Option<&str>],
        args: &[Value],
    ) -> Result<u32, Box<VmError>> {
        let Object::BoundMethod(method) = self.get_object(callee) else {
            return self.push_virtual_args(callee, Some(layout), args);
        };
        let receiver = method.receiver;
        let full_layout: Vec<Option<&str>> = std::iter::once(None)
            .chain(layout.iter().copied())
            .collect();
        let full_args: Vec<Value> = std::iter::once(receiver)
            .chain(args.iter().copied())
            .collect();
        let base = self.stack.len();
        let count = self.push_virtual_args(callee, Some(&full_layout), &full_args)?;
        self.stack.0.remove(base);
        Ok(count - 1)
    }

    fn lay_out_virtual_args(
        &mut self,
        callee: HeapPtr,
        layout: Option<&[Option<&str>]>,
        args: &[Value],
    ) -> Result<u32, Box<VmError>> {
        let count = |count: usize| u32::try_from(count).expect("an argument count fits u32");
        let Some(layout) = layout else {
            self.stack.extend(args.iter().copied());
            return Ok(count(args.len()));
        };
        // Extra optional parameters require remapping even for positional arguments.
        if layout.iter().all(Option::is_none) && self.callee_params(callee)?.arity() == args.len() {
            self.stack.extend(args.iter().copied());
            return Ok(count(args.len()));
        }
        let caller = baml_type::CallLayout(
            layout
                .iter()
                .map(|slot| slot.map(baml_type::Name::new))
                .collect(),
        );
        let params = self.callee_params(callee)?;
        if params.layout_is(&caller) {
            self.stack.extend(args.iter().copied());
            return Ok(count(args.len()));
        }
        let target = params.layout();
        let mapping = caller
            .map_to(&target)
            .map_err(VmInternalError::CallLayout)?;
        self.stack.extend(
            mapping
                .into_iter()
                .map(|slot| slot.map_or(Value::OMITTED_ARG, |index| args[index])),
        );
        Ok(count(target.len()))
    }

    /// Read an interface field from the receiver's mapped slot.
    pub fn virtual_load_field(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        field_index: usize,
    ) -> Result<Value, Box<VmError>> {
        let head = self.interface_head(iface)?;
        let slot = self.resolve_virtual_field_slot(receiver, head, iface_args, field_index)?;
        self.load_field(receiver, slot)
    }

    /// Write an interface field in the receiver's mapped slot.
    pub fn virtual_store_field(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        field_index: usize,
        value: Value,
    ) -> Result<(), Box<VmError>> {
        let head = self.interface_head(iface)?;
        let slot = self.resolve_virtual_field_slot(receiver, head, iface_args, field_index)?;
        self.store_field(receiver, slot, value)
    }

    /// Bind an interface method with implementation type arguments followed by method arguments.
    pub fn make_virtual_bound_method(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<Value, Box<VmError>> {
        let self_ty = self.dispatch_self_ty(receiver)?;
        let resolver = ImplResolver::for_value(self, receiver);
        let (function, type_args) = resolve_method(
            self,
            resolver,
            &self_ty,
            iface,
            iface_args,
            method,
            method_type_args,
        )?;
        let bound = Object::BoundMethod(BoundMethod {
            function,
            receiver,
            type_args: type_args.into_boxed_slice(),
        });
        Ok(Value::object(self.tlab.alloc(bound)))
    }

    /// Create an interface method closure for a statically known `Self` type.
    pub fn make_virtual_function(
        &mut self,
        self_ty: &RealizedTy,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<Value, Box<VmError>> {
        // Static `Self` types use implementations visible in the lexical world.
        let resolver = ImplResolver::new(self);
        let (function, type_args) = resolve_method(
            self,
            resolver,
            self_ty,
            iface,
            iface_args,
            method,
            method_type_args,
        )?;
        let closure = Object::Closure(Closure {
            function,
            captures: Box::new([]),
            captured_type_args: type_args.into_boxed_slice(),
        });
        Ok(Value::object(self.tlab.alloc(closure)))
    }

    fn interface_head(&self, iface: HeapPtr) -> Result<TypeHead, Box<VmError>> {
        match self.get_object(iface) {
            Object::Interface(declaration) => Ok(TypeHead::new(iface, declaration.type_tag)),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Interface.into(),
                got: ObjectType::of(other).into(),
            }
            .into()),
        }
    }

    fn dispatch_self_ty(&self, receiver: Value) -> Result<RealizedTy, Box<VmError>> {
        self.value_concrete_ty(receiver)
            .map(RealizedTy::from)
            .ok_or_else(|| {
                VmInternalError::CompiledBody {
                    message: format!(
                        "value of kind {:?} cannot be a virtual-call dispatch argument",
                        self.type_of(&receiver)
                    ),
                }
                .into()
            })
    }
}

// Resolve the callee with implementation type arguments followed by method arguments.
fn resolve_method(
    vm: &BexVm,
    resolver: ImplResolver<'_>,
    self_ty: &RealizedTy,
    iface: HeapPtr,
    iface_args: &[RealizedTy],
    method: &str,
    method_type_args: &[RealizedTy],
) -> Result<(HeapPtr, Vec<RealizedTy>), Box<VmError>> {
    let head = vm.interface_head(iface)?;
    let implementation = resolver
        .resolve_implementation(self_ty, head, iface_args)
        .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
            method: method.to_string(),
        })?;
    let (callee, mut frame) = resolver.implementation_method(&implementation, method)?;
    frame.extend_from_slice(method_type_args);
    Ok((callee, frame))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use bex_vm_types::{ObjectIndex, RealizedTy, Value};

    use crate::{
        compiled_rt::Step,
        errors::VmError,
        vm::{
            BexVm,
            compiled_rt::tests::{run_with_compiled_bodies, test_int},
        },
    };

    type Out = Result<Value, Box<VmError>>;

    fn object(index: &AtomicU32) -> ObjectIndex {
        ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize)
    }

    fn dispatch(
        vm: &mut BexVm,
        args: &[Value],
        iface: &AtomicU32,
        iface_args: &[RealizedTy],
        method: &str,
    ) -> Out {
        let receiver = args[0];
        let iface = vm.idx_to_ptr(object(iface));
        let callee = vm.virtual_target(receiver, iface, iface_args, method, &[], Some(0))?;
        let argc = vm.push_virtual_args(callee, None, &[receiver])?;
        vm.suspend(Step::Call { callee, argc })
    }

    #[test]
    fn explicit_implementations_dispatch_and_cache() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        fn total(vm: &mut BexVm, _fr: &crate::compiled_rt::FrameRecord, args: &[Value]) -> Out {
            dispatch(vm, args, &SHAPE, &[], "area")
        }
        const SOURCE: &str = r#"
            interface Shape { function area(self) -> int throws never }
            class Square {
                side int
                implements Shape {
                    function area(self) -> int { if (self.side == 0) { 1 / self.side } else { self.side * self.side } }
                }
            }
            class Rect {
                w int
                h int
                implements Shape { function area(self) -> int { self.w * self.h } }
            }
            function Total(s: Shape) -> int { s.area() }
            function Main() -> string {
                let a = Total(Square { side: 2 });
                let b = Total(Rect { w: 2, h: 3 });
                let c = Total(Square { side: 3 });
                let d = Total(Square { side: 0 }) catch (e) { baml.panics.DivisionByZero => -1 };
                `${a}|${b}|${c}|${d}`
            }
        "#;
        for telemetry in [false, true] {
            let out = run_with_compiled_bodies(
                SOURCE,
                &[("user.Total", total)],
                &[("user.Shape", &SHAPE)],
                telemetry,
            );
            assert_eq!(out, "4|6|9|-1", "telemetry: {telemetry}");
        }
    }

    #[test]
    fn inline_implementations_run_in_place() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        static IN_PLACE: AtomicU32 = AtomicU32::new(0);
        fn square_area(
            vm: &mut BexVm,
            _fr: &crate::compiled_rt::FrameRecord,
            args: &[Value],
        ) -> Result<Value, Box<VmError>> {
            IN_PLACE.fetch_add(1, Ordering::Relaxed);
            let side = test_int(vm.load_field(args[0], 0)?);
            Ok(Value::int(side * side))
        }
        fn total(vm: &mut BexVm, fr: &crate::compiled_rt::FrameRecord, args: &[Value]) -> Out {
            let receiver = args[0];
            let iface = vm.idx_to_ptr(object(&SHAPE));
            let callee = vm.virtual_target(receiver, iface, &[], "area", &[], Some(0))?;
            if let Some(value) = vm.call_inline(fr, callee, &[receiver], 0)? {
                return Ok(value);
            }
            let argc = vm.push_virtual_args(callee, None, &[receiver])?;
            vm.suspend(Step::Call { callee, argc })
        }
        const SOURCE: &str = r#"
            interface Shape { function area(self) -> int throws never }
            class Square {
                side int
                implements Shape { function area(self) -> int { 0 } }
            }
            class Rect {
                w int
                h int
                implements Shape { function area(self) -> int { self.w * self.h } }
            }
            function Total(s: Shape) -> int { s.area() }
            function Main() -> string {
                `${Total(Square { side: 2 })}|${Total(Rect { w: 2, h: 3 })}|${Total(Square { side: 3 })}`
            }
        "#;
        for telemetry in [false, true] {
            let before = IN_PLACE.load(Ordering::Relaxed);
            let out = run_with_compiled_bodies(
                SOURCE,
                &[
                    ("user.Total", total),
                    ("user.<(user.Square as user.Shape)>.area", square_area),
                ],
                &[("user.Shape", &SHAPE)],
                telemetry,
            );
            assert_eq!(out, "4|6|9", "telemetry: {telemetry}");
            assert_eq!(IN_PLACE.load(Ordering::Relaxed), before + 2);
        }
    }

    #[test]
    fn adopted_defaults_carry_their_frame_through_the_cache() {
        static GREETER: AtomicU32 = AtomicU32::new(0);
        fn greet(vm: &mut BexVm, _fr: &crate::compiled_rt::FrameRecord, args: &[Value]) -> Out {
            dispatch(vm, args, &GREETER, &[], "greet")
        }
        const SOURCE: &str = r#"
            interface Greeter {
                function greet(self) -> string throws never { `hello from ${reflect.Type.of<Self>().to_string()}` }
            }
            class Adopts { implements Greeter {} }
            function Greet(g: Greeter) -> string { g.greet() }
            function Main() -> string { `${Greet(Adopts {})}|${Greet(Adopts {})}` }
        "#;
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Greet", greet)],
            &[("user.Greeter", &GREETER)],
            false,
        );
        assert_eq!(out, "hello from Adopts|hello from Adopts");
    }

    #[test]
    fn structural_defaults_resolve_without_caching() {
        static TO_STRING: AtomicU32 = AtomicU32::new(0);
        fn show(vm: &mut BexVm, _fr: &crate::compiled_rt::FrameRecord, args: &[Value]) -> Out {
            dispatch(vm, args, &TO_STRING, &[], "to_string")
        }
        const SOURCE: &str = r#"
            class Plain { x int }
            function Show(p: Plain) -> string { p.to_string() }
            function Main() -> string { Show(Plain { x: 1 }) }
        "#;
        let expected = run_with_compiled_bodies(SOURCE, &[], &[], false);
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Show", show)],
            &[("baml.ToString", &TO_STRING)],
            false,
        );
        assert_eq!(out, expected);
        assert!(out.contains("Plain"), "{out}");
    }

    #[test]
    fn generic_interfaces_dispatch_on_their_instantiation() {
        static CONV: AtomicU32 = AtomicU32::new(0);
        fn describe(vm: &mut BexVm, _fr: &crate::compiled_rt::FrameRecord, args: &[Value]) -> Out {
            dispatch(vm, args, &CONV, &[RealizedTy::Int], "describe")
        }
        const SOURCE: &str = r#"
            interface Conv<T> { function describe(self) -> string throws never }
            class Both {
                implements Conv<int> { function describe(self) -> string throws never { "as int" } }
                implements Conv<string> { function describe(self) -> string throws never { "as string" } }
            }
            function Describe(c: Conv<int>) -> string { c.describe() }
            function Main() -> string { Describe(Both {}) }
        "#;
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Describe", describe)],
            &[("user.Conv", &CONV)],
            false,
        );
        assert_eq!(out, "as int");
    }

    #[test]
    fn bound_method_values_resolve_at_bind_time() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        fn area_fn(
            vm: &mut BexVm,
            _fr: &crate::compiled_rt::FrameRecord,
            args: &[Value],
        ) -> Result<Value, Box<VmError>> {
            let iface = vm.idx_to_ptr(object(&SHAPE));
            vm.make_virtual_bound_method(args[0], iface, &[], "area", &[])
        }
        const SOURCE: &str = r#"
            interface Shape { function area(self) -> int throws never }
            class Rect {
                w int
                h int
                implements Shape { function area(self) -> int { self.w * self.h } }
            }
            function AreaFn(s: Shape) -> () -> int throws never { s.area }
            function Main() -> string {
                let f = AreaFn(Rect { w: 2, h: 5 });
                `${f()}`
            }
        "#;
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.AreaFn", area_fn)],
            &[("user.Shape", &SHAPE)],
            false,
        );
        assert_eq!(out, "10");
    }

    #[test]
    fn interface_fields_follow_the_implementation_links() {
        static NAMED: AtomicU32 = AtomicU32::new(0);
        fn rename(
            vm: &mut BexVm,
            _fr: &crate::compiled_rt::FrameRecord,
            args: &[Value],
        ) -> Result<Value, Box<VmError>> {
            let iface = vm.idx_to_ptr(object(&NAMED));
            vm.virtual_store_field(args[0], iface, &[], 0, args[1])?;
            vm.virtual_load_field(args[0], iface, &[], 0)
        }
        const SOURCE: &str = r#"
            interface Named { name string }
            class Person {
                name string
                implements Named {}
            }
            class Pet {
                kind string
                name string
                implements Named {}
            }
            function Rename(n: Named, to: string) -> string { n.name = to; n.name }
            function Main() -> string {
                let pet = Pet { kind: "cat", name: "c" };
                `${Rename(Person { name: "a" }, "b")}|${Rename(pet, "d")}|${pet.kind}`
            }
        "#;
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Rename", rename)],
            &[("user.Named", &NAMED)],
            false,
        );
        assert_eq!(out, "b|d|cat");
    }
}
