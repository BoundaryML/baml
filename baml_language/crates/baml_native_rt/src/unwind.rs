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
//! records from the throw outwards and looks each return address up in the
//! table of calls that have handlers. The table says what each handler
//! takes, so a handler that would only pass the error on is not landed in:
//! the walk goes by it. The first handler that takes the error is where the
//! throw resumes.
//!
//! The same walk decides whether the trace of the throw is made, by going
//! on past that handler for as long as the answer is open. A handler that
//! takes the error and reads its context wants the trace. One that takes
//! it and cannot throw it again ends its flight: nothing will read a
//! trace. One that takes it and may throw it again (a `defer`, always)
//! leaves the question to the handlers after it, as does one that does not
//! take it. The function that enters compiled code takes everything and
//! reads the context, to report it. A trace that is wanted is allocated
//! then, with every frame still in place and described by its stack map,
//! so a collection there is like any other, and travels with the error
//! through every landing and rethrow; one that is not wanted costs nothing.
//! The second phase allocates nothing: it only says where to resume.
//!
//! Only the frame records are read, through the chain of frame pointers:
//! the unwinder never derives a stack pointer for a frame that has no
//! handler.

use std::{cell::OnceCell, io::Write};

use crate::{
    class::{self, Init},
    heap::{Mutator, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, HANDLER_BINDS_CONTEXT, HANDLER_MAY_RETHROW,
        HANDLER_TAKES_ALL, HANDLER_TAKES_NULL, HANDLER_TAKES_OTHERS, HANDLER_TAKES_PANICS,
        HEADER_INTS, HEADER_PANIC, STORAGE_CAPACITY, STORAGE_ELEMENTS, STRING_TYPES_END,
        TYPE_ARRAY_OF_VALUES, TYPE_ID_MASK, TYPE_STRING, TYPE_VALUES, WELL_KNOWN_CONTEXT,
        WELL_KNOWN_DIVISION_BY_ZERO, WELL_KNOWN_INDEX_OUT_OF_BOUNDS, WELL_KNOWN_MAP_KEY_NOT_FOUND,
        WELL_KNOWN_NEGATIVE_BIT_SHIFT, WELL_KNOWN_STACK_OVERFLOW, WORD,
    },
};

/// The versions of the compiler's tables this reads.
const UNWIND_VERSION: usize = 2;
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
    /// What the handler lands with: the error and its trace, or zero for
    /// a trace nothing will read.
    error: usize,
    context: usize,
    /// The worker the handler runs on.
    worker: usize,
}

/// A call with handlers: the handlers of the blocks it is in, innermost
/// first.
#[derive(Clone, Copy)]
struct Site {
    return_address: usize,
    /// The frame pointer minus the stack pointer at the call.
    frame_offset: usize,
    /// Its handlers, in [`Tables::handlers`].
    first: usize,
    count: usize,
}

/// A handler and what it takes.
#[derive(Clone, Copy)]
struct Handler {
    /// Its first instruction.
    address: usize,
    flags: u32,
    /// The type ids it takes whatever its flags say: the address of the
    /// first, and how many.
    ids: usize,
    count: usize,
}

/// What a handler asks of an error to say whether it takes it.
#[derive(Clone, Copy)]
struct Thrown {
    is_null: bool,
    is_panic: bool,
    /// Its type id; a string's is that of one holding its text.
    type_id: usize,
}

impl Thrown {
    /// # Safety
    ///
    /// `error` is zero or a live object.
    unsafe fn of(error: usize) -> Self {
        if error == 0 {
            return Self {
                is_null: true,
                is_panic: false,
                type_id: 0,
            };
        }
        // SAFETY: the caller's contract.
        let header = unsafe { load(error) };
        let type_id = header & TYPE_ID_MASK;
        Self {
            is_null: false,
            is_panic: header & HEADER_PANIC != 0,
            type_id: if type_id < STRING_TYPES_END {
                TYPE_STRING
            } else {
                type_id
            },
        }
    }
}

