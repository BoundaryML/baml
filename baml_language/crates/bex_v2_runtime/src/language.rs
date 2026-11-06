//! Typed phase-one language operations. Exception objects use the same declared
//! class fields as the interpreter; stack frames are appended only on failure.
use crate::Instance;
use baml_type::Int63;
use bex_v2_heap::{Descriptor, ElementLayout, Elements, Field, Header, Kind, SlotKind};
use std::sync::atomic::{AtomicU8, Ordering};
pub type Dyn = bex_v2_heap::TaggedSlot;
pub type R<T> = Result<T, Box<Error>>;

pub const STRING_DESC: Descriptor = Descriptor {
    kind: Kind::String,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: Some(Elements {
        offset: 16,
        layout: ElementLayout::Scalar,
    }),
    type_id: 3,
    fields: &[],
    finalizer: None,
};
const MESSAGE_FIELDS: &[Field] = &[Field {
    name: "message",
    offset: 8,
    slot: SlotKind::Pointer,
    type_id: 3,
}];
pub const OVERFLOW_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 16,
    ptr_slots: (8, 1),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 4,
    fields: MESSAGE_FIELDS,
    finalizer: None,
};
pub const SHIFT_DESC: Descriptor = Descriptor {
    type_id: 5,
    ..OVERFLOW_DESC
};
pub const CANCEL_DESC: Descriptor = Descriptor {
    type_id: 6,
    ..OVERFLOW_DESC
};
pub const ALLOC_DESC: Descriptor = Descriptor {
    type_id: 8,
    ..OVERFLOW_DESC
};
pub const STACK_DESC: Descriptor = Descriptor {
    type_id: 7,
    ..OVERFLOW_DESC
};

/// Leave room for error allocation and fiber unwinding below each BAML frame.
#[inline(always)]
pub fn check_stack() -> R<()> {
    let marker = 0u8;
    if (&raw const marker as usize).saturating_sub(crate::STACK_LIMIT.get()) < 64 * 1024 {
        Err(from_panic(bex_vm_types::errors::VmPanic::StackOverflow))
    } else {
        Ok(())
    }
}

#[repr(C)]
struct MessagePanic {
    header: Header,
    message: usize,
}
#[derive(Debug)]
pub struct Source {
    pub file: &'static str,
    pub function: &'static str,
    pub lines: &'static [usize],
}
#[derive(Debug)]
pub struct Frame {
    pub file: &'static str,
    pub function: &'static str,
    pub line: usize,
}
#[derive(Debug)]
pub struct Error {
    /// Actual v2 heap instance of the panic class, zero only for internal errors.
    pub value: usize,
    pub class: &'static str,
    pub message: String,
    pub frames: Vec<Frame>,
    pub display_value: Option<String>,
}
impl Error {
    #[cold]
    pub fn internal(message: String) -> Box<Self> {
        Box::new(Self {
            value: 0,
            class: "InternalError",
            message,
            frames: Vec::new(),
            display_value: None,
        })
    }
    #[cold]
    pub fn at(mut self: Box<Self>, source: &'static Source, site: usize) -> Box<Self> {
        self.frames.push(Frame {
            file: source.file,
            function: source.function,
            line: source.lines[site],
        });
        self
    }
}
#[cold]
fn panic_value(desc: u32, class: &'static str, message: String) -> Box<Error> {
    // SAFETY: called only from a running generated root. The string is rooted
    // in this stack frame across the class allocation; initialization cannot GC.
    unsafe {
        let string = match crate::try_alloc(0, 16 + message.len().next_multiple_of(8)) {
            Ok(p) => p,
            Err(e) => return e,
        };
        string.add(8).cast::<usize>().write(message.len());
        std::ptr::copy_nonoverlapping(message.as_ptr(), string.add(16), message.len());
        let object = match crate::try_alloc(desc, std::mem::size_of::<MessagePanic>()) {
            Ok(p) => p.cast::<MessagePanic>(),
            Err(e) => return e,
        };
        bex_v2_heap::write_ptr(object.cast(), &raw mut (*object).message, string as usize);
        Box::new(Error {
            value: object as usize,
            class,
            message,
            frames: Vec::new(),
            display_value: None,
        })
    }
}
#[cold]
fn overflow(l: i64, op: char, r: i64) -> Box<Error> {
    from_panic(bex_vm_types::errors::VmPanic::IntegerOverflow {
        message: format!("{l} {op} {r} overflows int"),
    })
}
#[inline(always)]
pub fn add(l: i64, r: i64) -> R<i64> {
    let v = l.wrapping_add(r);
    if Int63::new(v).is_some() {
        Ok(v)
    } else {
        Err(overflow(l, '+', r))
    }
}
#[inline(always)]
pub fn sub(l: i64, r: i64) -> R<i64> {
    let v = l.wrapping_sub(r);
    if Int63::new(v).is_some() {
        Ok(v)
    } else {
        Err(overflow(l, '-', r))
    }
}
#[inline(always)]
pub fn mul(l: i64, r: i64) -> R<i64> {
    let (v, o) = l.overflowing_mul(r);
    if !o & Int63::new(v).is_some() {
        Ok(v)
    } else {
        Err(overflow(l, '*', r))
    }
}
#[cold]
fn negative(count: i64) -> Box<Error> {
    from_panic(bex_vm_types::errors::VmPanic::NegativeBitShift {
        message: format!("bit shift count is negative: {count}"),
    })
}
#[inline(always)]
pub fn shl(l: i64, r: i64) -> R<i64> {
    Int63::new(l)
        .expect("valid i63")
        .shift_left(r)
        .map(|v| v.get())
        .map_err(|_| negative(r))
}
#[inline(always)]
pub fn shr(l: i64, r: i64) -> R<i64> {
    Int63::new(l)
        .expect("valid i63")
        .shift_right(r)
        .map(|v| v.get())
        .map_err(|_| negative(r))
}
#[inline(always)]
pub fn nonnull(p: usize) -> R<usize> {
    if p != 0 {
        Ok(p)
    } else {
        use bex_vm_types::{
            Value,
            errors::VmInternalError,
            types::{ObjectType, Type},
        };
        Err(Error::internal(
            VmInternalError::TypeError {
                expected: ObjectType::Instance.into(),
                got: Type::of(&Value::NULL, |_| unreachable!("null is not an object")),
            }
            .to_string(),
        ))
    }
}
#[inline(always)]
pub fn observe(word: &AtomicU8) {
    if word.load(Ordering::Relaxed) != 0 {
        unimplemented!("runtime v2 telemetry producer");
    }
}
#[inline(always)]
pub fn poll() -> R<()> {
    // SAFETY: generated code calls this only from a running root task.
    unsafe { crate::safepoint() }.map_err(|e| match e {
        bex_v2_loop::Err::Cancelled => from_panic(bex_vm_types::errors::VmPanic::Cancelled),
        e => Error::internal(format!("{e:?}")),
    })
}

