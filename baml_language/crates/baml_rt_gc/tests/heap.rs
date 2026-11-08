use baml_rt_gc::*;
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
fn stack_object_fields_root_heap_children_and_allow_heap_borrows() {
    static DESCRIPTORS: [Descriptor; 2] = [
        DESCS[0],
        Descriptor {
            ptr_slots: (8, 1),
            tagged_slots: (16, 1),
            ..DESCS[0]
        },
    ];
    #[repr(C)]
    struct StackObject {
        header: Header,
        child: usize,
        tagged: TaggedSlot,
    }
    struct StackRoots<'a>(&'a StackObject, RefCell<usize>);
    impl RootSource for StackRoots<'_> {
        fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize)) {
            let low = std::ptr::from_ref(self.0).cast::<usize>();
            visit(low, unsafe {
                low.add(std::mem::size_of::<StackObject>() / 8)
            });
        }
        fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
            visit(&mut *self.1.borrow_mut());
        }
    }
    unsafe {
        let mut heap = Heap::new(&DESCRIPTORS, &TYPES);
        heap.set_stress(false);
        let child = heap.alloc(0, 32, &Roots::default()) as usize;
        let tagged = heap.alloc(0, 32, &Roots::default()) as usize;
        (child as *mut u8).add(24).cast::<u64>().write(71);
        (tagged as *mut u8).add(24).cast::<u64>().write(93);
        let stack = StackObject {
            header: Header { desc: 1, aux: 0 },
            child,
            tagged: TaggedSlot {
                tag: 1,
                payload: tagged as u64,
            },
        };
        let parent = heap.alloc(0, 32, &Roots::default());
        link(parent, 1, std::ptr::from_ref(&stack) as usize);
        let roots = StackRoots(&stack, RefCell::new(parent as usize));
        heap.set_stress(true);
        for _ in 0..16 {
            heap.alloc(0, 32, &roots);
            heap.safepoint(&roots);
            heap.verify(&roots);
            assert_eq!(heap.object_start(stack.child), Some(child));
            assert_eq!((stack.child as *const u8).add(24).cast::<u64>().read(), 71);
            assert_eq!(
                (stack.tagged.payload as *const u8)
                    .add(24)
                    .cast::<u64>()
                    .read(),
                93
            );
            assert_eq!(
                heap.object_start(stack.tagged.payload as usize),
                Some(tagged)
            );
        }
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
fn conservative_false_retention_can_outlive_a_stack_borrow() {
    unsafe {
        let mut heap = heap();
        let parent = heap.alloc(0, 32, &NoRoots);
        {
            // Same 32-byte layout as descriptor 0. No collection happens in
            // this frame, so the collector has never seen its stack range.
            let stack = [0usize; 4];
            link(parent, 1, stack.as_ptr() as usize);
            std::hint::black_box(&stack);
        }
        // This word only looks like a live reference. Its dead container's
        // non-managed slot must not be followed or rejected by verification.
        let false_root = Roots {
            words: vec![parent as usize],
            ..Roots::default()
        };
        heap.collect(&false_root);
        heap.verify(&false_root);
        assert_eq!(heap.stats().live_bytes, 32);
    }
}

#[test]
#[should_panic(expected = "dangling precise GC pointer")]
fn stack_borrow_support_still_rejects_dead_managed_pointers() {
    unsafe {
        let mut heap = heap();
        let dead = heap.alloc(0, 32, &NoRoots) as usize;
        heap.collect(&NoRoots);
        let roots = Roots::default();
        roots.precise.borrow_mut().push(dead);
        heap.verify(&roots);
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
fn builtin_allocation_preserves_tracing_and_ordinary_finalizers() {
    unsafe {
        let mut h = heap();
        let r = Roots::default();
        let parent = h.try_alloc_unfinalized(0, 32, &r).unwrap();
        r.precise.borrow_mut().push(parent as usize);
        let child = h.try_alloc_unfinalized(0, 32, &r).unwrap();
        link(parent, 1, child as usize);
        // Both the slow and cached ordinary paths must still register cleanup.
        let first = h.alloc(1, 32, &r);
        let second = h.alloc(1, 32, &r);
        h.collect(&r);
        assert_eq!(h.object_start(child as usize), Some(child as usize));
        let pending = h.take_pending_finalizers();
        assert_eq!(
            pending.iter().map(|f| f.object()).collect::<Vec<_>>(),
            [first, second]
        );
        for task in pending {
            h.complete_finalizer(task);
        }
        r.precise.borrow_mut().clear();
        h.collect(&r);
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
            kind: Kind::InlineElement,
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

#[test]
fn refusal_collects_preserves_live_graph_and_keeps_emergency_capacity() {
    unsafe {
        let mut h = heap();
        h.reserve_emergency().unwrap();
        h.set_capacity_limit(BLOCK_BYTES);
        let roots = Roots::default();
        for _ in 0..BLOCK_BYTES / 32 {
            let p = h.try_alloc(0, 32, &roots).unwrap();
            roots.precise.borrow_mut().push(p as usize);
        }
        let before = h.stats().collections;
        assert_eq!(h.try_alloc(0, 32, &roots), Err(AllocFailure { bytes: 32 }));
        assert!(h.stats().collections > before);
        for p in roots.precise.borrow().iter() {
            assert_eq!(h.object_start(*p), Some(*p));
        }
        let error = h.alloc_emergency(0, 32).unwrap();
        let message = h.alloc_emergency(2, 64).unwrap();
        link(error, 1, message as usize);
        roots.precise.borrow_mut().clear();
        roots.precise.borrow_mut().push(error as usize);
        h.collect(&roots);
        assert_eq!(h.object_start(message as usize), Some(message as usize));
        h.verify(&roots);
        // A failed allocation has not poisoned the ordinary allocator.
        assert!(h.try_alloc(0, 32, &roots).is_ok());
        assert_eq!(
            h.try_alloc(2, usize::MAX, &roots),
            Err(AllocFailure { bytes: usize::MAX })
        );
    }
}

#[test]
fn large_refusal_retries_collection_without_spending_the_reserve() {
    unsafe {
        let mut h = heap();
        h.reserve_emergency().unwrap();
        h.set_capacity_limit(0);
        let before = h.stats().collections;
        assert_eq!(
            h.try_alloc(2, 16392, &NoRoots),
            Err(AllocFailure { bytes: 16392 })
        );
        assert_eq!(h.stats().collections, before + 1);
        assert!(h.alloc_emergency(0, 32).is_ok());
    }
}

static INLINE_LIST: [Descriptor; 3] = [
    Descriptor {
        kind: Kind::List,
        size: 16,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 16,
            layout: ElementLayout::Descriptor(1),
        }),
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::InlineElement,
        size: 16,
        ptr_slots: (0, 1),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
    Descriptor {
        kind: Kind::Class,
        size: 16,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: None,
    },
];
#[test]
fn headerless_list_elements_trace_without_object_starts() {
    unsafe {
        let mut h = Heap::new(&INLINE_LIST, &TYPES);
        h.set_stress(true);
        let mut roots = Roots::default();
        let list = h.alloc(0, 16 + 64 * 16, &roots);
        roots.precise.borrow_mut().push(list as usize);
        for i in 0..64 {
            let child = h.alloc(2, 16, &roots);
            write_ptr(list, list.add(16 + i * 16).cast(), child as usize);
            child.add(8).cast::<usize>().write(i);
        }
        // Only an interior conservative reference remains, to an element.
        roots.precise.borrow_mut().clear();
        roots.words.push(list.add(16 + 17 * 16) as usize);
        h.collect(&roots);
        for i in 0..64 {
            let entry = list.add(16 + i * 16);
            assert_eq!(h.object_start(entry as usize), Some(list as usize));
            let child = entry.cast::<usize>().read();
            assert_eq!(h.object_start(child), Some(child));
            assert_eq!(((child + 8) as *const usize).read(), i);
        }
        roots.words.clear();
        h.collect(&roots);
        assert_eq!(h.object_start(list as usize), None);
        assert_eq!(h.stats().live_bytes, 0);
    }
}
#[test]
fn inline_elements_cannot_enter_either_allocator() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut h = Heap::new(&INLINE_LIST, &TYPES);
    h.reserve_emergency().unwrap();
    for _ in 0..2 {
        assert!(
            catch_unwind(AssertUnwindSafe(|| unsafe { h.try_alloc(1, 16, &NoRoots) })).is_err()
        );
        assert!(catch_unwind(AssertUnwindSafe(|| unsafe { h.alloc_emergency(1, 16) })).is_err());
    }
    assert_eq!(h.stats().allocated_since_gc, 0);
}
#[test]
#[should_panic(expected = "pointer/tagged range overlaps object header")]
fn allocatable_descriptors_still_require_headers() {
    static D: [Descriptor; 1] = [Descriptor {
        kind: Kind::Class,
        size: 16,
        ptr_slots: (0, 1),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 1,
        fields: &[],
        finalizer: None,
    }];
    let _ = Heap::new(&D, &TYPES);
}