impl Handler {
    fn takes(&self, thrown: Thrown) -> bool {
        let by_flags = if thrown.is_null {
            HANDLER_TAKES_ALL | HANDLER_TAKES_OTHERS | HANDLER_TAKES_NULL
        } else if thrown.is_panic {
            HANDLER_TAKES_ALL | HANDLER_TAKES_PANICS
        } else {
            HANDLER_TAKES_ALL | HANDLER_TAKES_OTHERS
        };
        if self.flags & by_flags != 0 {
            return true;
        }
        // SAFETY: the table holds `count` ids at `ids`.
        !thrown.is_null
            && (0..self.count).any(|index| unsafe { word(self.ids + index * 4) } == thrown.type_id)
    }
}

/// The compiler's table, decoded.
#[derive(Default)]
struct Tables {
    /// Sorted by return address.
    sites: Vec<Site>,
    handlers: Vec<Handler>,
}

/// The program's table of calls with handlers, decoded when first thrown
/// through.
pub(crate) struct Unwinder {
    table: usize,
    /// The address of the program's function table, or zero.
    pub(crate) functions: usize,
    tables: OnceCell<Tables>,
    /// The return addresses of the last walk, kept for its room.
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
            tables: OnceCell::new(),
            trace: Vec::new(),
        }
    }

    /// The table, decoded. The format is the compiler's, described where it
    /// emits the table.
    fn tables(&self) -> &Tables {
        self.tables.get_or_init(|| {
            let mut tables = Tables::default();
            // SAFETY: `table` is the address of the compiler's table.
            unsafe {
                assert!(word(self.table) == UNWIND_VERSION, "an unwind table");
                let mut cursor = self.table + 8;
                for _ in 0..word(self.table + 4) {
                    let function = load(cursor);
                    let (sites, handlers) = (word(cursor + 8), word(cursor + 12));
                    cursor += 16;
                    let first = tables.handlers.len();
                    for _ in 0..sites {
                        tables.sites.push(Site {
                            return_address: function + word(cursor),
                            frame_offset: word(cursor + 4),
                            first: first + word(cursor + 8),
                            count: word(cursor + 12),
                        });
                        cursor += 16;
                    }
                    for _ in 0..handlers {
                        tables.handlers.push(Handler {
                            address: function + word(cursor),
                            flags: ((cursor + 4) as *const u32).read_unaligned(),
                            ids: self.table + word(cursor + 8),
                            count: word(cursor + 12),
                        });
                        cursor += 16;
                    }
                }
            }
            tables
                .sites
                .sort_unstable_by_key(|site| site.return_address);
            tables
        })
    }

    /// The handlers of the call that returns to `return_address`,
    /// innermost first, with the call's frame offset.
    fn handlers(&self, return_address: usize) -> Option<(&[Handler], usize)> {
        let tables = self.tables();
        let index = tables
            .sites
            .binary_search_by_key(&return_address, |site| site.return_address)
            .ok()?;
        let site = tables.sites[index];
        Some((
            &tables.handlers[site.first..site.first + site.count],
            site.frame_offset,
        ))
    }
}

/// The name of the compiled function whose code holds `address`, from the
/// function table at `functions`. `None` for an address outside compiled
/// code, and for any address when there is no table, as in a test of the
/// runtime alone.
pub(crate) fn function_name(functions: usize, address: usize) -> Option<&'static str> {
    const RECORD: usize = 24;
    if functions == 0 {
        return None;
    }
    // SAFETY: `functions` is the address of the compiler's function
    // table, whose records are in ascending address order and whose
    // names are in the executable's data.
    unsafe {
        if word(functions) != FUNCTIONS_VERSION {
            return None;
        }
        let records = functions + 16;
        let (mut low, mut high) = (0, word(functions + 4));
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
                let name = functions + word(record + 20);
                return Some(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                    (name + 4) as *const u8,
                    word(name),
                )));
            }
        }
    }
    None
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
        store(array, TYPE_ARRAY_OF_VALUES | HEADER_INTS);
        store(array + ARRAY_LENGTH, length);
        store(array + ARRAY_DATA, data);
        store(storage, TYPE_VALUES);
        store(storage + STORAGE_CAPACITY, length);
        std::ptr::copy_nonoverlapping(addresses.as_ptr(), data as *mut usize, length);
    }
    array
}