/// Native speedtest wire protocol with an optional plain single-call mode.
/// Every warmup and measured return is checked, not just the first call.
pub fn driver(descriptors: &'static [Descriptor], entry: fn() -> R<i64>) -> std::process::ExitCode {
    match driver_inner(descriptors, entry) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let traceback = bex_vm_types::errors::format_traceback(
                error
                    .frames
                    .iter()
                    .rev()
                    .map(|f| (f.file, f.line, f.function)),
            );
            if error.value == 0 {
                eprintln!("error: {traceback}VM internal error: {}", error.message);
            } else {
                let value = error
                    .display_value
                    .unwrap_or_else(|| format!("{} {{message: {:?}}}", error.class, error.message));
                eprintln!("error: {traceback}uncaught throw: {value}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
fn driver_inner(descriptors: &'static [Descriptor], entry: fn() -> R<i64>) -> R<()> {
    if baml_env::raw_var("BAML_TELEMETRY").is_some_and(|level| level != "off") {
        return Err(Error::internal(
            "unsupported in runtime v2: telemetry other than off".into(),
        ));
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut jobs = None;
    let mut warmup = 1usize;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let dst = match arg.as_str() {
            "--jobs" => &mut jobs,
            "--warmup" => {
                let value = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::internal("invalid --warmup".into()))?;
                warmup = value;
                continue;
            }
            _ => return Err(Error::internal(format!("unsupported argument {arg}"))),
        };
        *dst = Some(
            args.next()
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| Error::internal("invalid --jobs".into()))?,
        );
    }
    let mut pointers = vec![false; 3];
    pointers.extend(vec![true; descriptors.len()]);
    let pointers = Box::leak(pointers.into_boxed_slice());
    let mut instance = Instance::new(descriptors, pointers);
    // SAFETY: compiler admission and descriptor construction enforce this ABI.
    let result = unsafe { instance.run_baml(entry) }?;
    let Some(jobs) = jobs else {
        println!("{result}");
        return Ok(());
    };
    for _ in 1..warmup {
        if unsafe { instance.run_baml(entry) }? != result {
            return Err(Error::internal("warmup result mismatch".into()));
        }
    }
    let start = std::time::Instant::now();
    for _ in 0..jobs {
        if unsafe { instance.run_baml(entry) }? != result {
            return Err(Error::internal("timed result mismatch".into()));
        }
    }
    let elapsed = start.elapsed().as_nanos();
    println!(
        "{}",
        serde_json::json!({"result":result.to_string(),"jobs":jobs,"elapsed_ns":elapsed})
    );
    Ok(())
}

#[cold]
fn from_panic(panic: bex_vm_types::errors::VmPanic) -> Box<Error> {
    use bex_vm_types::errors::VmPanic;
    // Same class and declaration-order fields as BexVm::panic_to_exception_value.
    match panic {
        VmPanic::IntegerOverflow { message } => {
            panic_value(1, "baml.panics.IntegerOverflow", message)
        }
        VmPanic::NegativeBitShift { message } => {
            panic_value(2, "baml.panics.NegativeBitShift", message)
        }
        VmPanic::StackOverflow => {
            panic_value(4, "baml.panics.StackOverflow", "stack overflow".into())
        }
        VmPanic::Cancelled => panic_value(3, "baml.panics.Cancelled", "operation cancelled".into()),
        _ => Error::internal("unsupported runtime v2 panic constructor".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static DESCRIPTORS: &[Descriptor] = &[STRING_DESC, OVERFLOW_DESC, SHIFT_DESC, CANCEL_DESC];

    #[test]
    fn i63_boundaries_and_heap_exception_under_stress() {
        fn entry() -> R<i64> {
            assert_eq!(
                nonnull(0).unwrap_err().message,
                "type error: expected instance, got any"
            );
            assert_eq!(add((1 << 62) - 2, 1).unwrap(), (1 << 62) - 1);
            assert_eq!(sub(-(1 << 62) + 1, 1).unwrap(), -(1 << 62));
            assert_eq!(shl(1, 63).unwrap(), 0);
            assert_eq!(shr(-1, 63).unwrap(), -1);
            for shift in [false, true] {
                let error = if shift {
                    shl(1, -1).unwrap_err()
                } else {
                    add((1 << 62) - 1, 1).unwrap_err()
                };
                // No collecting operation occurs while these errors are in the
                // host box. Inspect the allocated language object itself.
                unsafe {
                    let object = error.value as *const MessagePanic;
                    assert!(!object.is_null());
                    let string = (*object).message as *const u8;
                    let len = string.add(8).cast::<usize>().read();
                    let bytes = std::slice::from_raw_parts(string.add(16), len);
                    assert_eq!(bytes, error.message.as_bytes());
                }
            }
            Ok(0)
        }
        let mut instance =
            Instance::new(DESCRIPTORS, &[false, false, false, true, true, true, true]);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 0);
        assert!(instance.stats().collections >= 4);
    }
}

/// The normal heap has already performed its full-collection retry. Construct
/// the language value exclusively from the pre-reserved traced block.
#[cold]
pub(crate) fn allocation_failure(bytes: usize) -> Box<Error> {
    let bex_vm_types::errors::VmPanic::AllocFailure { message } =
        bex_vm_types::errors::VmPanic::allocation_failure(bytes)
    else {
        unreachable!()
    };
    unsafe {
        let string = crate::emergency_alloc(0, 16 + message.len().next_multiple_of(8));
        string.add(8).cast::<usize>().write(message.len());
        std::ptr::copy_nonoverlapping(message.as_ptr(), string.add(16), message.len());
        let object =
            crate::emergency_alloc(5, std::mem::size_of::<MessagePanic>()).cast::<MessagePanic>();
        bex_v2_heap::write_ptr(object.cast(), &raw mut (*object).message, string as usize);
        Box::new(Error {
            value: object as usize,
            class: "baml.panics.AllocFailure",
            message,
            frames: Vec::new(),
            display_value: None,
        })
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
    ];

    #[test]
    fn exhausted_heap_constructs_traced_language_failure_and_recovers() {
        fn entry() -> R<i64> {
            let error = unsafe { crate::try_alloc(1, 16) }.unwrap_err();
            assert_eq!(error.class, "baml.panics.AllocFailure");
            assert_eq!(error.message, "Allocation of 16 bytes failed");
            let value = error.value;
            drop(error);
            unsafe { crate::safepoint() }.unwrap();
            unsafe {
                let object = value as *const MessagePanic;
                assert_eq!((*object).header.desc, 5);
                let message = (*object).message as *const u8;
                let len = message.add(8).cast::<usize>().read();
                assert_eq!(
                    std::slice::from_raw_parts(message.add(16), len),
                    b"Allocation of 16 bytes failed"
                );
            }
            Ok(42)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[false, false, false, true, true, true, true, true, true],
        );
        instance.set_heap_limit(0);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 42);
        assert!(instance.stats().collections >= 2);
        instance.set_heap_limit(32768);
        fn recover() -> R<i64> {
            unsafe { crate::try_alloc(1, 16) }?;
            Ok(7)
        }
        assert_eq!(unsafe { instance.run_baml(recover) }.unwrap(), 7);
    }
}

pub const INDEX_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 24,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 9,
    fields: &[
        Field {
            name: "index",
            offset: 8,
            slot: SlotKind::Scalar,
            type_id: 0,
        },
        Field {
            name: "length",
            offset: 16,
            slot: SlotKind::Scalar,
            type_id: 0,
        },
    ],
    finalizer: None,
};

