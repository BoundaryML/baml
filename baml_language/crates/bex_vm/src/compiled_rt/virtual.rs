//! Interface dispatch from compiled code: the interpreter's `VirtualCall`,
//! `VirtualLoadField`/`VirtualStoreField`, `MakeVirtualBoundMethod` and
//! `MakeVirtualFunction`, for a call site whose interface and method are
//! known at compile time and whose receiver is not.
//!
//! Resolution is the interpreter's: the receiver's concrete type and the
//! interface select at most one implementation (`ImplResolver`), and the
//! method's callee comes with the implementation's frame type arguments. What
//! differs is the cache. The interpreter keys a program-wide table on the call
//! site and the receiver; compiled code gives each site its own
//! [`VirtualSite`], a `static` in the generated crate, so a hit is one atomic
//! load and a compare instead of a hash probe.

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

/// The inline cache of one virtual call site in compiled code.
///
/// Monomorphic: it remembers the last receiver type the site dispatched on
/// and the callee that resolved for it, as one word, so readers on every
/// thread see a receiver with its own callee or nothing. Only a static
/// implementation (an `implements` rule in the program image) is cached, as
/// in the interpreter: a structural default can be replaced by an
/// implementation a later-loaded package registers. A receiver is keyed by
/// its type tag when that alone decides the implementation: a primitive, or
/// an instance of a non-generic class in the image. Anything else (a generic
/// instance, an array, an enum variant) resolves on every call.
///
/// The implementation's frame type arguments ride beside the word in a
/// `OnceLock`, set once for the receiver that first filled the site; a
/// site that later sees another receiver with a non-empty frame resolves
/// without caching, so a hit never pairs one receiver's callee with
/// another's frame. The heads in a frame are compile-time objects, which
/// never move, so a `static` may hold them; they belong to one heap's image,
/// so a hit from a VM over another heap (the same program loaded again in
/// this process) resolves instead.
pub struct VirtualSite {
    /// `FILLED | HAS_FRAME? | receiver tag << 32 | callee object index`.
    entry: AtomicU64,
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
/// Receiver tags are declared heads' tags (`CLASS_BASE` plus an object index)
/// or a primitive's: a few bits short of 30 in any program.
const TAG_LIMIT: u64 = 1 << 30;

impl VirtualSite {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entry: AtomicU64::new(0),
            frame: OnceLock::new(),
        }
    }

    /// Whether the site has cached a target; for tests.
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.entry.load(Ordering::Relaxed) & FILLED != 0
    }

    /// The cached callee and frame for a receiver keyed `tag`, in the heap
    /// whose compile-time image starts at `image`.
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
            // The frame slot belongs to the receiver that first filled it;
            // another receiver's frame is not cached.
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
    /// The callee of interface method `method` for `receiver`, exactly as the
    /// interpreter's `VirtualCall` resolves it, through `site`'s cache. The
    /// callee's frame type arguments (the implementation's, then
    /// `method_type_args`) are pushed as the type arguments of the call a
    /// resumable body is about to request ([`Self::push_type_arg`]).
    ///
    /// `iface` is the interface's `Object::Interface`; `iface_args` its
    /// instantiation, empty for a non-generic interface. Failing to resolve is
    /// an internal error: the checker proved the receiver implements the
    /// interface.
    pub fn virtual_target(
        &mut self,
        site: &VirtualSite,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<HeapPtr, Box<VmError>> {
        let key = self.site_key(receiver);
        if let Some(tag) = key
            && let Some((callee, frame)) = site.get(tag, self.image())
        {
            self.call_type_args.extend_from_slice(frame);
            self.call_type_args.extend_from_slice(method_type_args);
            return Ok(self.heap.compile_time_ptr(callee as usize));
        }
        let head = self.interface_head(iface)?;
        let self_ty = self.dispatch_self_ty(receiver)?;
        let resolver = ImplResolver::for_value(self, receiver);
        let implementation = resolver
            .resolve_implementation(&self_ty, head, iface_args)
            .ok_or_else(|| {
                Box::new(VmError::from(VmInternalError::UnresolvedVirtualCall {
                    method: method.to_string(),
                }))
            })?;
        let (callee, frame) = resolver
            .implementation_method(&implementation, method)
            .map_err(|error| Box::new(VmError::from(error)))?;
        if implementation.is_static()
            && let Some(tag) = key
            && let Some(index) = self.compile_time_index(callee)
        {
            site.set(tag, index, self.image(), &frame);
        }
        self.call_type_args.extend(frame);
        self.call_type_args.extend_from_slice(method_type_args);
        Ok(callee)
    }

    /// Run the callee [`Self::virtual_target`] resolved in place when it is a
    /// compiled body that cannot suspend, called with no type arguments and
    /// with `args` as its slots: the implementation of a non-generic class,
    /// in the common case. Otherwise `None`, and the body requests the call
    /// from the VM with the type arguments already pushed.
    ///
    /// The call is [`Self::call_compiled`]'s: a frame, the depth limit, and
    /// on `Err` the callee's frame left for the caller's catch or the
    /// unwinder.
    #[inline]
    pub fn call_inline(
        &mut self,
        callee: HeapPtr,
        args: &[Value],
        line: u32,
    ) -> Result<Option<Value>, Box<VmError>> {
        if !self.call_type_args.is_empty() {
            return Ok(None);
        }
        let Object::Function(function) = self.get_object(callee) else {
            return Ok(None);
        };
        if function.arity != args.len() {
            return Ok(None);
        }
        match super::native_body_fn(function) {
            Some(super::CompiledFn::Inline(body)) => {
                self.call_compiled(callee, body, args, line).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Push `args`, the value arguments of a virtual call in the interface
    /// method's declared slot order, as the arguments of the call a resumable
    /// body is about to request ([`Self::push_arg`]), laid over `callee`'s
    /// slots as the interpreter's `remap_call_arguments` does: `layout`
    /// names the optional slot each argument fills (`None` for a required
    /// one), and is `None` when the site pushes the method's own slots.
    /// Returns the callee's argument count.
    pub fn push_virtual_args(
        &mut self,
        callee: HeapPtr,
        layout: Option<&[Option<&str>]>,
        args: &[Value],
    ) -> Result<u32, Box<VmError>> {
        match self.lay_out_virtual_args(callee, layout, args) {
            Ok(count) => Ok(count),
            Err(error) => {
                // The type arguments `virtual_target` pushed belong to the
                // call that will not happen.
                self.call_type_args.clear();
                Err(Box::new(error.into()))
            }
        }
    }

    fn lay_out_virtual_args(
        &mut self,
        callee: HeapPtr,
        layout: Option<&[Option<&str>]>,
        args: &[Value],
    ) -> Result<u32, VmInternalError> {
        let count = |count: usize| u32::try_from(count).expect("an argument count fits u32");
        let Some(layout) = layout else {
            self.stack.extend(args.iter().copied());
            return Ok(count(args.len()));
        };
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

    /// Read interface field `field_index` of `iface` off `receiver`: the
    /// interpreter's `VirtualLoadField`.
    pub fn virtual_load_field(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        field_index: usize,
    ) -> Result<Value, VmError> {
        let head = self.interface_head(iface)?;
        let slot = self.resolve_virtual_field_slot(receiver, head, iface_args, field_index)?;
        self.load_field(receiver, slot)
    }

    /// Write interface field `field_index` of `iface` on `receiver`: the
    /// interpreter's `VirtualStoreField`.
    pub fn virtual_store_field(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        field_index: usize,
        value: Value,
    ) -> Result<(), VmError> {
        let head = self.interface_head(iface)?;
        let slot = self.resolve_virtual_field_slot(receiver, head, iface_args, field_index)?;
        self.store_field(receiver, slot, value)
    }

    /// `receiver.method` as a value, for an interface method: the
    /// interpreter's `MakeVirtualBoundMethod`. The bound method carries the
    /// implementation's frame type arguments followed by `method_type_args`.
    pub fn make_virtual_bound_method(
        &mut self,
        receiver: Value,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<Value, VmError> {
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

    /// `(Self as Iface).method` as a value, for a `Self` type known at
    /// compile time: the interpreter's `MakeVirtualFunction`. A capture-less
    /// closure whose captured type arguments are the implementation's frame
    /// followed by `method_type_args`.
    pub fn make_virtual_function(
        &mut self,
        self_ty: &RealizedTy,
        iface: HeapPtr,
        iface_args: &[RealizedTy],
        method: &str,
        method_type_args: &[RealizedTy],
    ) -> Result<Value, VmError> {
        // A static `Self` lives in the program image, whose implementations
        // the lexical world sees; a runtime package's type never reaches
        // compiled code (its heads are not known at compile time).
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

    /// The head of the interface object `iface`.
    fn interface_head(&self, iface: HeapPtr) -> Result<TypeHead, VmError> {
        match self.get_object(iface) {
            Object::Interface(declaration) => Ok(TypeHead::new(iface, declaration.type_tag)),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Interface.into(),
                got: ObjectType::of(other).into(),
            }
            .into()),
        }
    }

    /// `Self` for a dispatch on `receiver`: its concrete type.
    fn dispatch_self_ty(&self, receiver: Value) -> Result<RealizedTy, VmError> {
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

    /// The tag a [`VirtualSite`] keys `receiver` by, when its type tag alone
    /// decides the implementation.
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

    /// The object index of a compile-time object, which `compile_time_ptr`
    /// takes back to the pointer; `None` for a runtime object.
    fn compile_time_index(&self, ptr: HeapPtr) -> Option<u32> {
        if !self.heap.is_compile_time_ptr(ptr) {
            return None;
        }
        let offset = (ptr.as_ptr() as usize).checked_sub(self.image())?;
        u32::try_from(offset / std::mem::size_of::<Object>()).ok()
    }

    /// Where this heap's compile-time image starts: the identity of the image
    /// the pointers in a cached frame belong to.
    fn image(&self) -> usize {
        self.heap.compile_time_ptr(0).as_ptr() as usize
    }
}

/// Resolve `method` of `iface<iface_args>` for `self_ty`: the callee and its
/// complete frame, the implementation's type arguments followed by the
/// method's own (`BexVm::resolve_virtual_method`).
fn resolve_method(
    vm: &BexVm,
    resolver: ImplResolver<'_>,
    self_ty: &RealizedTy,
    iface: HeapPtr,
    iface_args: &[RealizedTy],
    method: &str,
    method_type_args: &[RealizedTy],
) -> Result<(HeapPtr, Vec<RealizedTy>), VmError> {
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
            tests::{run_with_compiled_bodies, test_int},
        },
    };

    type Out = Result<Step, Box<VmError>>;

    fn object(index: &AtomicU32) -> ObjectIndex {
        ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize)
    }

    /// A resumable body `fn F(x) { x.method() }` dispatching through `site`
    /// on the interface at `iface`, instantiated with `iface_args`.
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

    /// Explicit implementations: each receiver's own method runs, the site
    /// caches the last one, and an error the implementation raises is
    /// delivered to the compiled caller and leaves it.
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

    /// A callee that is itself a compiled body that cannot suspend runs in
    /// place of the request; one that is not (bytecode here) is requested.
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

    /// An adopted default body runs with the implementation's frame
    /// (`[Self]`), which the site keeps beside the callee: the second call
    /// is a hit that re-derives it.
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

    /// A structural default (`to_string` of a class with no override) is
    /// the interpreter's, and is never cached: a later-loaded implementation
    /// may replace it.
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

    /// A generic interface instantiated with a primitive selects the
    /// implementation at that instantiation.
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

    /// `receiver.method` as a value binds the receiver's implementation.
    #[test]
    fn bound_method_values_resolve_at_bind_time() {
        static SHAPE: AtomicU32 = AtomicU32::new(0);
        fn area_fn(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
            let iface = vm.idx_to_ptr(object(&SHAPE));
            Ok(vm.make_virtual_bound_method(args[0], iface, &[], "area", &[])?)
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

    /// An interface field reads and writes the slot the receiver's
    /// implementation links it to, wherever the class keeps it.
    #[test]
    fn interface_fields_follow_the_implementation_links() {
        static NAMED: AtomicU32 = AtomicU32::new(0);
        fn rename(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
            let iface = vm.idx_to_ptr(object(&NAMED));
            vm.virtual_store_field(args[0], iface, &[], 0, args[1])?;
            Ok(vm.virtual_load_field(args[0], iface, &[], 0)?)
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
