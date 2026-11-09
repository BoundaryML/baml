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
/// The VM's startup traceback renders each frame with `Function::name`, which
/// spells synthetic bodies (closures) without their package label:
/// `.<lambda(GLOBAL, 0)>`, where emitted sources carry `user.<lambda(..)>`.
fn vm_frame_name(function: &str) -> &str {
    match function.find(".<") {
        Some(dot) if !function[..dot].contains('<') => &function[dot..],
        _ => function,
    }
}
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
            .map(|f| (f.file, f.line, vm_frame_name(f.function))),
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
