//! WASM-compatible implementations for AWS SDK requirements.
//!
//! On WASM, `std::time::SystemTime::now()` panics and `block_on` deadlocks.
//! This module provides browser-compatible replacements used by both
//! `build_request` (dry-run SDK config) and `auth_request` (credential resolution).

#[allow(clippy::disallowed_types)]
use std::time::SystemTime;

use aws_smithy_async::{
    rt::sleep::{AsyncSleep, Sleep},
    time::TimeSource,
};

/// Browser-compatible time source using `web_time`.
#[derive(Debug)]
pub(crate) struct BrowserTime;

#[allow(clippy::disallowed_types)]
impl TimeSource for BrowserTime {
    fn now(&self) -> SystemTime {
        let offset = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default();
        std::time::UNIX_EPOCH + offset
    }
}

/// Browser-compatible async sleep using `futures_timer`.
#[derive(Debug, Clone)]
pub(crate) struct BrowserSleep;

impl AsyncSleep for BrowserSleep {
    fn sleep(&self, duration: std::time::Duration) -> Sleep {
        Sleep::new(futures_timer::Delay::new(duration))
    }
}
