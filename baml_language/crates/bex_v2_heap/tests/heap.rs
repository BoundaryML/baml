use bex_v2_heap::*;
use std::cell::RefCell;
static DESCS: [Descriptor; 6] = [
    Descriptor {
        kind: Kind::Class,
        size: 32,
        ptr_slots: (8, 2),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::Class,
        size: 32,
        ptr_slots: (8, 2),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: Some(7),
    },
    Descriptor {
        kind: Kind::Opaque,
        size: 8,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 8,
            layout: ElementLayout::Scalar,
        }),
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::Class,
        size: 32,
        ptr_slots: (0, 0),
        tagged_slots: (8, 1),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::List,
        size: 8,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 8,
            layout: ElementLayout::Pointer,
        }),
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::List,
        size: 16,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 8,
            layout: ElementLayout::Tagged,
        }),
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
];
static TYPES: [bool; 2] = [false, true];
#[derive(Default)]
struct Roots {
    precise: RefCell<Vec<usize>>,
    words: Vec<usize>,
}
impl RootSource for Roots {
    fn conservative_ranges(&self, v: &mut dyn FnMut(*const usize, *const usize)) {
        v(self.words.as_ptr(), unsafe {
            self.words.as_ptr().add(self.words.len())
        });
    }
    fn precise_roots(&self, v: &mut dyn FnMut(*mut usize)) {
        for p in self.precise.borrow_mut().iter_mut() {
            v(p);
        }
    }
}
fn heap() -> Heap {
    let mut h = Heap::new(&DESCS, &TYPES);
    h.set_stress(false);
    h
}
unsafe fn link(p: *mut u8, index: usize, value: usize) {
    unsafe {
        write_ptr(p, p.cast::<usize>().add(index), value);
    }
}
#[test]
fn allocation_zeroing_alias_cycles_and_reuse() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let a = h.alloc(0, 32, &r);
        r.precise.borrow_mut().extend([a as usize, a as usize]);
        assert_eq!(*(a as *const Header), Header { desc: 0, aux: 0 });
        assert!(
            std::slice::from_raw_parts(a.add(8), 24)
                .iter()
                .all(|x| *x == 0)
        );
        let b = h.alloc(0, 32, &r);
        link(a, 1, b as usize);
        link(b, 1, a as usize);
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 64);
        assert_eq!(*a.cast::<usize>().add(1), b as usize);
        r.precise.borrow_mut().clear();
        h.collect(&r);
        assert_eq!(h.object_start(a as usize), None);
        assert_eq!(h.stats().live_bytes, 0);
        let c = h.alloc(0, 32, &r);
        assert_eq!(c, a);
        assert_eq!(*c.cast::<usize>().add(1), 0);
    }
}
#[test]
fn interior_false_positive_bounds_and_large_objects() {
    unsafe {
        let mut h = heap();
        let mut r = Roots::default();
        let p = h.alloc(0, 32, &r);
        r.words.push(p as usize + 31);
        h.collect(&r);
        assert_eq!(h.object_start(p as usize + 31), Some(p as usize));
        // This integer is intentionally indistinguishable from an interior pointer.
        r.words[0] = p as usize + 32;
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 0);
        let p = h.alloc(2, 16392, &r);
        assert_eq!(p as usize % 65536, 0);
        assert!(
            std::slice::from_raw_parts(p.add(8), 16384)
                .iter()
                .all(|x| *x == 0)
        );
        r.words[0] = p as usize + 16000;
        h.collect(&r);
        assert!(h.object_start(p as usize).is_some());
        r.words.clear();
        h.collect(&r);
        assert!(h.object_start(p as usize).is_none());
    }
}
#[test]
fn finalizer_graph_once_resurrection_weak_and_order() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let w = h.new_weak_table();
        let a = h.alloc(1, 32, &r);
        let b = h.alloc(1, 32, &r);
        let c = h.alloc(0, 32, &r);
        link(a, 1, b as usize);
        link(b, 1, a as usize);
        link(b, 2, c as usize);
        h.weak_insert(w, c as usize, 99);
        h.collect(&r);
        let mut tasks = h.take_pending_finalizers();
        assert_eq!(tasks.iter().map(|f| f.object()).collect::<Vec<_>>(), [a, b]);
        assert_eq!(tasks[0].cleanup, 7);
        assert_eq!(h.stats().live_bytes, 96);
        h.collect(&r);
        assert!(h.take_pending_finalizers().is_empty());
        assert_eq!(h.weak_get(w, c as usize), Some(99));
        // A cleanup can collect, then resurrect its receiver; the other cleanup
        // still owns a keepalive token throughout both collections.
        r.precise.borrow_mut().push(a as usize);
        h.complete_finalizer(tasks.remove(0));
        h.collect(&r);
        h.complete_finalizer(tasks.remove(0));
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 96);
        assert!(h.take_pending_finalizers().is_empty());
        r.precise.borrow_mut().clear();
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 0);
        assert_eq!(h.weak_get(w, c as usize), None);
        assert!(h.take_pending_finalizers().is_empty());
    }
}
#[test]
fn explicit_cleanup_suppresses_finalizer() {
    unsafe {
        let mut h = heap();
        let p = h.alloc(1, 32, &NoRoots);
        h.suppress_finalizer(p as usize);
        h.collect(&NoRoots);
        assert!(h.take_pending_finalizers().is_empty());
        assert_eq!(h.stats().live_bytes, 0);
    }
}
#[test]
fn tagged_and_variable_elements_are_precise() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let a = h.alloc(3, 32, &r);
        let child = h.alloc(0, 32, &r);
        a.add(8).cast::<TaggedSlot>().write(TaggedSlot {
            tag: 0,
            payload: child as u64,
        });
        r.precise.borrow_mut().push(a as usize);
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 32);
        let child = h.alloc(0, 32, &r);
        a.add(8).cast::<TaggedSlot>().write(TaggedSlot {
            tag: 1,
            payload: child as u64,
        });
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 64);
        for desc in [4, 5] {
            let size = if desc == 4 { 24 } else { 40 };
            let list = h.alloc(desc, size, &r);
            r.precise.borrow_mut().push(list as usize);
            if desc == 4 {
                link(list, 1, a as usize);
            } else {
                list.add(8).cast::<TaggedSlot>().write(TaggedSlot {
                    tag: 1,
                    payload: a as u64,
                });
            }
            r.precise.borrow_mut().remove(0);
            h.collect(&r);
            assert!(h.object_start(child as usize).is_some());
        }
    }
}
#[test]
fn stress_slow_paths_and_safepoints() {
    unsafe {
        let mut h = heap();
        h.set_stress(true);
        let r = Roots::default();
        let a = h.alloc(0, 32, &r);
        r.precise.borrow_mut().push(a as usize);
        assert_eq!(h.stats().collections, 1);
        for _ in 0..3000 {
            h.alloc(0, 32, &r);
        }
        assert!(h.stats().collections >= 3);
        let before = h.stats().collections;
        h.safepoint(&r);
        assert_eq!(h.stats().collections, before + 1);
        assert!(h.object_start(a as usize).is_some());
    }
}
#[test]
fn budget_headroom_large_payload_and_saturation() {
    unsafe {
        assert_eq!(Heap::budget_for(0), MIN_BUDGET);
        assert_eq!(Heap::budget_for(MIN_BUDGET), 4 * MIN_BUDGET);
        assert_eq!(Heap::budget_for(usize::MAX), usize::MAX);
        let mut h = heap();
        let r = Roots::default();
        let p = h.alloc(2, MIN_BUDGET, &r);
        assert_eq!(h.stats().allocated_since_gc, MIN_BUDGET);
        r.precise.borrow_mut().push(p as usize);
        h.safepoint(&r);
        assert_eq!(h.stats().collections, 1);
        assert_eq!(h.stats().allocated_since_gc, 0);
        assert_eq!(h.stats().budget, 4 * MIN_BUDGET);
        r.precise.borrow_mut().clear();
        h.collect(&r);
        assert_eq!(h.stats().budget, MIN_BUDGET);
    }
}
#[test]
#[should_panic(expected = "dangling precise GC pointer")]
fn debug_verifier_rejects_invalid_pointer() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let p = h.alloc(0, 32, &r);
        link(p, 1, 1234);
        r.precise.borrow_mut().push(p as usize);
        h.collect(&r);
        h.verify(&r);
    }
}
#[test]
fn partial_line_survivors_and_repeated_unswept_collections() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let a = h.alloc(2, 136, &r);
        r.precise.borrow_mut().push(a as usize + 135);
        h.collect(&r);
        h.collect(&r);
        for _ in 0..4000 {
            let p = h.alloc(0, 32, &r);
            assert!((p as usize >= a as usize + 256) || ((p as usize) < a as usize));
        }
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 144);
    }
}

