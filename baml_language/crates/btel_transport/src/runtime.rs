//! Instance-owned ring setup and a single drainer endpoint.
//!
//! The copied ring uses stable pointers internally. Every public endpoint
//! retains the owning Arc; no raw ring reference escapes this module.
#![allow(unsafe_code)]

use std::{
    collections::HashSet,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use btel_core::{marker::Marker, stage::MarkerRange};

use crate::{
    DrainTarget,
    memory::ProfilerMemoryGovernor,
    registry::{Cursor, Registry},
    ring::{OSThreadMarkerRingHandle, RingCtx},
    sizing::{MeasuredLayouts, ProfilerSizingPolicy},
};

#[derive(Clone, Copy, Debug)]
pub struct TransportConfig {
    pub segment_bytes: usize,
    pub freelist_segments: usize,
    pub memory_bytes: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            segment_bytes: 256 * 1024,
            freelist_segments: 2,
            memory_bytes: 256 * 1024 * 1024,
        }
    }
}

impl TransportConfig {
    /// Preflight for a fixed-source replay that retries full rings. Once all
    /// backlog is drained, rings may still retain a head plus their freelist.
    /// Keep one additional segment available so a full head can roll forward.
    /// This checks configuration once; it does not reserve memory or measure writes.
    pub fn check_retry_capacity(self, source_count: usize) -> Result<(), TransportError> {
        if self.segment_bytes == 0 || self.segment_bytes > 16 * 1024 * 1024 {
            return Err(TransportError::InvalidConfig);
        }
        let measured = MeasuredLayouts {
            transport_segment_bytes: self.segment_bytes as u64,
            ..MeasuredLayouts::V1
        };
        let sizing = ProfilerSizingPolicy::derive(self.memory_bytes, measured)
            .map_err(|_| TransportError::InvalidConfig)?;
        let required = self
            .freelist_segments
            .checked_add(1)
            .and_then(|n| n.checked_mul(source_count))
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(crate::ring::segment_footprint(self.segment_bytes)))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(TransportError::InvalidConfig)?;
        if required > sizing.general_bytes {
            return Err(TransportError::InsufficientRetryCapacity);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransportError {
    InvalidConfig,
    Closed,
    DuplicateSource(u64),
    OutOfMemoryBudget,
    ActiveProducers,
    InsufficientRetryCapacity,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TransportError {}

#[derive(Default)]
struct Admission {
    closed: bool,
    sources: HashSet<u64>,
}

struct Inner {
    // Registry must drop its ring allocations before their budget context.
    registry: Registry,
    admission: Mutex<Admission>,
    active: AtomicUsize,
    config: TransportConfig,
    ctx: Box<RingCtx>,
}

#[derive(Clone)]
pub struct SourceFactory {
    inner: Arc<Inner>,
}

/// Thread-bound writer. Dropping it publishes the source's final write via
/// orphaning before announcing that the producer has finished.
pub struct Producer {
    handle: OSThreadMarkerRingHandle,
    inner: Arc<Inner>,
}

/// The sole round-robin drainer for an instance. It owns a persistent cursor.
/// Scheduling is fixed here; only range handling is supplied by the pipeline.
/// There is no public per-ring drain or cursor reset API. Movable, not cloned.
pub struct Drainer {
    inner: Arc<Inner>,
    cursor: Cursor,
}

pub fn transport(config: TransportConfig) -> Result<(SourceFactory, Drainer), TransportError> {
    if config.segment_bytes == 0 || config.segment_bytes > 16 * 1024 * 1024 {
        return Err(TransportError::InvalidConfig);
    }
    let measured = MeasuredLayouts {
        transport_segment_bytes: config.segment_bytes as u64,
        ..MeasuredLayouts::V1
    };
    let sizing = ProfilerSizingPolicy::derive(config.memory_bytes, measured)
        .map_err(|_| TransportError::InvalidConfig)?;
    let inner = Arc::new(Inner {
        registry: Registry::new(),
        admission: Mutex::default(),
        active: AtomicUsize::new(0),
        config,
        ctx: Box::new(RingCtx::with_governor(ProfilerMemoryGovernor::new(
            sizing, measured,
        ))),
    });
    Ok((
        SourceFactory {
            inner: inner.clone(),
        },
        Drainer {
            inner,
            cursor: Cursor::default(),
        },
    ))
}

impl SourceFactory {
    /// Call on the OS thread that will write. Registration is outside replay's
    /// timed path; only this setup operation locks the admission table.
    pub fn producer(&self, source_id: u64) -> Result<Producer, TransportError> {
        let mut admission = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if admission.closed {
            return Err(TransportError::Closed);
        }
        if admission.sources.contains(&source_id) {
            return Err(TransportError::DuplicateSource(source_id));
        }
        // SAFETY: Box gives a stable address. Producer/Drainer keep Inner
        // alive; registry drops all rings before ctx, and no static ref escapes.
        let ctx: &'static RingCtx = unsafe { &*std::ptr::from_ref(&*self.inner.ctx) };
        let handle = self
            .inner
            .registry
            .acquire(
                ctx,
                self.inner.config.segment_bytes,
                self.inner.config.freelist_segments,
                source_id,
            )
            .ok_or(TransportError::OutOfMemoryBudget)?;
        admission.sources.insert(source_id);
        self.inner.active.fetch_add(1, Ordering::Relaxed);
        Ok(Producer {
            handle,
            inner: self.inner.clone(),
        })
    }

    pub fn wake_consumer(&self) {
        self.inner.ctx.wake().force_wake();
    }
}

impl Producer {
    /// Encode directly into the reserved ring slot using the copied marker codec.
    /// The caller supplies the timestamp, just as the VM producer does.
    #[inline]
    pub fn write_marker(&mut self, marker: &Marker<'_>) -> bool {
        // SAFETY: exclusive thread-bound producer; encode_to initializes the
        // entire encoded_len slot before the copied ring publishes its commit.
        unsafe {
            self.handle.ring().push_with(marker.encoded_len(), |slot| {
                marker.encode_to(slot);
            })
        }
    }

    /// Write one whole encoded record. False means budget rejection, not retry.
    #[inline]
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        // SAFETY: this private handle cannot outlive its owner or leave the
        // claiming thread, and only Drop orphans it after the last write.
        unsafe { self.handle.push(bytes) }
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        self.handle.ring().orphan();
        self.inner.active.fetch_sub(1, Ordering::Release);
        self.inner.ctx.wake().force_wake();
    }
}

impl Drainer {
    pub fn register_worker(&self) {
        self.inner.ctx.wake().register_consumer();
    }

    /// A bounded round-robin service step shared by all drainer presets.
    pub fn drain<T: DrainTarget>(&mut self, pipeline: &mut T, source_budget: usize) -> bool {
        let mut progress = false;
        for _ in 0..source_budget.max(1) {
            // SAFETY: this endpoint is the instance's sole drainer. Every
            // pipeline call accepts the range before the ring advances.
            let visit = unsafe {
                self.inner
                    .registry
                    .visit_next(&mut self.cursor, &mut |ring, bytes| {
                        pipeline.accept(MarkerRange {
                            source_id: ring.engine_id(),
                            bytes,
                        });
                    })
            };
            let Some(visit) = visit else { break };
            progress |= visit.progress;
            if visit.wrapped {
                break;
            }
        }
        pipeline.end_step();
        progress
    }

    pub fn idle<T: DrainTarget>(
        &mut self,
        pipeline: &mut T,
        source_budget: usize,
        timeout: Duration,
    ) {
        self.inner.ctx.wake().pre_park();
        if !self.drain(pipeline, source_budget) {
            self.inner.ctx.wake().park(timeout);
        }
        self.inner.ctx.wake().post_park();
    }

    /// Seal admission and finish only after producer ownership has ended.
    /// No decoder/root-completion state participates in this boundary.
    pub fn finish<T: DrainTarget>(self, mut pipeline: T) -> Result<T::Output, TransportError> {
        self.inner
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .closed = true;
        if self.inner.active.load(Ordering::Acquire) != 0 {
            return Err(TransportError::ActiveProducers);
        }
        // After all producer drops, orphan Acquire observes their final
        // commits. Repeat complete sweeps until every source is drained.
        while !unsafe {
            self.inner.registry.sweep_outcome(&mut |ring, bytes| {
                pipeline.accept(MarkerRange {
                    source_id: ring.engine_id(),
                    bytes,
                });
            })
        }
        .caught_up
        {
            pipeline.end_step();
        }
        Ok(pipeline.finish())
    }
}
