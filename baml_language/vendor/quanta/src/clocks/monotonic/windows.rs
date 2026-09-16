use std::mem;
use windows_sys::Win32::System::Performance;

#[derive(Clone, Copy, Debug)]
pub struct Monotonic {
    frequency: u64,
}

impl Monotonic {
    pub fn raw(&self) -> u64 {
        unsafe {
            // TODO: Can we do any better than the `mem::zeroed` call here?
            let mut count = mem::zeroed();
            if Performance::QueryPerformanceCounter(&mut count) <= 0 {
                unreachable!(
                    "QueryPerformanceCounter on Windows XP or later should never return zero!"
                );
            }
            count as u64
        }
    }

    pub fn scale(&self, raw: u64) -> u64 {
        ((u128::from(raw) * 1_000_000_000) / u128::from(self.frequency)) as u64
    }

    pub fn conversion(&self) -> (u64, u32) {
        // Fixed-point metadata for cold interpretation of raw QPC readings.
        (
            ((1_000_000_000_u128 << 32) / u128::from(self.frequency)) as u64,
            32,
        )
    }

    pub fn now(&self) -> u64 {
        self.scale(self.raw())
    }
}

impl Default for Monotonic {
    fn default() -> Self {
        let denom = unsafe {
            // TODO: Can we do any better than the `mem::zeroed` call here?
            let mut freq = mem::zeroed();
            if Performance::QueryPerformanceFrequency(&mut freq) <= 0 {
                unreachable!(
                    "QueryPerformanceFrequency on Windows XP or later should never return zero!"
                );
            }
            freq as u64
        };

        Self { frequency: denom }
    }
}
