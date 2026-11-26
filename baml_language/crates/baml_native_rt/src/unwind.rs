//! Throwing: finding the handler of an error and resuming compiled code
//! there.
//!
//! Compiled code throws by calling one of the stubs of [`crate::stubs`],
//! which never return to their caller. The stub makes a frame record and
//! calls a function here with it. That function returns, to the stub, where
//! to go on: a handler's address, the frame it runs in, and the value it
//! lands with, a tag and a payload. The stub puts those in place and
//! branches. What the value travels with, its trace or the context a
//! handler made of that, is left in the worker's scratch word, which the
//! handler reads before anything else. No Rust frame
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
    heap::{Mutator, Value, load},
    layout::{
        HANDLER_BINDS_CONTEXT, HANDLER_MAY_RETHROW, HANDLER_TAKES_ALL, HANDLER_TAKES_NULL,
        HANDLER_TAKES_OTHERS, HANDLER_TAKES_PANICS, HEADER_PANIC, INSTANCE_CLASS, TAG_ENUM_VARIANT,
        TAG_INSTANCE, TAG_STRING, TAG_STRING_SLICE, WELL_KNOWN_CONTEXT,
        WELL_KNOWN_DIVISION_BY_ZERO, WELL_KNOWN_INDEX_OUT_OF_BOUNDS, WELL_KNOWN_MAP_KEY_NOT_FOUND,
        WELL_KNOWN_NEGATIVE_BIT_SHIFT, WELL_KNOWN_STACK_OVERFLOW, WORD,
    },
};

/// The versions of the compiler's tables this reads.
const UNWIND_VERSION: usize = 3;
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
    /// What the handler lands with: the tag and the payload of the value
    /// thrown.
    tag: usize,
    payload: usize,
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
    /// What it takes whatever its flags say, a word each: a tag, or the
    /// address of the class object of the instances or of the enum's
    /// variants it takes. The address of the first, and how many.
    list: usize,
    count: usize,
}

/// What a handler asks of a thrown value to say whether it takes it.
#[derive(Clone, Copy)]
struct Thrown {
    is_null: bool,
    is_panic: bool,
    /// The type id of its tag; a string's is that of one holding its text.
    type_id: usize,
    /// The class object of an instance, or of an enum's variant; zero for
    /// any other value.
    class: usize,
}