/// Find the handler of `error`, thrown by the compiled frame that last
/// called the runtime, and say where to resume. `context` is the trace the
/// error was caught with, or zero when it has none: a new throw, or one
/// whose trace nothing was going to read.
fn unwind(mutator: &mut Mutator, mut error: usize, mut context: usize, resume: &mut Resume) {
    // SAFETY: compiled code throws null or a live object.
    let thrown = unsafe { Thrown::of(error) };
    // The first phase: nothing is popped. One walk finds the handler to
    // land in and whether the trace is wanted, and when it is, goes on to
    // the outermost frame for it: the return address into every compiled
    // frame, innermost first.
    let mut trace = std::mem::take(&mut mutator.unwind.trace);
    trace.clear();
    let mut found = None;
    // An error that has its trace needs none made.
    let mut wanted = (context != 0).then_some(false);
    'frames: for (pc, fp) in frames(mutator) {
        if wanted != Some(true)
            && let Some((handlers, frame_offset)) = mutator.unwind.handlers(pc)
        {
            for handler in handlers {
                if !handler.takes(thrown) {
                    continue;
                }
                if found.is_none() {
                    found = Some((handler.address, fp - frame_offset, fp));
                }
                if wanted.is_some() {
                    break 'frames;
                }
                if handler.flags & HANDLER_BINDS_CONTEXT != 0 {
                    wanted = Some(true);
                    break;
                }
                if handler.flags & HANDLER_MAY_RETHROW == 0 {
                    wanted = Some(false);
                    break 'frames;
                }
            }
        }
        trace.push(pc);
    }
    let Some((pc, sp, fp)) = found else {
        // The function that enters compiled code takes everything.
        fatal("a throw found no handler");
    };
    if wanted == Some(true) {
        mutator.root(&mut error);
        context = int_array(mutator, &trace);
        mutator.unroot(1);
    }
    mutator.unwind.trace = trace;
    // The second phase: nothing allocates. A popped frame's telemetry slot
    // would be completed here, with how the frame ended.
    *resume = Resume {
        sp,
        fp,
        pc,
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

/// A `baml.panics.DivisionByZero`. Its dividend is an `int` or a `bigint`,
/// so the `int` is in a box. May collect.
fn division_by_zero(mutator: &mut Mutator, dividend: i64) -> usize {
    class::well_known(
        mutator,
        WELL_KNOWN_DIVISION_BY_ZERO,
        &[Init::BoxedInt(dividend)],
    )
}

/// A `baml.panics.NegativeBitShift`. May collect.
fn negative_shift(mutator: &mut Mutator, count: i64) -> usize {
    class::well_known(
        mutator,
        WELL_KNOWN_NEGATIVE_BIT_SHIFT,
        &[Init::Text(&format!("bit shift count is negative: {count}"))],
    )
}

/// A `baml.panics.IndexOutOfBounds`. May collect.
fn index_out_of_bounds(mutator: &mut Mutator, index: i64, length: i64) -> usize {
    class::well_known(
        mutator,
        WELL_KNOWN_INDEX_OUT_OF_BOUNDS,
        &[Init::Int(index), Init::Int(length)],
    )
}

/// A `baml.panics.MapKeyNotFound`. Its key is a string whatever the map's
/// keys are, and the interpreter does not say which key it was: the text
/// is the one it gives. May collect.
fn map_key_not_found(mutator: &mut Mutator) -> usize {
    class::well_known(
        mutator,
        WELL_KNOWN_MAP_KEY_NOT_FOUND,
        &[Init::Text("(unknown)")],
    )
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
    let error = division_by_zero(mutator, dividend);
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
    let error = negative_shift(mutator, count);
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
    let error = index_out_of_bounds(mutator, index, length);
    unwind(mutator, error, 0, resume);
}

pub(crate) extern "C" fn panic_map_key_not_found(
    _unused: usize,
    _also_unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = map_key_not_found(mutator);
    unwind(mutator, error, 0, resume);
}

/// What `baml.panics.StackOverflow` says, as the interpreter has it.
const STACK_OVERFLOW: &str = "stack overflow";

/// A compiled function was entered with its stack pointer below its task's
/// limit: `baml.panics.StackOverflow`, thrown from that function's caller
/// on, since the function has done nothing yet.
///
/// The limit leaves part of the stack to the runtime, and this runs in it,
/// below a frame the limit did not allow for. The panic is made and thrown
/// when most of that part is still free; a function whose own frame took
/// more ends the program here, with what the uncaught panic would say.
pub(crate) extern "C" fn panic_stack_overflow(
    _unused: usize,
    _also_unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    if !crate::sched::has_room_to_throw(mutator, frame) {
        crate::uncaught(&format!(
            "baml.panics.StackOverflow {{message: {STACK_OVERFLOW:?}}}"
        ));
    }
    mutator.enter(frame);
    let error = class::well_known(
        mutator,
        WELL_KNOWN_STACK_OVERFLOW,
        &[Init::Text(STACK_OVERFLOW)],
    );
    unwind(mutator, error, 0, resume);
}

crate::stubs::anchored! {
    /// The symbol of [`error_map_key_not_found`].
    baml_rt_error_map_key_not_found => error_map_key_not_found
}

/// The `baml.panics.MapKeyNotFound` a handler in the indexing function
/// lands with.
extern "C" fn error_map_key_not_found(mutator: &mut Mutator) -> usize {
    map_key_not_found(mutator)
}

crate::stubs::anchored! {
    /// The symbol of [`error_division_by_zero`].
    baml_rt_error_division_by_zero => error_division_by_zero
}

/// The `baml.panics.DivisionByZero` a handler in the dividing function
/// lands with.
extern "C" fn error_division_by_zero(mutator: &mut Mutator, dividend: i64) -> usize {
    division_by_zero(mutator, dividend)
}

crate::stubs::anchored! {
    /// The symbol of [`error_negative_shift`].
    baml_rt_error_negative_shift => error_negative_shift
}

/// The `baml.panics.NegativeBitShift` a handler in the shifting function
/// lands with.
extern "C" fn error_negative_shift(mutator: &mut Mutator, count: i64) -> usize {
    negative_shift(mutator, count)
}

crate::stubs::anchored! {
    /// The symbol of [`error_index_out_of_bounds`].
    baml_rt_error_index_out_of_bounds => error_index_out_of_bounds
}

/// The `baml.panics.IndexOutOfBounds` a handler in the indexing function
/// lands with.
extern "C" fn error_index_out_of_bounds(mutator: &mut Mutator, index: i64, length: i64) -> usize {
    index_out_of_bounds(mutator, index, length)
}

/// The functions an error was thrown through, innermost first, from what
/// it travels with: the trace of the throw, or the context a handler made
/// of that.
///
/// # Safety
///
/// `trace` is a live trace or `baml.errors.Context`.
unsafe fn thrown_through(mutator: &Mutator, trace: usize) -> Vec<&'static str> {
    // SAFETY: the caller's contract: a trace is an `int[]` of return
    // addresses.
    unsafe {
        if is_context(mutator, trace) {
            return class::context_functions(mutator, trace);
        }
        let addresses = std::slice::from_raw_parts(
            load(trace + ARRAY_DATA) as *const usize,
            load(trace + ARRAY_LENGTH),
        );
        named(mutator, addresses.iter().copied())
    }
}

