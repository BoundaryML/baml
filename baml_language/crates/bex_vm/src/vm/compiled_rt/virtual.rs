//! Interface dispatch with a per-site cache for static implementations.

use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

use bex_vm_types::{
    HeapPtr, Object, ObjectType, RealizedTy, TypeHead, Value,
    types::{BoundMethod, Closure},
};

use crate::{
    errors::{VmError, VmInternalError},
    package_baml::ImplResolver,
    vm::BexVm,
};

/// An atomic cache of a virtual call site's last receiver tag and callee.
pub struct VirtualSite {
    /// `FILLED | HAS_FRAME? | receiver tag << 32 | callee object index`.
    entry: AtomicU64,
    // Cached type heads belong to one immutable program image and never move.
    frame: OnceLock<SiteFrame>,
}

struct SiteFrame {
    receiver: u32,
    /// The compile-time image the heads in `type_args` point into.
    image: usize,
    type_args: Box<[RealizedTy]>,
}

const FILLED: u64 = 1 << 63;
const HAS_FRAME: u64 = 1 << 62;
// Declared and primitive receiver tags fit below the two flag bits.
const TAG_LIMIT: u64 = 1 << 30;

impl VirtualSite {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entry: AtomicU64::new(0),
            frame: OnceLock::new(),
        }
    }

    /// Return whether the site has cached a target.
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.entry.load(Ordering::Relaxed) & FILLED != 0
    }

    fn get(&self, tag: u32, image: usize) -> Option<(u32, &[RealizedTy])> {
        let word = self.entry.load(Ordering::Acquire);
        if word & FILLED == 0 || (word >> 32) & (TAG_LIMIT - 1) != u64::from(tag) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        let callee = word as u32;
        if word & HAS_FRAME == 0 {
            return Some((callee, &[]));
        }
        let frame = self.frame.get()?;
        (frame.receiver == tag && frame.image == image).then_some((callee, &*frame.type_args))
    }

    fn set(&self, tag: u32, callee: u32, image: usize, type_args: &[RealizedTy]) {
        let mut word = FILLED | (u64::from(tag) << 32) | u64::from(callee);
        if !type_args.is_empty() {
            // The first receiver owns the frame slot for the site's lifetime.
            let _ = self.frame.set(SiteFrame {
                receiver: tag,
                image,
                type_args: type_args.into(),
            });
            if self
                .frame
                .get()
                .is_none_or(|frame| frame.receiver != tag || frame.image != image)
            {
                return;
            }
            word |= HAS_FRAME;
        }
        self.entry.store(word, Ordering::Release);
    }
}

impl Default for VirtualSite {
    fn default() -> Self {
        Self::new()
    }
}

