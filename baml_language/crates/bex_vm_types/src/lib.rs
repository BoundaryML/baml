//! Baml virtual machine.
//!
//! This crate implements a stack based virtual machine similar to the `CPython`
//! VM or Lox VM from [Crafting Interpreters](https://craftinginterpreters.com/).
//!
//! Main entry point is `Vm::exec` (in `bex_vm` crate) which runs the VM cycle:
//! 1. Decode Instruction.
//! 2. Execute Instruction.
//! 3. Increment instruction pointer and repeat loop.
//!
//! The instructions that the VM runs are defined in [`Instruction`] enum.

pub mod bytecode;
pub mod heap_ptr;
pub mod indexable;
mod roots;
pub mod types;

pub use bytecode::{
    BinOp, Bytecode, CmpOp, Instruction, JumpTableData, UnaryOp, VizExecDelta, VizExecEvent,
    VizNodeMeta, VizNodeType,
};
pub use heap_ptr::HeapPtr;
pub use indexable::{GlobalIndex, GlobalPool, ObjectIndex, ObjectPool, StackIndex};
pub use roots::RootHaver;
pub use types::{
    Class, ClassField, ClientBuildMeta, ClientBuildType, CollectorRef, ConstValue, Enum,
    EnumVariant, Function, FunctionKind, FunctionMeta, Future, Instance, MediaValue, Object,
    ObjectType, PanicClass, PendingFuture, Program, PromptAst, RetryPolicyMeta, SysOp,
    SysOpErrorCategory, SysOpPanicCategory, TestArgValue, TestCase, Value, Variant,
    sys_op_for_path, type_tags,
};

/// Used to check if the VM should yield early.
///
/// ## Why do we need this?
///
/// #### On multi-threaded targets
///
/// A thread that is running an infinite loop (or that otherwise just doesn't yield for a long time)
/// would prevent all other threads from continuing whenever they are waiting for a GC to complete,
/// since GCs require all threads to be parked.
///
/// #### On single-threaded targets (WASM)
///
/// A thread that is running an infinite loop (or that otherwise just doesn't yield for a long time)
/// would prevent the VM from running any other instructions, since the VM is single-threaded.
/// This may eventually crash the program since the GC cannot run, resulting in unpreventable heap growth.
///
/// #### Solution
///
/// [`EarlyYieldCheck`] increments a counter every time it is called (should be called for every __*control flow* instruction__).
/// - For single-threaded targets, it always returns `true` after a certain number of increments.
/// - For multi-threaded targets, it checks an atomic flag every `N` increments. If the flag is set, it returns `true`.
///   The flag should be set by another thread that wants to park the VM.
pub struct EarlyYieldCheck {
    counter: u64,
    /// Only used in non-WASM targets, since WASM currently doesn't support threads.
    /// If another thread wants us to park (e.g. for a GC) they will set this to true.
    #[cfg(not(target_arch = "wasm32"))]
    park_requested: ::std::sync::Arc<::std::sync::atomic::AtomicBool>,
}
impl EarlyYieldCheck {
    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { counter: 0 }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub const fn new(park_requested: ::std::sync::Arc<::std::sync::atomic::AtomicBool>) -> Self {
        Self {
            counter: 0,
            park_requested,
        }
    }
    /// Update the counter and return true if we should yield.
    pub fn should_early_yield(&mut self) -> bool {
        self.counter += 1;
        #[cfg(target_arch = "wasm32")]
        {
            // there are no other threads, so always yield after about ~65K times
            self.counter > (1 << 16)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // other threads can request a park, so check if they've requested every ~4K times
            (self.counter.trailing_zeros() >= 12)
                && self
                    .park_requested
                    .load(::core::sync::atomic::Ordering::Relaxed)
        }
    }
    pub const fn reset(&mut self) {
        self.counter = 0;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use ::std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::EarlyYieldCheck;

    const POLL_WINDOW: u64 = 4_100;

    #[test]
    fn flag_false_never_yields() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut check = EarlyYieldCheck::new(flag);
        for _ in 0..10_000 {
            assert!(!check.should_early_yield());
        }
    }

    #[test]
    fn flag_true_yields_within_poll_window() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut check = EarlyYieldCheck::new(flag);
        let mut yielded_at = None;
        for i in 0..POLL_WINDOW {
            if check.should_early_yield() {
                yielded_at = Some(i);
                break;
            }
        }
        assert!(
            yielded_at.is_some(),
            "should have yielded within {POLL_WINDOW} increments when flag was set"
        );
    }

    #[test]
    fn flag_set_mid_execution_is_observed() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut check = EarlyYieldCheck::new(Arc::clone(&flag));

        for _ in 0..2_000 {
            assert!(!check.should_early_yield());
        }

        flag.store(true, Ordering::Relaxed);

        let mut yielded = false;
        for _ in 0..POLL_WINDOW {
            if check.should_early_yield() {
                yielded = true;
                break;
            }
        }
        assert!(
            yielded,
            "yield should fire within one poll window after the flag flips"
        );
    }

    #[test]
    fn reset_clears_counter() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut check = EarlyYieldCheck::new(Arc::clone(&flag));

        let mut saw_yield = false;
        for _ in 0..POLL_WINDOW {
            if check.should_early_yield() {
                saw_yield = true;
                break;
            }
        }
        assert!(saw_yield, "precondition: counter should have yielded once");

        check.reset();
        flag.store(false, Ordering::Relaxed);

        for _ in 0..2_000 {
            assert!(!check.should_early_yield());
        }
    }

    #[test]
    fn reset_is_public() {
        let _: fn(&mut EarlyYieldCheck) = EarlyYieldCheck::reset;
    }
}