/// The names of the functions these return addresses are in. The last is
/// in the function that enters compiled code, which has no name a program
/// knows. Nor has an entry the runtime calls a function of the program
/// through, whose name is empty, and nor has the runtime itself.
fn named(mutator: &Mutator, addresses: impl Iterator<Item = usize>) -> Vec<&'static str> {
    let mut names: Vec<&str> = addresses
        .filter_map(|address| function_name(mutator.unwind.functions, address))
        .collect();
    names.pop();
    names.retain(|name| !name.is_empty());
    names
}

/// Whether what an error travels with is already its context.
///
/// # Safety
///
/// `trace` is a live trace or `baml.errors.Context`.
unsafe fn is_context(mutator: &Mutator, trace: usize) -> bool {
    // SAFETY: the caller's contract.
    unsafe { load(trace) & TYPE_ID_MASK == class::well_known_type(mutator, WELL_KNOWN_CONTEXT) }
}

crate::stubs::anchored! {
    /// The symbol of [`error_context`].
    baml_rt_error_context => error_context
}

/// The `baml.errors.Context` a handler that reads its context lands with:
/// made of the trace the error travelled with, or the one an earlier
/// handler made, which is the same error still in flight. A throw in the
/// handler's own function arrives with no trace: it was thrown through
/// the frames that are still there.
///
/// # Safety
///
/// Called from compiled code with its mutator, an error and what the
/// handler landed with.
unsafe extern "C" fn error_context(mutator: &mut Mutator, error: usize, trace: usize) -> usize {
    // SAFETY: the caller's contract.
    let functions = unsafe {
        if trace == 0 {
            let addresses: Vec<usize> = frames(mutator).map(|(pc, _)| pc).collect();
            named(mutator, addresses.into_iter())
        } else if is_context(mutator, trace) {
            return trace;
        } else {
            thrown_through(mutator, trace)
        }
    };
    class::context(mutator, error, &functions)
}