impl BexVm {
    /// Resolve a cached interface method and push implementation and method type arguments.
    pub fn virtual_target(
        &mut self,
        site: &VirtualSite,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<HeapPtr, Box<VmError>> {
        self.virtual_target_in(
            Some(site),
            receiver,
            iface,
            iface_args,
            method,
            method_type_args,
        )
    }

    /// Resolve without caching when the site's type arguments can vary.
    pub fn virtual_target_uncached(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<HeapPtr, Box<VmError>> {
        self.virtual_target_in(None, receiver, iface, iface_args, method, method_type_args)
    }

    fn virtual_target_in(
        &mut self,
        site: Option<&VirtualSite>,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<HeapPtr, Box<VmError>> {
        // The receiver tag must fully determine the implementation.
        let key = site.and(self.site_key(receiver));
        if let (Some(site), Some(tag)) = (site, key)
            && let Some((callee, frame)) = site.get(tag, self.image())
        {
            self.compiled.call_type_args.extend_from_slice(frame);
            self.compiled
                .call_type_args
                .extend_from_slice(method_type_args);
            return Ok(self.heap.compile_time_ptr(callee as usize));
        }
        let head = self.interface_head(iface)?;
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
            && let (Some(site), Some(tag)) = (site, key)
            && let Some(index) = self.compile_time_index(callee)
        {
            site.set(tag, index, self.image(), &frame);
        }
        self.compiled.call_type_args.extend(frame);
        self.compiled
            .call_type_args
            .extend_from_slice(method_type_args);
        Ok(callee)
    }

    /// Call an eligible inline callee, leaving its frame on error for the unwinder.
    #[inline]
    pub fn call_inline(
        &mut self,
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
        match super::compiled_fn(function) {
            Some(super::CompiledFn::Inline(body)) => {
                self.call_compiled(callee, body, args, pc).map(Some)
            }
            _ => Ok(None),
        }
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

    fn site_key(&self, receiver: Value) -> Option<u32> {
        let tag = match receiver.as_object_ptr() {
            None => crate::vm::value_type_tag(receiver),
            Some(ptr) => match self.get_object(ptr) {
                Object::Float(_) | Object::String(_) => crate::vm::value_type_tag(receiver),
                Object::Instance(instance)
                    if instance.class_type_args.is_empty()
                        && self.heap.is_compile_time_ptr(instance.class) =>
                {
                    match self.get_object(instance.class) {
                        Object::Class(class) => class.type_tag.as_i64(),
                        _ => return None,
                    }
                }
                _ => return None,
            },
        };
        u32::try_from(tag)
            .ok()
            .filter(|&tag| u64::from(tag) < TAG_LIMIT)
    }

    fn compile_time_index(&self, ptr: HeapPtr) -> Option<u32> {
        if !self.heap.is_compile_time_ptr(ptr) {
            return None;
        }
        let offset = (ptr.as_ptr() as usize).checked_sub(self.image())?;
        u32::try_from(offset / std::mem::size_of::<Object>()).ok()
    }

    fn image(&self) -> usize {
        self.heap.compile_time_ptr(0).as_ptr() as usize
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

    use super::VirtualSite;
    use crate::{
        compiled_rt::{CompiledFn, Resume, Step},
        errors::VmError,
        vm::{
            BexVm,
            compiled_rt::tests::{run_with_compiled_bodies, test_int},
        },
    };

    type Out = Result<Step, Box<VmError>>;

    fn object(index: &AtomicU32) -> ObjectIndex {
        ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize)
    }

    fn dispatch(
        vm: &mut BexVm,
        resume: Resume,
        site: &VirtualSite,
        iface: &AtomicU32,
        iface_args: &[RealizedTy],
        method: &str,
    ) -> Out {
        let depth = vm.frame_depth();
        match resume {
            Resume::Start => {
                vm.reserve_slots(depth, 3);
                let receiver = vm.slot(depth, 0);
                vm.set_slot(depth, 2, Value::int(1));
                let iface = vm.idx_to_ptr(object(iface));
                let callee = vm.virtual_target(site, receiver, iface, iface_args, method, &[])?;
                let argc = vm.push_virtual_args(callee, None, &[receiver])?;
                Ok(Step::Call { callee, argc })
            }
            Resume::Value(value) => Ok(Step::Return(value)),
            Resume::Thrown(error) => Err(error),
            Resume::Yielded => unreachable!("the body never yields"),
        }
    }

    #[test]
    fn explicit_implementations_dispatch_and_cache() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        static SITE: VirtualSite = VirtualSite::new();
        fn total(vm: &mut BexVm, resume: Resume) -> Out {
            dispatch(vm, resume, &SITE, &SHAPE, &[], "area")
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
                &[("user.Total", CompiledFn::Resumable(total))],
                &[("user.Shape", &SHAPE)],
                telemetry,
            );
            assert_eq!(out, "4|6|9|-1", "telemetry: {telemetry}");
            assert!(SITE.is_filled());
        }
    }

    #[test]
    fn inline_implementations_run_in_place() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        static SITE: VirtualSite = VirtualSite::new();
        static IN_PLACE: AtomicU32 = AtomicU32::new(0);
        fn square_area(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
            IN_PLACE.fetch_add(1, Ordering::Relaxed);
            let side = test_int(vm.load_field(args[0], 0)?);
            Ok(Value::int(side * side))
        }
        fn total(vm: &mut BexVm, resume: Resume) -> Out {
            let depth = vm.frame_depth();
            match resume {
                Resume::Start => {
                    vm.reserve_slots(depth, 3);
                    let receiver = vm.slot(depth, 0);
                    let iface = vm.idx_to_ptr(object(&SHAPE));
                    let callee = vm.virtual_target(&SITE, receiver, iface, &[], "area", &[])?;
                    if let Some(value) = vm.call_inline(callee, &[receiver], 0)? {
                        return Ok(Step::Return(value));
                    }
                    vm.set_slot(depth, 2, Value::int(1));
                    let argc = vm.push_virtual_args(callee, None, &[receiver])?;
                    Ok(Step::Call { callee, argc })
                }
                Resume::Value(value) => Ok(Step::Return(value)),
                Resume::Thrown(error) => Err(error),
                Resume::Yielded => unreachable!("the body never yields"),
            }
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
                    ("user.Total", CompiledFn::Resumable(total)),
                    (
                        "user.<(user.Square as user.Shape)>.area",
                        CompiledFn::Inline(square_area),
                    ),
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
        static SITE: VirtualSite = VirtualSite::new();
        fn greet(vm: &mut BexVm, resume: Resume) -> Out {
            dispatch(vm, resume, &SITE, &GREETER, &[], "greet")
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
            &[("user.Greet", CompiledFn::Resumable(greet))],
            &[("user.Greeter", &GREETER)],
            false,
        );
        assert_eq!(out, "hello from Adopts|hello from Adopts");
        assert!(SITE.is_filled());
    }

    #[test]
    fn structural_defaults_resolve_without_caching() {
        static TO_STRING: AtomicU32 = AtomicU32::new(0);
        static SITE: VirtualSite = VirtualSite::new();
        fn show(vm: &mut BexVm, resume: Resume) -> Out {
            dispatch(vm, resume, &SITE, &TO_STRING, &[], "to_string")
        }
        const SOURCE: &str = r#"
            class Plain { x int }
            function Show(p: Plain) -> string { p.to_string() }
            function Main() -> string { Show(Plain { x: 1 }) }
        "#;
        let expected = run_with_compiled_bodies(SOURCE, &[], &[], false);
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Show", CompiledFn::Resumable(show))],
            &[("baml.ToString", &TO_STRING)],
            false,
        );
        assert_eq!(out, expected);
        assert!(out.contains("Plain"), "{out}");
        assert!(!SITE.is_filled());
    }

    #[test]
    fn generic_interfaces_dispatch_on_their_instantiation() {
        static CONV: AtomicU32 = AtomicU32::new(0);
        static SITE: VirtualSite = VirtualSite::new();
        fn describe(vm: &mut BexVm, resume: Resume) -> Out {
            dispatch(vm, resume, &SITE, &CONV, &[RealizedTy::Int], "describe")
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
            &[("user.Describe", CompiledFn::Resumable(describe))],
            &[("user.Conv", &CONV)],
            false,
        );
        assert_eq!(out, "as int");
    }

    #[test]
    fn bound_method_values_resolve_at_bind_time() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        fn area_fn(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
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
            &[("user.AreaFn", CompiledFn::Inline(area_fn))],
            &[("user.Shape", &SHAPE)],
            false,
        );
        assert_eq!(out, "10");
    }

    #[test]
    fn interface_fields_follow_the_implementation_links() {
        static NAMED: AtomicU32 = AtomicU32::new(0);
        fn rename(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
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
            &[("user.Rename", CompiledFn::Inline(rename))],
            &[("user.Named", &NAMED)],
            false,
        );
        assert_eq!(out, "b|d|cat");
    }
}
