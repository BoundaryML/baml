//! Times full collections over a large live heap of small objects.
//!
//! `cargo run --release -p bex_heap --example gc_live_heap -- 1000000 [rounds]`
//!
//! Each live record mirrors `{a: string, b: string[2], c: int}`: an instance,
//! three strings and an array, all reachable from one root array.
use std::{sync::Arc, time::Instant};

use baml_type::{Name, TypeName};
use bex_heap::{BexHeap, Tlab};
use bex_vm_types::{Class, Object, RealizedTy, Value};

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map_or(1_000_000, |s| s.parse().expect("record count"));
    let rounds: usize = std::env::args()
        .nth(2)
        .map_or(3, |s| s.parse().expect("round count"));
    let class = Object::Class(Box::new(Class {
        name: bex_vm_types::DeclarationName::Declared(TypeName::local(Name::new("Obj"))),
        fields: vec![],
        description: None,
        alias: None,
        docstring: None,
        other: Default::default(),
        stream_done: false,
        type_tag: baml_type::typetag::TypeTag::from_i64(0),
        has_cleanup: false,
        generic_param_count: 0,
        owner: bex_vm_types::HeapPtr::null(),
    }));
    let heap = BexHeap::new(vec![class]);
    let class_ptr = heap.compile_time_ptr(0);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let start = Instant::now();
    let mut records = Vec::with_capacity(n);
    for i in 0..n {
        let a = tlab.alloc_string(format!("a-{i}"));
        let x = tlab.alloc_string(format!("x{i}"));
        let y = tlab.alloc_string(format!("y{i}"));
        let b = tlab.alloc_array(
            RealizedTy::string(),
            vec![Value::object(x), Value::object(y)],
        );
        let inst = tlab.alloc_instance(
            class_ptr,
            vec![Value::object(a), Value::object(b), Value::int(i as i64)],
        );
        records.push(Value::object(inst));
    }
    let mut root = tlab.alloc_array(RealizedTy::int(), records);
    println!("build {n} records: {:?}", start.elapsed());

    for round in 0..rounds {
        let start = Instant::now();
        // SAFETY: single-threaded heap; the only retained pointer is remapped.
        let (stats, roots, fwd) = unsafe { heap.collect_garbage(&[root]) };
        let gc = start.elapsed();
        drop(fwd);
        tlab.invalidate();
        root = roots[0];
        println!(
            "full gc #{round}: {gc:?} live={} ({:.0} ns/object)",
            stats.live_count,
            gc.as_nanos() as f64 / stats.live_count as f64
        );
    }
}
