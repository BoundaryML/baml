// Appended only to diagnostic generated source, never linked by normal packs.
mod diag {
    use super::rt;
    const CLOCK_KIND: usize = if cfg!(feature="floor-ecv") { 2 } else { 0 };
    static mut CLOCK: *const btel_clock::ClockEpoch = std::ptr::null();
    static mut SLOTS: [(u64,u64); FUNCTION_SLOTS] = [(0,0); FUNCTION_SLOTS];
    pub struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let slots=unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(SLOTS).cast::<(u64,u64)>(), FUNCTION_SLOTS) };
            let count:u64=slots.iter().map(|s|s.0).sum();
            let ticks:u64=slots.iter().map(|s|s.1).sum();
            eprintln!("diagnostic samples={count} ticks={ticks}");
        }
    }
    pub fn init() -> Guard {
        assert_eq!(std::env::var("BAML_TELEMETRY").unwrap(), "off");
        if CLOCK_KIND == 2 {
            assert_ne!(unsafe { libc::getauxval(libc::AT_HWCAP2) } & (1<<19), 0, "FEAT_ECV required");
        }
        let runtime=btel_clock::ClockRuntime::new(btel_clock::ClockMode::Auto);
        let clock=runtime.start_run();
        eprintln!("diagnostic clock={:?}, fallback={:?}, variant={}",clock.metadata().source,clock.metadata().fallback,CLOCK_KIND);
        unsafe { CLOCK=std::sync::Arc::into_raw(clock); }
        Guard
    }
    #[inline(always)]
    pub fn read() -> u64 {
        if CLOCK_KIND == 0 {
            let clock=unsafe { &*CLOCK };
            return btel_clock_tick(clock.read());
        }
        let count: u64;
        unsafe {
            if CLOCK_KIND == 1 {
                core::arch::asm!("isb", "mrs {}, cntvct_el0", out(reg) count, options(nostack, preserves_flags));
            } else {
                // CNTVCTSS_EL0, encoding works with baseline ARMv8 assemblers.
                core::arch::asm!("mrs {}, S3_3_C14_C0_6", out(reg) count, options(nostack, preserves_flags));
            }
        }
        count
    }
    #[inline(always)]
    fn btel_clock_tick(t: rt::telemetry::types::ClockInstant) -> u64 {
        rt::telemetry::types::ClockInstant::from_ticks(0).elapsed_until(t).get()
    }
    #[inline(always)]
    pub fn add(function: usize, start: u64, end: u64) {
        unsafe {
            let slot=&mut *std::ptr::addr_of_mut!(SLOTS).cast::<(u64,u64)>().add(function);
            slot.0+=1;
            slot.1+=end.saturating_sub(start);
        }
    }
}