#[test]
fn mixed_graph_matches_reachability_and_dead_starts_do_not_revive() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let mut objects = Vec::new();
        for _ in 0..1500 {
            let p = h.alloc(0, 32, &r);
            objects.push(p);
            r.precise.borrow_mut().push(p as usize);
        }
        let mut edges = vec![[0usize; 2]; objects.len()];
        for (i, pair) in edges.iter_mut().enumerate() {
            // Disconnected cycles with aliases, crossing bitmap words and blocks.
            *pair = [(i / 5) * 5 + (i + 1) % 5, (i / 5) * 5 + (i + 3) % 5];
            for (j, &child) in pair.iter().enumerate() {
                link(objects[i], j + 1, objects[child] as usize);
            }
        }
        r.precise.borrow_mut().clear();
        for i in (0..objects.len()).step_by(35) {
            r.precise.borrow_mut().push(objects[i] as usize);
        }
        let mut live = vec![false; objects.len()];
        let mut queue: Vec<_> = (0..objects.len()).step_by(35).collect();
        while let Some(i) = queue.pop() {
            if !live[i] {
                live[i] = true;
                queue.extend(edges[i]);
            }
        }
        h.collect(&r);
        for (i, &p) in objects.iter().enumerate() {
            assert_eq!(h.object_start(p as usize).is_some(), live[i]);
        }
        // A stale conservative integer must not revive a dead start, including
        // one sharing a retained line. No allocation has swept these blocks yet.
        let stale = Roots {
            words: vec![objects[5] as usize],
            ..Roots::default()
        };
        h.collect(&stale);
        assert_eq!(h.stats().live_bytes, 0);
    }
}

