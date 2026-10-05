//! Bytes a value keeps alive that change after it is stored.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Bytes a payload keeps alive that change after it is stored: whoever
/// mutates the payload keeps this current, and a measurement reads it without
/// touching the payload's own synchronization.
#[derive(Debug, Default)]
pub struct RetainedBytes(AtomicUsize);

impl RetainedBytes {
    pub const fn new(bytes: usize) -> Self {
        Self(AtomicUsize::new(bytes))
    }

    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// Publish the current size. Returns the change: positive for growth.
    pub fn set(&self, bytes: usize) -> isize {
        let before = self.0.swap(bytes, Ordering::Relaxed);
        isize::try_from(bytes).unwrap_or(isize::MAX) - isize::try_from(before).unwrap_or(isize::MAX)
    }

    pub fn add(&self, bytes: usize) {
        self.0.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_reports_the_signed_change() {
        let retained = RetainedBytes::new(100);
        assert_eq!(retained.set(150), 50);
        assert_eq!(retained.set(120), -30);
        retained.add(5);
        retained.sub(25);
        assert_eq!(retained.get(), 100);
    }
}
