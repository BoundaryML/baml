//! Saved contexts include register spills. Deliberately no object-pointer root
//! registry: only parked stack ranges are supplied to the collector.
use bex_v2_heap::*;
use corosensei::{
    Coroutine, CoroutineResult,
    stack::{DefaultStack, Stack},
};
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
            let mut co = Coroutine::with_stack(stack, move |y, (): ()| {
                let p = (*heap_ptr).alloc(0, 32, &NoRoots);
                p.add(8).cast::<usize>().write(100 + id);
                // Only this fiber retains the object. Yielded values contain no pointer.
                let identity = std::hint::black_box(p);
                for _ in 0..10 {
                    y.suspend(());
                    assert_eq!(std::hint::black_box(p), identity);
                    assert_eq!(*p.add(8).cast::<usize>(), 100 + id);
                    assert_eq!((*heap_ptr).object_start(p as usize), Some(p as usize));
                }
            });
            assert!(matches!(co.resume(()), CoroutineResult::Yield(())));
            ranges.0.push((co.saved_sp().unwrap(), high));
            fibers.push(co);
        }
        for round in 0..10 {
            heap.collect(&ranges);
            assert_eq!(heap.stats().live_bytes, 8 * 32);
            for (i, co) in fibers.iter_mut().enumerate() {
                let result = co.resume(());
                if round == 9 {
                    assert!(matches!(result, CoroutineResult::Return(())));
                } else {
                    assert!(matches!(result, CoroutineResult::Yield(())));
                    ranges.0[i].0 = co.saved_sp().unwrap();
                }
            }
        }
        assert_eq!(heap.stats().collections, 10);
        heap.collect(&NoRoots);
        assert_eq!(heap.stats().live_bytes, 0);
    }
}