#[test]
fn alignment_padding_is_not_an_interior_pointer() {
    unsafe {
        let mut h = heap();
        let p = h.alloc(4, 24, &NoRoots);
        assert_eq!(h.object_start(p as usize + 23), Some(p as usize));
        assert_eq!(h.object_start(p as usize + 24), None);
        let r = Roots {
            words: vec![p as usize + 24],
            ..Roots::default()
        };
        h.collect(&r);
        assert_eq!(h.stats().live_bytes, 0);
    }
}

#[test]
fn inline_map_entry_descriptors_and_large_finalization() {
    static D: [Descriptor; 3] = [
        Descriptor {
            kind: Kind::Class,
            size: 24,
            ptr_slots: (8, 1),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 1,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::Map,
            size: 24,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 8,
                layout: ElementLayout::Descriptor(0),
            }),
            type_id: 1,
            fields: &[],
            finalizer: Some(5),
        },
        Descriptor {
            kind: Kind::Opaque,
            size: 16,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 1,
            fields: &[],
            finalizer: None,
        },
    ];
    unsafe {
        let mut h = Heap::new(&D, &TYPES);
        h.set_stress(false);
        let map = h.alloc(1, 8 + 24 * 1000, &NoRoots);
        let child = h.alloc(2, 16, &NoRoots);
        link(map, 2, child as usize);
        h.collect(&NoRoots);
        let task = h.take_pending_finalizers().pop().unwrap();
        assert_eq!(task.object(), map);
        h.collect(&NoRoots);
        assert!(h.object_start(child as usize).is_some());
        h.complete_finalizer(task);
        h.collect(&NoRoots);
        assert_eq!(h.object_start(map as usize), None);
        assert_eq!(h.object_start(child as usize), None);
    }
}

#[test]
fn finalizer_order_follows_reused_storage_not_registration_time() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        for _ in 0..4 {
            h.alloc(0, 32, &r);
        }
        let later_slot = h.alloc(1, 32, &r);
        r.precise.borrow_mut().push(later_slot as usize);
        h.collect(&r);
        let reused = h.alloc(1, 32, &r);
        assert!((reused as usize) < later_slot as usize);
        r.precise.borrow_mut().clear();
        h.collect(&r);
        let tasks = h.take_pending_finalizers();
        assert_eq!(
            tasks.iter().map(|f| f.object()).collect::<Vec<_>>(),
            [reused, later_slot]
        );
    }
}
