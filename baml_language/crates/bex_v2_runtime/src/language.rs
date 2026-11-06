//! Typed phase-one language operations. Exception objects use the same declared
//! class fields as the interpreter; stack frames are appended only on failure.
use crate::{Instance, alloc};
use baml_type::Int63;
use bex_v2_heap::{Descriptor, ElementLayout, Elements, Field, Header, Kind, SlotKind};
use std::sync::atomic::{AtomicU8, Ordering};
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
    type_id: 0,
    fields: &[],
    finalizer: None,
};
const MESSAGE_FIELDS: &[Field] = &[Field {
    name: "message",
    offset: 8,
    slot: SlotKind::Pointer,
    type_id: 0,
}];
pub const OVERFLOW_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 16,
    ptr_slots: (8, 1),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 1,
    fields: MESSAGE_FIELDS,
    finalizer: None,
};
pub const SHIFT_DESC: Descriptor = Descriptor {
    type_id: 2,
    ..OVERFLOW_DESC
};
pub const CANCEL_DESC: Descriptor = Descriptor {
    type_id: 3,
    ..OVERFLOW_DESC
};
pub const STACK_DESC: Descriptor = Descriptor {
    type_id: 4,
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
}
impl Error {
    #[cold]
    pub fn internal(message: String) -> Box<Self> {
        Box::new(Self {
            value: 0,
            class: "InternalError",
            message,
            frames: Vec::new(),
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
        let string = alloc(0, 16 + message.len().next_multiple_of(8));
        string.add(8).cast::<usize>().write(message.len());
        std::ptr::copy_nonoverlapping(message.as_ptr(), string.add(16), message.len());
        let object = alloc(desc, std::mem::size_of::<MessagePanic>()).cast::<MessagePanic>();
        bex_v2_heap::write_ptr(object.cast(), &raw mut (*object).message, string as usize);
        Box::new(Error {
            value: object as usize,
            class,
            message,
            frames: Vec::new(),
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
                eprintln!(
                    "error: {traceback}uncaught throw: {} {{message: {:?}}}",
                    error.class, error.message
                );
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
    let pointers = Box::leak(vec![true; descriptors.len()].into_boxed_slice());
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
        let mut instance = Instance::new(DESCRIPTORS, &[true; 4]);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 0);
        assert!(instance.stats().collections >= 4);
    }
}
