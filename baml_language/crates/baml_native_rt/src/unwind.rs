//! Throwing: finding the handler of an error and resuming compiled code
//! there.
//!
//! Compiled code throws by calling one of the stubs of [`crate::stubs`],
//! which never return to their caller. The stub makes a frame record and
//! calls a function here with it. That function returns, to the stub, where
//! to go on: a handler's address, the frame it runs in, and the two words
//! it lands with. The stub puts those in place and branches. No Rust frame
//! is abandoned, and nothing unwinds through Rust.
//!
//! The search has two phases. The first pops nothing: it walks the frame
//! records from the throw outwards, looks each return address up in the
//! table of calls that have a handler, and stops at the first one found.
//! Whatever the handler needs made (the trace of the throw, when it reads
//! the error's context) is allocated then, with every frame still in place
//! and described by its stack map, so a collection there is like any other.
//! The second phase allocates nothing: it only says where to resume.
//!
//! Only the frame records are read, through the chain of frame pointers:
//! the unwinder never derives a stack pointer for a frame that has no
//! handler.

use std::{cell::OnceCell, io::Write};

use crate::{
    division_by_zero_text, float_text,
    heap::{Mutator, current_frame, load, store},
    index_out_of_bounds_text,
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, BOX_SIZE, BOX_VALUE, STORAGE_CAPACITY,
        STORAGE_ELEMENTS, TYPE_ARRAY_OF_VALUES, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT,
        TYPE_ERROR, TYPE_ID_MASK, TYPE_PANIC, TYPE_STRING, TYPE_VALUES, WORD,
    },
    negative_shift_text, string,
};

/// Set in a site's flags when its handler reads the context it lands with.
const SITE_WANTS_CONTEXT: usize = 1;

/// The versions of the compiler's tables this reads.
const UNWIND_VERSION: usize = 1;
const FUNCTIONS_VERSION: usize = 2;

/// Where compiled code goes on after a throw, as the stub reads it.
#[repr(C)]
#[derive(Default)]
pub struct Resume {
    /// The handler's frame: its stack pointer and frame pointer.
    sp: usize,
    fp: usize,
    /// The handler's first instruction.
    pc: usize,
    /// What the handler lands with: the error and its context.
    error: usize,
    context: usize,
    /// The worker the handler runs on.
    worker: usize,
}

/// A call with a handler.
#[derive(Clone, Copy)]
struct Site {
    return_address: usize,
    handler: usize,
    /// The frame pointer minus the stack pointer at the call.
    frame_offset: usize,
    flags: usize,
}

/// The program's table of calls with a handler, decoded when first thrown
/// through.
pub(crate) struct Unwinder {
    table: usize,
    functions: usize,
    sites: OnceCell<Vec<Site>>,
    /// The return addresses of the last trace taken, kept for its room.
    trace: Vec<usize>,
}

/// Read the 32-bit word at `address`.
///
/// # Safety
///
/// `address` is inside one of the compiler's tables.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read_unaligned() as usize }
}

impl Unwinder {
    pub(crate) fn new(table: usize, functions: usize) -> Self {
        Self {
            table,
            functions,
            sites: OnceCell::new(),
            trace: Vec::new(),
        }
    }

    /// The sites of the table, sorted by return address. The format is the
    /// compiler's, described where it emits the table.
    fn sites(&self) -> &[Site] {
        self.sites.get_or_init(|| {
            let mut sites = Vec::new();
            // SAFETY: `table` is the address of the compiler's table.
            unsafe {
                assert!(word(self.table) == UNWIND_VERSION, "an unwind table");
                let mut cursor = self.table + 8;
                for _ in 0..word(self.table + 4) {
                    let function = load(cursor);
                    let count = word(cursor + 8);
                    cursor += 16;
                    for _ in 0..count {
                        sites.push(Site {
                            return_address: function + word(cursor),
                            handler: function + word(cursor + 4),
                            frame_offset: word(cursor + 8),
                            flags: word(cursor + 12),
                        });
                        cursor += 16;
                    }
                }
            }
            sites.sort_unstable_by_key(|site| site.return_address);
            sites
        })
    }

    fn handler(&self, return_address: usize) -> Option<Site> {
        let sites = self.sites();
        sites
            .binary_search_by_key(&return_address, |site| site.return_address)
            .ok()
            .map(|index| sites[index])
    }

