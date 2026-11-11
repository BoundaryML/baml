//! Instance-owned global slots. Initialization is eager and runs once; pending
//! reads are errors, rather than recursively evaluating a second initializer.
use crate::{Dyn, Error, R};
use std::cell::RefCell;

struct Slot {
    value: Dyn,
    initialized: bool,
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            value: Dyn { tag: 2, payload: 0 },
            initialized: false,
        }
    }
}
#[derive(Default)]
pub(crate) struct Globals {
    slots: RefCell<Vec<Slot>>,
    pointer_slots: RefCell<Vec<usize>>,
    string_literals: RefCell<Vec<usize>>,
    integer_value: std::cell::Cell<i64>,
    integer_text: std::cell::Cell<usize>,
    state: std::cell::Cell<u8>,
    pub(crate) pointer_types: &'static [bool],
}
impl Globals {
    pub(crate) fn new(pointer_types: &'static [bool]) -> Self {
        Self {
            pointer_types,
            ..Default::default()
        }
    }
    pub(crate) fn roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        for pointer in self.string_literals.borrow_mut().iter_mut() {
            if *pointer != 0 {
                visit(pointer);
            }
        }
        if self.integer_text.get() != 0 {
            visit(self.integer_text.as_ptr());
        }
        let mut slots = self.slots.borrow_mut();
        for &index in self.pointer_slots.borrow().iter() {
            let slot = &mut slots[index];
            if slot.initialized && self.pointer_types.get(slot.value.tag as usize) == Some(&true) {
                visit((&raw mut slot.value.payload).cast());
            }
        }
    }
}
thread_local! {
    pub(crate) static CURRENT: std::cell::Cell<*const Globals> = const { std::cell::Cell::new(std::ptr::null()) };
}
fn with<T>(f: impl FnOnce(&Globals) -> T) -> T {
    CURRENT.with(|current| {
        assert!(!current.get().is_null(), "globals outside an instance");
        unsafe { f(&*current.get()) }
    })
}
/// Lazily materialize a compiler-indexed literal, shared for this instance.
/// Strings are immutable, and the pool contributes precise roots independently
/// of eager global initialization. No pool borrow crosses a managed allocation.
pub fn string_literal(index: usize, text: &'static str) -> R<usize> {
    with(|globals| {
        let cached = globals
            .string_literals
            .borrow()
            .get(index)
            .copied()
            .unwrap_or(0);
        if cached != 0 {
            return Ok(cached);
        }
        let pointer = unsafe { crate::strings::Cx::current() }
            .string(text)?
            .as_raw();
        let mut literals = globals.string_literals.borrow_mut();
        if literals.len() <= index {
            literals.resize(index + 1, 0);
        }
        literals[index] = pointer;
        Ok(pointer)
    })
}

/// A bounded cache for the pure primitive conversion. Publish a new entry only
/// after construction succeeds; no root/cache borrow crosses allocation.
pub(crate) fn integer_string(value: i64, create: impl FnOnce() -> R<usize>) -> R<usize> {
    with(|globals| {
        let cached = globals.integer_text.get();
        if cached != 0 && globals.integer_value.get() == value {
            return Ok(cached);
        }
        let pointer = create()?;
        globals.integer_value.set(value);
        globals.integer_text.set(pointer);
        Ok(pointer)
    })
}

