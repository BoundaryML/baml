//! Managed closure environments and shared mutable capture cells.
//! Every capture points to the original cell, including recursive and forwarded
//! captures. Host allocations never own managed environments.
use crate::{Dyn, R};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind};

pub const CELL_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 24,
    ptr_slots: (0, 0),
    tagged_slots: (8, 1),
    elem: None,
    type_id: 13,
    fields: &[],
    finalizer: None,
};
pub const CLOSURE_DESC: Descriptor = Descriptor {
    kind: Kind::Closure,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: Some(Elements {
        offset: 56,
        layout: ElementLayout::Pointer,
    }),
    type_id: 14,
    fields: &[],
    finalizer: None,
};
pub const fn cell_descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        type_id,
        ..CELL_DESC
    }
}
pub const fn closure_descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        type_id,
        ..CLOSURE_DESC
    }
}
pub type Code = unsafe fn(usize, usize, &[Dyn]) -> R<Dyn>;
#[repr(C)]
struct Environment {
    header: Header,
    len: usize,
    object: usize,
    code: Code,
    name: Option<&'static str>,
    kind: u64,
}
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Closure {
    pointer: usize,
    owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Closure {
    /// # Safety
    /// The address names a live closure in the current instance's heap.
    pub unsafe fn from_raw(raw: usize) -> Self {
        Self {
            pointer: raw,
            owner: std::marker::PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
}
unsafe impl crate::lists::Slot for Closure {
    const SIZE: usize = 8;
    const LAYOUT: ElementLayout = ElementLayout::Pointer;
    unsafe fn read(slot: *const u8) -> Self {
        unsafe { Self::from_raw(slot.cast::<usize>().read()) }
    }
    unsafe fn write(self, object: *mut u8, slot: *mut u8) {
        unsafe { baml_rt_gc::write_ptr(object, slot.cast(), self.pointer) }
    }
}
/// # Safety
/// `desc` describes a capture cell; the value remains rooted until return.
pub unsafe fn cell(desc: u32, value: Dyn) -> R<usize> {
    unsafe {
        let p = crate::try_alloc(desc, 24)?;
        crate::write_dyn(p, p.add(8).cast(), value);
        Ok(p as usize)
    }
}
/// # Safety
/// `cell` names a managed capture cell.
#[inline(always)]
pub unsafe fn load(cell: usize) -> Dyn {
    unsafe { *((cell + 8) as *const Dyn) }
}
/// # Safety
/// Both operands belong to the current heap, and `cell` is a capture cell.
#[inline(always)]
pub unsafe fn store(cell: usize, value: Dyn) {
    unsafe { crate::write_dyn(cell as *mut u8, (cell + 8) as *mut Dyn, value) }
}
/// # Safety
/// `desc` describes an environment; `cells` are live managed capture cells
/// rooted by the caller's stack.
pub unsafe fn new(desc: u32, object: usize, code: Code, cells: &[usize]) -> R<usize> {
    unsafe { create(desc, object, code, cells, None, 0) }
}
/// # Safety
/// Same managed-heap and descriptor contract as `new`. An object id must
/// always identify the same declaration, code and name in this instance.
pub unsafe fn function(desc: u32, object: usize, code: Code, name: &'static str) -> R<usize> {
    unsafe { crate::function_value(object, || create(desc, object, code, &[], Some(name), 1)) }
}
unsafe fn create(
    desc: u32,
    object: usize,
    code: Code,
    cells: &[usize],
    name: Option<&'static str>,
    kind: u64,
) -> R<usize> {
    unsafe {
        let p = crate::try_alloc(desc, 56 + cells.len() * 8)?;
        let env = p.cast::<Environment>();
        (*env).len = cells.len();
        (*env).object = object;
        (*env).code = code;
        (*env).name = name;
        (*env).kind = kind;
        for (i, &cell) in cells.iter().enumerate() {
            baml_rt_gc::write_ptr(p, p.add(56 + i * 8).cast(), cell);
        }
        Ok(p as usize)
    }
}
/// # Safety
/// `env` is a live closure with a capture at `index`.
#[inline(always)]
pub unsafe fn capture(env: usize, index: usize) -> usize {
    unsafe {
        debug_assert!(index < (*(env as *const Environment)).len);
        *((env + 56 + index * 8) as *const usize)
    }
}
/// # Safety
/// `env` is null or a live environment; non-null arguments match its
/// declared signature and remain rooted.
#[inline(always)]
pub unsafe fn call(env: usize, site: usize, args: &[Dyn]) -> R<Dyn> {
    unsafe {
        object(env)?;
        ((*(env as *const Environment)).code)(env, site, args)
    }
}

/// # Safety
/// `env` is null or a live managed closure.
#[inline(always)]
pub unsafe fn object(env: usize) -> R<usize> {
    if env == 0 {
        use bex_vm_types::{
            Value,
            errors::VmInternalError,
            types::{FunctionType, Type},
        };
        return Err(crate::Error::internal(
            VmInternalError::TypeError {
                expected: FunctionType::Callable.into(),
                got: Type::of(&Value::NULL, |_| unreachable!("null is not an object")),
            }
            .to_string(),
        ));
    }
    Ok(unsafe { (*(env as *const Environment)).object })
}

#[cfg(test)]
mod tests {
    use super::*;
    static DESCRIPTORS: &[Descriptor] = &[
        crate::STRING_DESC,
        crate::OVERFLOW_DESC,
        crate::SHIFT_DESC,
        crate::CANCEL_DESC,
        crate::STACK_DESC,
        crate::ALLOC_DESC,
        crate::INDEX_DESC,
        crate::json::PARSE_DESC,
        crate::json::DECODE_DESC,
        crate::DIV_DESC,
        CELL_DESC,
        CLOSURE_DESC,
    ];
    unsafe fn add(env: usize, _: usize, args: &[Dyn]) -> R<Dyn> {
        unsafe {
            let cell = capture(env, 0);
            let n = crate::add(load(cell).payload as i64, args[0].payload as i64)?;
            let value = Dyn {
                tag: 0,
                payload: n as u64,
            };
            store(cell, value);
            Ok(value)
        }
    }
    fn job() -> R<i64> {
        unsafe {
            let shared = cell(
                10,
                Dyn {
                    tag: 0,
                    payload: 10,
                },
            )?;
            let first = new(11, 1, add, &[shared])?;
            let second = new(11, 1, add, &[shared])?;
            // Force the cell to remain reachable only through environments.
            std::hint::black_box(shared);
            let mut total = 0;
            for _ in 0..100 {
                crate::poll()?;
                total += call(first, 0, &[Dyn { tag: 0, payload: 1 }])?.payload as i64;
                total += call(second, 0, &[Dyn { tag: 0, payload: 1 }])?.payload as i64;
            }
            Ok(total)
        }
    }
    #[test]
    fn shared_cells_are_traced_through_environments_under_stress() {
        let mut instance = crate::Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(job) }.unwrap(), 22100);
    }
    #[test]
    fn null_callees_report_the_vm_callable_type_error() {
        use bex_vm_types::{
            Value,
            errors::VmInternalError,
            types::{FunctionType, Type},
        };
        let expected = VmInternalError::TypeError {
            expected: FunctionType::Callable.into(),
            got: Type::of(&Value::NULL, |_| unreachable!()),
        }
        .to_string();
        assert_eq!(unsafe { object(0) }.unwrap_err().message, expected);
        assert_eq!(unsafe { call(0, 0, &[]) }.unwrap_err().message, expected);
    }

    #[test]
    fn function_identity_and_roots_survive_successive_jobs() {
        thread_local! { static PREVIOUS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
        unsafe fn identity(_: usize, _: usize, args: &[Dyn]) -> R<Dyn> {
            Ok(args[0])
        }
        fn entry() -> R<i64> {
            unsafe {
                let first = function(11, 99, identity, "identity")?;
                let second = function(11, 99, identity, "identity")?;
                assert_eq!(first, second);
                PREVIOUS.with(|previous| {
                    let old = previous.replace(first);
                    if old != 0 {
                        assert_eq!(old, first);
                    }
                });
                for n in 0..100 {
                    let _ = cell(10, Dyn { tag: 0, payload: n })?;
                }
                Ok(call(
                    first,
                    0,
                    &[Dyn {
                        tag: 0,
                        payload: 42,
                    }],
                )?
                .payload as i64)
            }
        }
        PREVIOUS.set(0);
        let mut instance = crate::Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 42);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 42);
    }
}

/// # Safety
/// `env` is a rooted, live environment in the active instance.
pub unsafe fn name(env: usize) -> Option<&'static str> {
    unsafe { (*(env as *const Environment)).name }
}

/// # Safety
/// Both descriptors match this module's cell/environment layouts. Receiver
/// and generated adapter belong to the active instance.
pub unsafe fn bind(
    cell_desc: u32,
    env_desc: u32,
    object: usize,
    code: Code,
    receiver: Dyn,
) -> R<usize> {
    unsafe {
        let cell = cell(cell_desc, receiver)?;
        create(env_desc, object, code, &[cell], None, 2)
    }
}
/// # Safety
/// `env` is a live closure environment.
pub unsafe fn is_bound(env: usize) -> bool {
    unsafe { (*(env as *const Environment)).kind == 2 }
}