    /// The name of the function whose code holds `address`.
    fn function_name(&self, address: usize) -> Option<&'static str> {
        const RECORD: usize = 24;
        // SAFETY: `functions` is the address of the compiler's function
        // table, whose records are in ascending address order and whose
        // names are in the executable's data.
        unsafe {
            if word(self.functions) != FUNCTIONS_VERSION {
                return None;
            }
            let records = self.functions + 16;
            let (mut low, mut high) = (0, word(self.functions + 4));
            while low < high {
                let middle = usize::midpoint(low, high);
                let record = records + middle * RECORD;
                let start = load(record);
                if address < start {
                    high = middle;
                } else if address - start > word(record + 8) {
                    // A return address can be one past a function's last
                    // instruction, so the end is inside.
                    low = middle + 1;
                } else {
                    let name = self.functions + word(record + 20);
                    return Some(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        (name + 4) as *const u8,
                        word(name),
                    )));
                }
            }
        }
        None
    }
}

/// The return address into each compiled frame and that frame's frame
/// pointer, from the one that last called the runtime outwards.
fn frames(mutator: &Mutator) -> impl Iterator<Item = (usize, usize)> {
    let base = mutator.base_frame();
    let mut next = mutator.top_frame();
    std::iter::from_fn(move || {
        let (pc, fp) = next;
        if fp >= base {
            return None;
        }
        // SAFETY: `fp` is the frame record of a compiled frame.
        next = unsafe { (load(fp + WORD), load(fp)) };
        Some((pc, fp))
    })
}

/// An `int[]` of `addresses`. May collect.
fn int_array(mutator: &mut Mutator, addresses: &[usize]) -> usize {
    let length = addresses.len();
    let array = mutator.allocate(ARRAY_SIZE + STORAGE_ELEMENTS + length * WORD);
    // SAFETY: the array and its storage are the bytes just allocated.
    unsafe {
        let storage = array + ARRAY_SIZE;
        let data = storage + STORAGE_ELEMENTS;
        store(array, TYPE_ARRAY_OF_VALUES);
        store(array + ARRAY_LENGTH, length);
        store(array + ARRAY_DATA, data);
        store(storage, TYPE_VALUES);
        store(storage + STORAGE_CAPACITY, length);
        std::ptr::copy_nonoverlapping(addresses.as_ptr(), data as *mut usize, length);
    }
    array
}

/// Find the handler of `error`, thrown by the compiled frame that last
/// called the runtime, and say where to resume. `context` is zero for a new
/// throw.
fn unwind(mutator: &mut Mutator, mut error: usize, mut context: usize, resume: &mut Resume) {
    // The first phase: nothing is popped. One walk finds the handler and,
    // when the handler reads a context that is yet to be made, goes on to
    // the outermost frame for the trace: the return address into every
    // compiled frame, innermost first.
    let mut trace = std::mem::take(&mut mutator.unwind.trace);
    trace.clear();
    let mut found = None;
    for (pc, fp) in frames(mutator) {
        if found.is_none()
            && let Some(site) = mutator.unwind.handler(pc)
        {
            found = Some((site, fp));
            if context != 0 || site.flags & SITE_WANTS_CONTEXT == 0 {
                break;
            }
        }
        trace.push(pc);
    }
    let Some((site, fp)) = found else {
        // The function that enters compiled code catches everything.
        fatal("a throw found no handler");
    };
    if context == 0 && site.flags & SITE_WANTS_CONTEXT != 0 {
        mutator.root(&mut error);
        context = int_array(mutator, &trace);
        mutator.unroot(1);
    }
    mutator.unwind.trace = trace;
    // The second phase: nothing allocates. A popped frame's telemetry slot
    // would be completed here, with how the frame ended.
    *resume = Resume {
        sp: fp - site.frame_offset,
        fp,
        pc: site.handler,
        error,
        context,
        worker: std::ptr::from_mut(mutator) as usize,
    };
}

#[expect(clippy::exit, reason = "the stack cannot be unwound")]
fn fatal(message: &str) -> ! {
    let _ = writeln!(std::io::stderr().lock(), "fatal: {message}");
    std::process::exit(70)
}

/// A panic that is written as `text`. May collect.
fn panic_object(mutator: &mut Mutator, text: &str) -> usize {
    let mut text = string::from_str(mutator, text);
    mutator.root(&mut text);
    let object = mutator.allocate_object(TYPE_PANIC, BOX_SIZE);
    mutator.unroot(1);
    // SAFETY: the object is a header and one slot.
    unsafe { store(object + BOX_VALUE, text) };
    object
}

