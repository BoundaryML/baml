use std::{hint::black_box,time::Instant};
#[inline(always)] fn raw() -> u64 { let t; unsafe { core::arch::asm!("mrs {}, cntvct_el0",out(reg)t,options(nomem,nostack,preserves_flags)); } t }
#[inline(always)] fn ordered() -> u64 { let t; unsafe { core::arch::asm!("isb","mrs {}, cntvct_el0",out(reg)t,options(nostack,preserves_flags)); } t }
#[inline(always)] fn ecv() -> u64 { let t; unsafe { core::arch::asm!("mrs {}, S3_3_C14_C0_6",out(reg)t,options(nostack,preserves_flags)); } t }
fn cpu_ns() -> u64 {
    let mut t=libc::timespec {tv_sec:0,tv_nsec:0};
    assert_eq!(unsafe {libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID,&mut t)},0);
    t.tv_sec as u64*1_000_000_000+t.tv_nsec as u64
}
fn measure(name:&str,round:usize,mut read:impl FnMut()->u64) {
    let mut backwards=0;let mut last=read();
    for _ in 0..100_000 {let next=read();backwards+=usize::from(next<last);last=next;}
    let n=2_000_000; let cpu=cpu_ns();let start=Instant::now();let mut sum=0;
    for _ in 0..n {sum^=read();} black_box(sum);
    let wall=start.elapsed().as_nanos() as f64/n as f64;let cpu=(cpu_ns()-cpu) as f64/n as f64;
    println!("{{\"source\":\"{name}\",\"round\":{round},\"ns_per_read\":{},\"cpu_ns_per_read\":{cpu},\"backwards\":{backwards}}}",wall);
}
fn main() {
    assert_ne!(unsafe {libc::getauxval(libc::AT_HWCAP2)} & (1<<19),0);
    let runtime=btel_clock::ClockRuntime::new(btel_clock::ClockMode::Auto);let clock=runtime.start_run();
    eprintln!("quanta source={:?} fallback={:?} calibration={:?}",clock.metadata().source,clock.metadata().fallback,clock.metadata().calibration);
    let monotonic=btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
    for round in 0..5 {
        for step in 0..6 { match (step+round)%6 {
            0=>measure("raw-cntvct-no-isb",round,raw),
            1=>measure("cntvct-isb",round,ordered),
            2=>measure("cntvctss",round,ecv),
            3=>measure("quanta-auto",round,||btel_types::ClockInstant::from_ticks(0).elapsed_until(clock.read()).get()),
            4=>measure("monotonic",round,||btel_types::ClockInstant::from_ticks(0).elapsed_until(monotonic.read()).get()),
            _=>measure("loop-black-box",round,||black_box(0u64)),
        }}
    }
}
