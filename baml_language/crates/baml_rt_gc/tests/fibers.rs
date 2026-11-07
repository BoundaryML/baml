//! Saved contexts include register spills. Deliberately no object-pointer root
//! registry: only parked stack ranges are supplied to the collector.
use baml_rt_gc::*;
use corosensei::{
    Coroutine, CoroutineResult,
    stack::{DefaultStack, Stack},
};
use std::{cell::Cell, rc::Rc};
static DESCS: [Descriptor; 1] = [Descriptor {
    kind: Kind::Class,
    size: 32,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 0,
    fields: &[],
    finalizer: None,
}];
struct Ranges(Vec<(usize, usize)>);
impl RootSource for Ranges {
    fn conservative_ranges(&self, v: &mut dyn FnMut(*const usize, *const usize)) {
        for &(lo, hi) in &self.0 {
            v(lo as *const usize, hi as *const usize);
        }
    }
    fn precise_roots(&self, _: &mut dyn FnMut(*mut usize)) {}
}
#[test]
fn eight_parked_fibers_survive_ten_collections() {
    unsafe {
        let mut heap = Heap::new(&DESCS, &[true]);
        heap.set_stress(false);
        let heap_ptr = &mut heap as *mut Heap;
        let mut fibers = Vec::new();
        let mut ranges = Ranges(vec![]);
        for id in 0..8 {
            let stack = DefaultStack::new(128 * 1024).unwrap();
            let high = stack.base().get();
            let saved_sp = Rc::new(Cell::new(0));
            let captured_sp = saved_sp.clone();
            let mut co = Coroutine::with_stack(stack, move |y, (): ()| {
                let p = (*heap_ptr).alloc(0, 32, &NoRoots);
                p.add(8).cast::<usize>().write(100 + id);
                // Only this fiber retains the object. Yielded values contain no pointer.
                let identity = std::hint::black_box(p);
                for _ in 0..10 {
                    with_registers(|sp, _| {
                        captured_sp.set(sp);
                        y.suspend(());
                    });
                    assert_eq!(std::hint::black_box(p), identity);
                    assert_eq!(*p.add(8).cast::<usize>(), 100 + id);
                    assert_eq!((*heap_ptr).object_start(p as usize), Some(p as usize));
                }
            });
            assert!(matches!(co.resume(()), CoroutineResult::Yield(())));
            ranges.0.push((saved_sp.get(), high));
            fibers.push((co, saved_sp));
        }
        for round in 0..10 {
            heap.collect(&ranges);
            assert_eq!(heap.stats().live_bytes, 8 * 32);
            for (i, (co, saved_sp)) in fibers.iter_mut().enumerate() {
                let result = co.resume(());
                if round == 9 {
                    assert!(matches!(result, CoroutineResult::Return(())));
                } else {
                    assert!(matches!(result, CoroutineResult::Yield(())));
                    ranges.0[i].0 = saved_sp.get();
                }
            }
        }
        assert_eq!(heap.stats().collections, 10);
        heap.collect(&NoRoots);
        assert_eq!(heap.stats().live_bytes, 0);
    }
}

// ABI spill frame remains live throughout suspension.
#[inline(never)]
fn with_registers(f: impl FnOnce(usize, &[usize])) {
    let mut regs = [0usize; 20];
    let sp: usize;
    // SAFETY: stores only within regs; observes the ABI's preserved registers.
    // No nostack/nomem options: compiler must keep the buffer addressable.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        std::arch::asm!(
            "stp x19, x20, [{buf}, #0]", "stp x21, x22, [{buf}, #16]",
            "stp x23, x24, [{buf}, #32]", "stp x25, x26, [{buf}, #48]",
            "stp x27, x28, [{buf}, #64]", "stp x29, x30, [{buf}, #80]",
            "stp d8, d9, [{buf}, #96]", "stp d10, d11, [{buf}, #112]",
            "stp d12, d13, [{buf}, #128]", "stp d14, d15, [{buf}, #144]",
            "mov {sp}, sp", buf = in(reg) regs.as_mut_ptr(), sp = out(reg) sp,
        );
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::asm!(
            "mov [{buf}], rbx", "mov [{buf} + 8], rbp", "mov [{buf} + 16], r12",
            "mov [{buf} + 24], r13", "mov [{buf} + 32], r14", "mov [{buf} + 40], r15",
            "mov {sp}, rsp", buf = in(reg) regs.as_mut_ptr(), sp = out(reg) sp,
        );
    }
    // The parked range must include this still-live spill buffer.
    debug_assert!(sp <= regs.as_ptr() as usize);
    f(sp, &regs);
    std::hint::black_box(regs);
}
