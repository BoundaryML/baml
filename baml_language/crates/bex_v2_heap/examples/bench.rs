//! Same checksums and allocation counts as perf/v2-gc. Precise roots isolate
//! allocation/collector cost; parked conservative stacks are tested separately.
use bex_v2_heap::*;
use std::{
    cell::{Cell, UnsafeCell},
    time::Instant,
};
static DESCS: [Descriptor; 2] = [
    Descriptor {
        kind: Kind::Class,
        size: 32,
        ptr_slots: (8, 2),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 0,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::Class,
        size: 48,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 0,
        fields: &[],
        finalizer: None,
    },
];
struct Roots {
    slots: UnsafeCell<[usize; 10300]>,
    len: Cell<usize>,
}
impl Roots {
    fn new() -> Self {
        Self {
            slots: UnsafeCell::new([0; 10300]),
            len: Cell::new(0),
        }
    }
    fn push(&self, p: usize) {
        self.set(self.len.get(), p);
        self.len.set(self.len.get() + 1);
    }
    fn set(&self, i: usize, p: usize) {
        unsafe {
            (*self.slots.get())[i] = p;
        }
    }
    fn get(&self, i: usize) -> usize {
        unsafe { (*self.slots.get())[i] }
    }
}
impl RootSource for Roots {
    fn conservative_ranges(&self, _: &mut dyn FnMut(*const usize, *const usize)) {}
    fn precise_roots(&self, v: &mut dyn FnMut(*mut usize)) {
        for i in 0..self.len.get() {
            v(unsafe { self.slots.get().cast::<usize>().add(i) });
        }
    }
}
#[inline(never)]
unsafe fn tree(h: &mut Heap, r: &Roots, d: usize) -> usize {
    unsafe {
        let n = r.len.get();
        let (left, right) = if d == 0 {
            (0, 0)
        } else {
            let left = tree(h, r, d - 1);
            r.push(left);
            let right = tree(h, r, d - 1);
            r.push(right);
            (left, right)
        };
        let p = h.alloc(0, 32, r);
        write_ptr(p, p.cast::<usize>().add(1), left);
        write_ptr(p, p.cast::<usize>().add(2), right);
        r.len.set(n);
        p as usize
    }
}
unsafe fn check(p: usize) -> usize {
    unsafe {
        let p = p as *const usize;
        if *p.add(1) == 0 {
            1
        } else {
            1 + check(*p.add(1)) + check(*p.add(2))
        }
    }
}
unsafe fn trees(h: &mut Heap, r: &Roots) {
    unsafe {
        let mut total = check(tree(h, r, 15));
        let live = tree(h, r, 14);
        r.push(live);
        for d in (4..=14).step_by(2) {
            for _ in 0..(1 << (18 - d)) {
                total += check(tree(h, r, d));
            }
        }
        total += check(live);
        assert_eq!(total, 3222190);
        r.len.set(0);
    }
}
unsafe fn low(h: &mut Heap, r: &Roots) {
    unsafe {
        r.len.set(10240);
        let mut sum = 0;
        for b in 0..128 {
            for i in 0..2048 {
                let p = h.alloc(1, 48, r).cast::<usize>();
                *p.add(1) = b + i + 1;
                for j in 0..4 {
                    *p.add(2 + j) = i + j;
                }
                *p.add(2) += 1;
                r.set(8192 + i, p as usize);
            }
            for i in 0..2048 {
                let p = r.get(8192 + i) as *const usize;
                sum += *p.add(1) + *p.add(2);
                r.set((b % 4) * 2048 + i, p as usize);
                r.set(8192 + i, 0);
            }
        }
        let live: usize = (0..8192)
            .map(|i| {
                let p = r.get(i) as *const usize;
                *p.add(1) + *p.add(2)
            })
            .sum();
        assert_eq!((sum, live), (553779200, 17813504));
        for i in 0..10240 {
            r.set(i, 0);
        }
        r.len.set(0);
    }
}
fn main() {
    unsafe {
        for name in ["trees", "low"] {
            for trial in 1..=5 {
                let mut h = Heap::new(&DESCS, &[true]);
                h.set_stress(false);
                let r = Roots::new();
                let job = if name == "trees" { trees } else { low };
                job(&mut h, &r); // untimed verification warmup; retain allocator capacity
                h.reset_pause_metrics();
                let before = h.stats();
                let jobs = if name == "trees" { 1 } else { 2 };
                let start = Instant::now();
                for _ in 0..jobs {
                    job(&mut h, &r);
                }
                let elapsed = start.elapsed().as_secs_f64() * 1000.0 / jobs as f64;
                let after = h.stats();
                println!(
                    "{name},trial={trial},ms/job={elapsed:.3},collections={},max_pause_ms={:.3},reserved_bytes={}",
                    after.collections - before.collections,
                    after.max_pause.as_secs_f64() * 1000.0,
                    after.reserved_bytes
                );
            }
        }
    }
}