/// Nothing caught `error`: report it as `baml run` does, after the
/// functions it was thrown through. `what` says whose error it was, when
/// not the program's own; `trace` is zero or what the error travelled with.
///
/// # Safety
///
/// `error` is zero or a live object, and `trace` zero, a live trace or a
/// live `baml.errors.Context`.
pub(crate) unsafe fn report(mutator: &Mutator, what: &str, error: usize, trace: usize) {
    let mut report = String::new();
    if trace != 0 {
        // SAFETY: the caller's contract.
        let named = unsafe { thrown_through(mutator, trace) };
        if !named.is_empty() {
            report.push_str("Traceback (most recent call last):\n");
            for name in named.into_iter().rev() {
                report.push_str("  in ");
                report.push_str(name);
                report.push('\n');
            }
        }
    }
    report.push_str("error: ");
    report.push_str(what);
    report.push_str("uncaught throw: ");
    // SAFETY: the caller's contract.
    unsafe { class::write_value(mutator, &mut report, error, None) };
    report.push('\n');
    let _ = std::io::stdout().flush();
    let _ = write!(std::io::stderr().lock(), "{report}");
}

/// Nothing in the program's entry task caught `error`: report it, then the
/// errors of tasks no `await` took, and end the process. `trace` is zero or
/// what the error travelled with.
///
/// A `baml.panics.Exit` is not reported: its code is the status the
/// process ends with, as `baml run` has it.
///
/// # Safety
///
/// Called from the compiled entry function's handler with what it landed
/// with.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_uncaught(mutator: &mut Mutator, error: usize, trace: usize) -> ! {
    // SAFETY: the caller's contract.
    let status = unsafe { class::exit_status(mutator, error) };
    if status.is_none() {
        // SAFETY: the caller's contract.
        unsafe { report(mutator, "", error, trace) };
    }
    end(mutator, status.unwrap_or(crate::UNCAUGHT_EXIT_STATUS))
}

/// End the process with `status`, from whichever task: report the errors
/// of tasks no `await` took, drop what the heap owns and flush stdout.
#[expect(clippy::exit, reason = "the program is over")]
pub(crate) fn end(mutator: &mut Mutator, status: i32) -> ! {
    crate::sched::report_unhandled(mutator);
    mutator.drop_owned();
    let _ = std::io::stdout().flush();
    std::process::exit(status)
}
