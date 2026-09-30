#[derive(Clone, Copy, Debug, Default)]
pub struct Monotonic {
    _default: (),
}

impl Monotonic {
    pub fn now(&self) -> u64 {
        #[cfg(all(target_os = "wasi", target_env = "p1"))]
        unsafe {
            wasip1::clock_time_get(wasip1::CLOCKID_MONOTONIC, 1).expect("failed to get time")
        }

        #[cfg(all(target_os = "wasi", target_env = "p2"))]
        wasip2::clocks::monotonic_clock::now()
    }
}
