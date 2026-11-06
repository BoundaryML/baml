//! Deterministic pure-loop microbenchmarks; optional counting runs are untimed evidence.
use bex_v2_loop::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};
thread_local! { static COUNT: Cell<bool> = const { Cell::new(false) }; static ALLOCS: Cell<usize> = const { Cell::new(0) }; static BYTES: Cell<usize> = const { Cell::new(0) }; }
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCS.with(|n| n.set(n.get() + 1));
            BYTES.with(|n| n.set(n.get() + layout.size()));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCS.with(|n| n.set(n.get() + 1));
            BYTES.with(|n| n.set(n.get() + size));
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOC: Allocator = Allocator;
fn run(lp: &mut TaskLoop, mode: &str) {
    if mode == "return" {
        let mut sum = 0;
        for _ in 0..10_000 {
            let f = lp.spawn(|_| Ok(Value::int(1)));
            lp.run_until_idle();
            sum += f.result().unwrap().unwrap().as_int();
        }
        assert_eq!(sum, 10_000);
    } else {
        let f = lp.spawn(|cx| {
            let mut sum = 0;
            for _ in 0..10_000 {
                let f = cx.spawn(|_| Ok(Value::int(1)));
                sum += cx.await_future(&f)?.as_int();
            }
            Ok(Value::int(sum))
        });
        assert_eq!(lp.run_until(&f).unwrap().as_int(), 10_000);
    }
}
fn run_fiber(stack: &mut Option<corosensei::stack::DefaultStack>, mode: &str) {
    use corosensei::{Coroutine, CoroutineResult};
    if mode == "switch" {
        let mut fiber = Coroutine::with_stack(stack.take().unwrap(), |y, ()| {
            for _ in 0..20_000 {
                y.suspend(());
            }
        });
        for _ in 0..20_000 {
            assert!(matches!(fiber.resume(()), CoroutineResult::Yield(())));
        }
        assert!(matches!(fiber.resume(()), CoroutineResult::Return(())));
        *stack = Some(fiber.into_stack());
    } else {
        for _ in 0..10_000 {
            let mut fiber = Coroutine::with_stack(stack.take().unwrap(), |_, ()| 1);
            assert!(matches!(
                fiber.resume(()),
                CoroutineResult::<(), _>::Return(1)
            ));
            *stack = Some(fiber.into_stack());
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map_or("await", String::as_str);
    let jobs: usize = args.get(2).map_or(3, |s| s.parse().unwrap());
    let count = args.get(3).is_some_and(|s| s == "count");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let mut lp = TaskLoop::new(
        Config::default(),
        rt.handle().clone(),
        Rc::new(RefCell::new(())),
    );
    let fiber_mode = matches!(mode, "switch" | "reset");
    let mut stack = if fiber_mode {
        Some(corosensei::stack::DefaultStack::new(64 * 1024).unwrap())
    } else {
        None
    };
    let mut job = |lp: &mut TaskLoop| {
        if fiber_mode {
            run_fiber(&mut stack, mode);
        } else {
            run(lp, mode);
        }
    };
    job(&mut lp);
    #[cfg(feature = "instrument")]
    lp.reset_metrics();
    COUNT.with(|c| c.set(count));
    let start = Instant::now();
    for _ in 0..jobs {
        job(&mut lp);
    }
    let elapsed = start.elapsed();
    COUNT.with(|c| c.set(false));
    println!(
        "{{\"mode\":\"{mode}\",\"jobs\":{jobs},\"elapsed_ns\":{},\"allocations\":{},\"allocated_bytes\":{}}}",
        elapsed.as_nanos(),
        ALLOCS.with(Cell::get),
        BYTES.with(Cell::get)
    );
    #[cfg(feature = "instrument")]
    println!("{:?}", lp.metrics());
}
