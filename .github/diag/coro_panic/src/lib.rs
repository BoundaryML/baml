//! corosensei alone, shaped like bex_vm's fiber: a 4 MiB `DefaultStack`, a body
//! that panics on the coroutine, and a parked coroutine dropped mid-body.

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use corosensei::{Coroutine, CoroutineResult, stack::DefaultStack};

    fn coroutine() -> Coroutine<u32, u32, (), DefaultStack> {
        let stack = DefaultStack::new(4 << 20).unwrap();
        Coroutine::with_stack(stack, |yielder, input| {
            if input == 0 {
                panic!("body panic");
            }
            let mut pad = [0u64; 64];
            pad[input as usize % 64] = 1;
            std::hint::black_box(&mut pad);
            yielder.suspend(input);
            panic!("resumed body panic");
        })
    }

    #[test]
    fn a_panic_on_the_coroutine_reaches_the_caller() {
        let mut co = coroutine();
        let caught = catch_unwind(AssertUnwindSafe(|| co.resume(0)));
        assert!(caught.is_err());
        assert!(co.done());
    }

    #[test]
    fn a_panic_after_a_suspend_reaches_the_caller() {
        let mut co = coroutine();
        assert!(matches!(co.resume(1), CoroutineResult::Yield(1)));
        let caught = catch_unwind(AssertUnwindSafe(|| co.resume(2)));
        assert!(caught.is_err());
    }

    #[test]
    fn dropping_a_parked_coroutine_force_unwinds() {
        let mut co = coroutine();
        assert!(matches!(co.resume(1), CoroutineResult::Yield(1)));
        drop(co);
    }
}
