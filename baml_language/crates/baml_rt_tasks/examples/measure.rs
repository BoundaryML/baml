//! Run in release mode, pinned to the same CPU as the Node reference.
use baml_rt_tasks::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
fn memory() -> (usize, usize) {
    let text = std::fs::read_to_string("/proc/self/status").unwrap();
    let field = |key: &str| {
        text.lines()
            .find(|l| l.starts_with(key))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    (field("VmSize:"), field("VmRSS:"))
}
fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let config = Config::default();
    let mut lp = TaskLoop::new(
        config.clone(),
        rt.handle().clone(),
        Rc::new(RefCell::new(())),
    );
    if std::env::args().nth(1).as_deref() == Some("memory") {
        // Warm runtime initialization and measure before allocating fibers.
        std::thread::sleep(Duration::from_millis(20));
        let before = memory();
        let start = Instant::now();
        for _ in 0..10_000 {
            lp.spawn(|cx| {
                cx.sleep(Duration::from_secs(3600))?;
                Ok(Value::int(0))
            });
        }
        lp.run_until_idle();
        assert_eq!(lp.task_count(), 10_000);
        let after = memory();
        let park_ms = start.elapsed().as_secs_f64() * 1000.0;
        // mincore measures resident pages of exactly the live fiber mappings,
        // including guard pages, even when adjacent VMAs have been merged.
        // SAFETY: sysconf only queries the host page size.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let mut resident_pages = 0usize;
        let mut mapped_bytes = 0usize;
        lp.stack_reservations(&mut |low, bytes| {
            let mut pages = vec![0u8; bytes / page];
            // SAFETY: diagnostic callback pins these page-aligned mappings;
            // mincore writes one status byte per page into the provided vector.
            assert_eq!(
                unsafe { libc::mincore(low.cast_mut().cast(), bytes, pages.as_mut_ptr()) },
                0
            );
            resident_pages += pages.iter().filter(|b| **b & 1 != 0).count();
            mapped_bytes += bytes;
        });
        let stack_rss_kib = resident_pages * page / 1024;
        println!("stack_rss_kib={stack_rss_kib}");
        assert_eq!(
            mapped_bytes,
            (config.stack_size.div_ceil(page) * page + page) * 10_000
        );
        println!(
            "{{\"tasks\":10000,\"stack_bytes\":{},\"mapped_stack_bytes\":{},\"vm_before_kib\":{},\"vm_after_kib\":{},\"rss_before_kib\":{},\"rss_after_kib\":{},\"park_ms\":{}}}",
            config.stack_size, mapped_bytes, before.0, after.0, before.1, after.1, park_ms
        );
        assert_eq!(lp.shutdown(Duration::ZERO).leaked.len(), 10_000);
    } else {
        let run = |lp: &mut TaskLoop| {
            let f = lp.spawn(|cx| {
                let mut sum = 0;
                for _ in 0..10_000 {
                    let f = cx.spawn(|_| Ok(Value::int(1)));
                    sum += cx.await_future(&f)?.as_int();
                }
                Ok(Value::int(sum))
            });
            assert_eq!(lp.run_until(&f).unwrap().as_int(), 10_000);
        };
        run(&mut lp);
        let start = Instant::now();
        for _ in 0..3 {
            run(&mut lp);
        }
        println!(
            "{{\"result\":\"10000\",\"elapsed_ns\":{},\"jobs\":3}}",
            start.elapsed().as_nanos()
        );
        assert!(lp.shutdown(Duration::ZERO).leaked.is_empty());
    }
}