#[cold]
fn index_error(index: i64, length: usize) -> Box<Error> {
    let message = bex_vm_types::errors::VmPanic::IndexOutOfBounds { index, length }.to_string();
    unsafe {
        let object = match crate::try_alloc(6, 24) {
            Ok(p) => p,
            Err(e) => return e,
        };
        object.add(8).cast::<i64>().write(index);
        object.add(16).cast::<i64>().write(length as i64);
        Box::new(Error {
            value: object as usize,
            class: "baml.panics.IndexOutOfBounds",
            message,
            frames: Vec::new(),
            display_value: Some(format!(
                "baml.panics.IndexOutOfBounds {{index: {index}, length: {length}}}"
            )),
        })
    }
}

/// Fixed-size typed list element access. Lists carry their logical length at 8.
/// # Safety
/// `list` is a rooted list with eight-byte elements, or null. The resulting
/// address must not be used after its owning list becomes unreachable.
#[inline(always)]
pub unsafe fn list_slot(list: usize, index: i64) -> R<*mut u8> {
    if list == 0 {
        use bex_vm_types::{
            Value,
            errors::VmInternalError,
            types::{ObjectType, Type},
        };
        return Err(Error::internal(
            VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: Type::of(&Value::NULL, |_| unreachable!()),
            }
            .to_string(),
        ));
    }
    let len = unsafe { ((list + 8) as *const usize).read() };
    // Match bex_vm::array_index::resolve_index_inline: negative subscripts
    // count from the end, while errors retain the original signed index.
    let resolved = i64::try_from(len)
        .ok()
        .and_then(|n| usize::try_from(if index < 0 { index + n } else { index }).ok())
        .filter(|&i| i < len);
    match resolved {
        Some(i) => Ok(unsafe { (list as *mut u8).add(16 + i * 8) }),
        None => Err(index_error(index, len)),
    }
}