impl Thrown {
    /// # Safety
    ///
    /// `error` is a value as a slot holds one: when its tag is a
    /// pointer's, its payload is a live object.
    unsafe fn of(error: Value) -> Self {
        let type_id = error.id();
        Self {
            is_null: error.is_null(),
            // A tag does not say whether its object is a panic: the
            // object's header does.
            // SAFETY: the caller's contract.
            is_panic: error.is_pointer() && unsafe { load(error.payload) } & HEADER_PANIC != 0,
            type_id: if type_id == TAG_STRING_SLICE {
                TAG_STRING
            } else {
                type_id
            },
            class: if matches!(type_id, TAG_INSTANCE | TAG_ENUM_VARIANT) {
                // SAFETY: the caller's contract: both hold the address of
                // a class object in their second word.
                unsafe { load(error.payload + INSTANCE_CLASS) }
            } else {
                0
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
        // No tag is the address of a class object, so one list holds both.
        // SAFETY: the table holds `count` words at `list`.
        !thrown.is_null
            && (0..self.count).any(|index| {
                let taken = unsafe { load(self.list + index * WORD) };
                taken == thrown.type_id || (thrown.class != 0 && taken == thrown.class)
            })
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
                            list: self.table + word(cursor + 8),
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
    let array = mutator.allocate(crate::array::size(length));
    // SAFETY: the array and its storage are the bytes just allocated, and
    // nothing else reaches the array yet.
    unsafe {
        crate::array::format(array, length);
        let addresses = addresses
            .iter()
            .map(|address| Value::int(address.cast_signed() as i64));
        crate::array::fill(array, addresses);
    }
    array
}

/// Find the handler of `error`, thrown by the compiled frame that last
/// called the runtime, and say where to resume. `context` is the trace the
/// error was caught with, or zero when it has none: a new throw, or one
/// whose trace nothing was going to read. What the handler reads as its
/// context is left in the worker's scratch word.
fn unwind(mutator: &mut Mutator, mut error: Value, mut context: usize, resume: &mut Resume) {
    // SAFETY: compiled code throws a value.
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
        mutator.root_value(&mut error);
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
        tag: error.tag,
        payload: error.payload,
        worker: std::ptr::from_mut(mutator) as usize,
    };
    // The scratch word is no root: nothing allocates between this store
    // and the handler's load of it.
    mutator.set_scratch(context);
}

#[expect(clippy::exit, reason = "the stack cannot be unwound")]
fn fatal(message: &str) -> ! {
    let _ = writeln!(std::io::stderr().lock(), "fatal: {message}");
    std::process::exit(70)
}

/// A `baml.panics.DivisionByZero`, whose dividend is an `int` here. May
/// collect.
fn division_by_zero(mutator: &mut Mutator, dividend: i64) -> usize {
    class::well_known(mutator, WELL_KNOWN_DIVISION_BY_ZERO, &[Init::Int(dividend)])
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

/// Throw the instance at `error`, which the runtime just made.
fn throw_instance(mutator: &mut Mutator, error: usize, resume: &mut Resume) {
    // SAFETY: a live instance.
    let error = unsafe { Value::reference(error) };
    unwind(mutator, error, 0, resume);
}

/// `throw` of the value of tag `tag` and payload `payload`: `frame` is the
/// frame record of the stub compiled code called.
pub(crate) extern "C" fn throw(
    tag: usize,
    payload: usize,
    _unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    unwind(mutator, Value { tag, payload }, 0, resume);
}

/// A caught value thrown again, with the context it was caught with.
pub(crate) extern "C" fn rethrow(
    tag: usize,
    payload: usize,
    context: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    unwind(mutator, Value { tag, payload }, context, resume);
}

pub(crate) extern "C" fn panic_division_by_zero(
    dividend: i64,
    _unused: usize,
    _also_unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = division_by_zero(mutator, dividend);
    throw_instance(mutator, error, resume);
}

pub(crate) extern "C" fn panic_negative_shift(
    count: i64,
    _unused: usize,
    _also_unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = negative_shift(mutator, count);
    throw_instance(mutator, error, resume);
}

pub(crate) extern "C" fn panic_index_out_of_bounds(
    index: i64,
    length: i64,
    _unused: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = index_out_of_bounds(mutator, index, length);
    throw_instance(mutator, error, resume);
}

pub(crate) extern "C" fn panic_map_key_not_found(
    _unused: usize,
    _also_unused: usize,
    _unused_too: usize,
    mutator: &mut Mutator,
    frame: usize,
    resume: &mut Resume,
) {
    mutator.enter(frame);
    let error = map_key_not_found(mutator);
    throw_instance(mutator, error, resume);
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
    _unused_too: usize,
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
    throw_instance(mutator, error, resume);
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
        let addresses = crate::array::values(trace);
        named(mutator, addresses.iter().map(|address| address.payload))
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
    unsafe { class::is_well_known(mutator, trace, WELL_KNOWN_CONTEXT) }
}

crate::stubs::anchored! {
    /// The symbol of [`error_context`].
    baml_rt_error_context => error_context
}

/// The `baml.errors.Context` a handler that reads its context makes of
/// what it landed with: of the trace the error travelled with, or the one
/// an earlier handler made, which is the same error still in flight. A
/// throw in the handler's own function arrives with no trace: it was
/// thrown through the frames that are still there.
///
/// # Safety
///
/// Called from compiled code with its mutator, the tag and the payload of
/// the value a handler landed with, and what it found in the scratch word.
unsafe extern "C" fn error_context(
    mutator: &mut Mutator,
    tag: usize,
    payload: usize,
    trace: usize,
) -> usize {
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
    class::context(mutator, Value { tag, payload }, &functions)
}

/// Nothing caught `error`: report it as `baml run` does, after the
/// functions it was thrown through. `what` says whose error it was, when
/// not the program's own; `trace` is zero or what the error travelled with.
///
/// # Safety
///
/// `error` is a value as a slot holds one, and `trace` zero, a live trace
/// or a live `baml.errors.Context`.
pub(crate) unsafe fn report(mutator: &Mutator, what: &str, error: Value, trace: usize) {
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

/// Nothing in the program's entry task caught the value of tag `tag` and
/// payload `payload`: report it, then the errors of tasks no `await` took,
/// and end the process. `trace` is zero or what the value travelled with.
///
/// A `baml.panics.Exit` is not reported: its code is the status the
/// process ends with, as `baml run` has it.
///
/// # Safety
///
/// Called from the compiled entry function's handler with what it landed
/// with and what it found in the scratch word.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_uncaught(
    mutator: &mut Mutator,
    tag: usize,
    payload: usize,
    trace: usize,
) -> ! {
    let error = Value { tag, payload };
    // SAFETY: the caller's contract.
    let status = unsafe { class::exit_status(mutator, error) };
    if status.is_none() {
        // SAFETY: the caller's contract.
        unsafe { report(mutator, "", error, trace) };
    }
    end(mutator, status.unwrap_or(crate::UNCAUGHT_EXIT_STATUS))
}

/// End the process with `status`, from whichever task: report the errors
/// of tasks no `await` took, drop what the heap owns, flush stdout and
/// write the `BAML_GC_STATS` line when collections are reported.
#[expect(clippy::exit, reason = "the program is over")]
pub(crate) fn end(mutator: &mut Mutator, status: i32) -> ! {
    crate::sched::report_unhandled(mutator);
    let owned_at_exit = mutator.owned();
    mutator.drop_owned();
    let _ = std::io::stdout().flush();
    mutator.report_gc_stats(owned_at_exit);
    std::process::exit(status)
}