/// `throw`: `frame` is the frame record of the stub compiled code called.
pub(crate) extern "C" fn throw(
    error: usize,
    _cause: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    unwind(mutator, error, 0, resume);
}

/// A caught error thrown again, with the context it was caught with.
pub(crate) extern "C" fn rethrow(
    error: usize,
    context: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    unwind(mutator, error, context, resume);
}

pub(crate) extern "C" fn panic_division_by_zero(
    dividend: i64,
    _unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = panic_object(mutator, &division_by_zero_text(dividend));
    unwind(mutator, error, 0, resume);
}

pub(crate) extern "C" fn panic_negative_shift(
    count: i64,
    _unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = panic_object(mutator, &negative_shift_text(count));
    unwind(mutator, error, 0, resume);
}

pub(crate) extern "C" fn panic_index_out_of_bounds(
    index: i64,
    length: i64,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = panic_object(mutator, &index_out_of_bounds_text(index, length));
    unwind(mutator, error, 0, resume);
}

/// The `baml.panics.DivisionByZero` a handler in the dividing function
/// lands with.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_error_division_by_zero(mutator: &mut Mutator, dividend: i64) -> usize {
    mutator.enter(current_frame());
    panic_object(mutator, &division_by_zero_text(dividend))
}

/// The `baml.panics.NegativeBitShift` a handler in the shifting function
/// lands with.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_error_negative_shift(mutator: &mut Mutator, count: i64) -> usize {
    mutator.enter(current_frame());
    panic_object(mutator, &negative_shift_text(count))
}

/// The `baml.panics.IndexOutOfBounds` a handler in the indexing function
/// lands with.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_error_index_out_of_bounds(
    mutator: &mut Mutator,
    index: i64,
    length: i64,
) -> usize {
    mutator.enter(current_frame());
    panic_object(mutator, &index_out_of_bounds_text(index, length))
}

/// A thrown value as an uncaught throw of it is reported.
///
/// # Safety
///
/// `error` is zero or a live object.
unsafe fn error_text(mutator: &Mutator, error: usize) -> String {
    if error == 0 {
        return "null".to_owned();
    }
    // SAFETY: the caller's contract: a box is a header and one value.
    unsafe {
        let (type_id, held) = (load(error) & TYPE_ID_MASK, load(error + BOX_VALUE));
        match type_id {
            TYPE_STRING => format!("{:?}", string::text(error)),
            TYPE_ERROR | TYPE_PANIC => string::text(held).to_owned(),
            TYPE_BOX_INT => held.cast_signed().to_string(),
            TYPE_BOX_FLOAT => float_text(f64::from_bits(held as u64)),
            TYPE_BOX_BOOL => (held & 0xFF != 0).to_string(),
            enum_type if !mutator.is_reference(enum_type) => {
                mutator.variant_name(enum_type, held).to_owned()
            }
            // The type table does not yet say how a class instance, an
            // array or a function is written.
            _ => "<object>".to_owned(),
        }
    }
}

/// Nothing caught `error`: report it as `baml run` does, after the
/// functions it was thrown through, and end the process. `trace` is zero or
/// the trace of the throw.
///
/// # Safety
///
/// Called from the compiled entry function's handler with what it landed
/// with.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_uncaught(mutator: &Mutator, error: usize, trace: usize) -> ! {
    let mut report = String::new();
    if trace != 0 {
        // SAFETY: a trace is an `int[]` of return addresses.
        let addresses = unsafe {
            std::slice::from_raw_parts(
                load(trace + ARRAY_DATA) as *const usize,
                load(trace + ARRAY_LENGTH),
            )
        };
        // The outermost frame is the entry function's caller, which has no
        // name a program knows.
        let named: Vec<&str> = addresses
            .iter()
            .rev()
            .filter_map(|address| mutator.unwind.function_name(*address))
            .skip(1)
            .collect();
        if !named.is_empty() {
            report.push_str("Traceback (most recent call last):\n");
            for name in named {
                report.push_str("  in ");
                report.push_str(name);
                report.push('\n');
            }
        }
    }
    let _ = std::io::stdout().flush();
    let _ = write!(std::io::stderr().lock(), "{report}");
    // SAFETY: the caller's contract.
    crate::uncaught(&unsafe { error_text(mutator, error) })
}