/// # Safety
/// `slot` belongs to `object`, whose descriptor traces this tagged slot.
#[inline(always)]
pub unsafe fn write_dyn(object: *mut u8, slot: *mut Dyn, value: Dyn) {
    unsafe {
        (*slot).tag = value.tag;
        bex_v2_heap::write_ptr(
            object,
            (&raw mut (*slot).payload).cast(),
            value.payload as usize,
        );
    }
}

#[cfg(test)]
mod list_tests {
    use super::*;
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
        Descriptor {
            kind: Kind::List,
            size: 8,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 16,
                layout: ElementLayout::Pointer,
            }),
            type_id: 10,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::Class,
            size: 24,
            ptr_slots: (0, 0),
            tagged_slots: (8, 1),
            elem: None,
            type_id: 11,
            fields: &[],
            finalizer: None,
        },
    ];
    #[test]
    fn pointer_lists_and_interface_slots_survive_collection() {
        fn entry() -> R<i64> {
            unsafe {
                let list = crate::try_alloc(7, 24)?;
                list.add(8).cast::<usize>().write(1);
                let owner = crate::try_alloc(8, 24)?;
                write_dyn(
                    owner,
                    owner.add(8).cast(),
                    Dyn {
                        tag: 10,
                        payload: list as u64,
                    },
                );
                let child = crate::try_alloc(6, 24)?;
                child.add(8).cast::<i64>().write(123);
                bex_v2_heap::write_ptr(list, list_slot(list as usize, 0)?.cast(), child as usize);
                crate::safepoint().unwrap();
                let slot = owner.add(8).cast::<Dyn>().read();
                let child =
                    list_slot(slot.payload as usize, 0)?.cast::<usize>().read() as *const u8;
                assert_eq!(child.add(8).cast::<i64>().read(), 123);
                assert_eq!(
                    list_slot(slot.payload as usize, -1)?,
                    list_slot(slot.payload as usize, 0)?
                );
                for index in [-2, 1] {
                    let e = list_slot(slot.payload as usize, index).unwrap_err();
                    assert_eq!(e.class, "baml.panics.IndexOutOfBounds");
                    let value = e.value as *const u8;
                    assert_eq!(value.add(8).cast::<i64>().read(), index);
                    assert_eq!(value.add(16).cast::<i64>().read(), 1);
                }
            }
            Ok(123)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 123);
        assert!(instance.stats().collections > 0);
    }
}