pub fn begin(count: usize) -> R<bool> {
    with(|globals| match globals.state.get() {
        2 => Ok(false),
        1 => Err(Error::internal(
            "program initialization did not complete".into(),
        )),
        _ => {
            globals.slots.borrow_mut().resize_with(count, Slot::default);
            globals.state.set(1);
            Ok(true)
        }
    })
}
pub fn is_ready() -> bool {
    with(|globals| globals.state.get() == 2)
}
pub fn complete() {
    with(|globals| globals.state.set(2));
}
pub fn load(index: usize) -> R<Dyn> {
    with(|globals| {
        let slots = globals.slots.borrow();
        match slots.get(index).filter(|slot| slot.initialized) {
            Some(slot) => Ok(slot.value),
            None => Err(Error::internal(format!(
                "global slot {index} was read before its initializer ran"
            ))),
        }
    })
}
pub fn store(index: usize, value: Dyn, _pointer: bool) {
    with(|globals| {
        globals.slots.borrow_mut()[index] = Slot {
            value,
            initialized: true,
        };
        if globals.pointer_types.get(value.tag as usize) == Some(&true) {
            let mut roots = globals.pointer_slots.borrow_mut();
            if !roots.contains(&index) {
                roots.push(index);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Instance,
        language::*,
        strings::{Cx, Str},
    };
    static DESCRIPTORS: &[baml_rt_gc::Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
    ];
    #[test]
    fn literals_are_lazy_precise_roots_shared_between_calls_and_owned_by_each_instance() {
        fn entry() -> R<i64> {
            let first = string_literal(3, "shared🦀literal")?;
            let allocated = unsafe { (&*crate::HEAP.get()).stats().allocated_since_gc };
            let again = string_literal(3, "shared🦀literal")?;
            assert_eq!(first, again);
            assert_eq!(
                unsafe { (&*crate::HEAP.get()).stats().allocated_since_gc },
                allocated
            );
            for _ in 0..100 {
                let mut cx = unsafe { Cx::current() };
                cx.string("temporary garbage")?;
                cx.poll()?;
            }
            let value = string_literal(3, "shared🦀literal")?;
            assert_eq!(
                unsafe { Str::from_raw(value) }.as_str(&unsafe { Cx::current() }),
                "shared🦀literal"
            );
            Ok(1)
        }
        const POINTERS: &[bool] = &[
            false, false, false, true, true, true, true, true, true, true,
        ];
        for _ in 0..2 {
            let mut instance = Instance::new(DESCRIPTORS, POINTERS);
            instance.set_stress(true);
            for _ in 0..3 {
                assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
            }
        }
    }
    #[test]
    fn globals_are_precise_roots_and_initialize_once_per_instance() {
        fn entry() -> R<i64> {
            if begin(2)? {
                assert!(
                    load(1)
                        .unwrap_err()
                        .message
                        .contains("before its initializer ran")
                );
                let mut cx = unsafe { Cx::current() };
                let value = cx.string("kept only by the global slot")?;
                store(
                    0,
                    Dyn {
                        tag: 3,
                        payload: value.as_raw() as u64,
                    },
                    true,
                );
                store(
                    1,
                    Dyn {
                        tag: 0,
                        payload: 41,
                    },
                    false,
                );
                complete();
            }
            for _ in 0..100 {
                unsafe { Cx::current() }.string("churn")?;
            }
            let value = load(0)?;
            assert_eq!(
                unsafe { Str::from_raw(value.payload as usize) }.as_str(&unsafe { Cx::current() }),
                "kept only by the global slot"
            );
            let old = load(1)?.payload;
            store(
                1,
                Dyn {
                    tag: 0,
                    payload: old + 1,
                },
                false,
            );
            Ok(old as i64)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 41);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 42);
        let mut fresh = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true,
            ],
        );
        assert_eq!(unsafe { fresh.run_baml(entry) }.unwrap(), 41);
    }
}

/// The initializer runner is a host boundary, just as engine construction is
/// for the VM. Observation of the original thrown value finishes before this
/// host diagnostic is constructed.
pub fn init_failed(error: Box<Error>, package: &str) -> Box<Error> {
    let global = error
        .frames
        .iter()
        .find_map(|frame| bex_vm_types::init_let_helper_global(frame.function));
    let traceback = bex_vm_types::errors::format_traceback(
        error
            .frames
            .iter()
            .rev()
            .map(|f| (f.file, f.line, f.function)),
    );
    let detail = if error.value == 0 {
        if let Some(detail) = error
            .display_value
            .as_deref()
            .filter(|text| text.starts_with("global `"))
        {
            format!("{traceback}{detail}")
        } else {
            format!("{traceback}VM internal error: {}", error.message)
        }
    } else {
        let value = error
            .display_value
            .as_ref()
            .cloned()
            .unwrap_or_else(|| format!("{} {{message: {:?}}}", error.class, error.message));
        format!("{traceback}uncaught throw: {value}")
    };
    let context = global.map_or_else(
        || format!("`$init` of package `{package}` failed"),
        |global| {
            format!("`$init` of package `{package}` failed while initializing global `{global}`")
        },
    );
    let mut error = Error::internal(format!(
        "Package initialization failed: {context}:\n{detail}"
    ));
    error.class = "InitFailed";
    error
}
pub fn load_named(index: usize, name: &str) -> R<Dyn> {
    load(index).map_err(|mut error| {
        // Keep the VM's raw error for recording; naming belongs to the host's
        // startup diagnostic, after the initializer invocation has completed.
        error.display_value=Some(format!("global `{name}` was read before its initializer ran — globals are initialized in dependency order, but this read goes through a call the compiler cannot follow (a virtual interface method or a function value); reach it directly from the initializer, or move the read into the function that needs it"));
        error
    })
}
