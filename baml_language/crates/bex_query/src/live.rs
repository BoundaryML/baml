use crate::{BqfFrame, QueryError};

pub const MAX_LIVE_RATE_HZ: u8 = 30;

/// Result of offering a latest-state frame to a live subscription.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveFrameOffer {
    /// The frame may be written to the transport now.
    Send(BqfFrame),
    /// One frame is already awaiting acknowledgement or the rate interval has
    /// not elapsed. The offered frame replaced any older pending snapshot.
    Deferred,
}

/// Transport-independent one-frame-in-flight and rate-cap state.
///
/// This type deliberately stores at most one pending frame: subscriptions are
/// latest-state snapshots, not event queues. A slow or backgrounded client
/// therefore has constant memory use and catches up with one frame after its
/// acknowledgement.
#[derive(Debug)]
pub struct LiveFrameGate {
    max_bytes: usize,
    rate_hz: u8,
    interval_ns: u64,
    next_send_ns: u64,
    in_flight: bool,
    pending: Option<BqfFrame>,
    frames_sent: u64,
    bytes_sent: u64,
}

impl LiveFrameGate {
    pub fn new(max_bytes: usize, rate_hz: u8) -> Result<Self, QueryError> {
        if !(1..=MAX_LIVE_RATE_HZ).contains(&rate_hz) {
            return Err(QueryError::invalid_request(format!(
                "live rate_hz must be in 1..={MAX_LIVE_RATE_HZ}"
            )));
        }
        if !(crate::BQF_HEADER_LEN + crate::BQF_CRC_LEN..=crate::HARD_MAX_BYTES)
            .contains(&max_bytes)
        {
            return Err(QueryError::invalid_request(format!(
                "live max_bytes must be in {}..={}",
                crate::BQF_HEADER_LEN + crate::BQF_CRC_LEN,
                crate::HARD_MAX_BYTES
            )));
        }
        Ok(Self {
            max_bytes,
            rate_hz,
            interval_ns: 1_000_000_000_u64.div_ceil(u64::from(rate_hz)),
            next_send_ns: 0,
            in_flight: false,
            pending: None,
            frames_sent: 0,
            bytes_sent: 0,
        })
    }

    #[must_use]
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    #[must_use]
    pub fn rate_hz(&self) -> u8 {
        self.rate_hz
    }

    #[must_use]
    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    #[must_use]
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent
    }

    #[must_use]
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }

    /// Whether rendering a fresh snapshot can produce an immediately sendable
    /// frame. Hosts use this to avoid doing query work for an unacked client.
    #[must_use]
    pub fn ready_for_snapshot(&self, now_ns: u64) -> bool {
        !self.in_flight && self.pending.is_none() && now_ns >= self.next_send_ns
    }

    pub fn offer(&mut self, now_ns: u64, frame: BqfFrame) -> Result<LiveFrameOffer, QueryError> {
        let frame_bytes = frame.as_bytes().len();
        if frame_bytes > self.max_bytes {
            return Err(QueryError::BudgetExceeded {
                required: frame_bytes,
                max_bytes: self.max_bytes,
            });
        }
        self.pending = Some(frame);
        Ok(self
            .poll(now_ns)
            .map_or(LiveFrameOffer::Deferred, LiveFrameOffer::Send))
    }

    /// Acknowledges the sole outstanding frame. Duplicate acknowledgements
    /// are harmless and cannot release more than one frame.
    pub fn acknowledge(&mut self) {
        self.in_flight = false;
    }

    /// Sends the newest pending snapshot when both the acknowledgement and
    /// rate gates are open.
    pub fn poll(&mut self, now_ns: u64) -> Option<BqfFrame> {
        if self.in_flight || now_ns < self.next_send_ns {
            return None;
        }
        let frame = self.pending.take()?;
        self.in_flight = true;
        self.next_send_ns = now_ns.saturating_add(self.interval_ns);
        self.frames_sent = self.frames_sent.saturating_add(1);
        self.bytes_sent = self
            .bytes_sent
            .saturating_add(u64::try_from(frame.as_bytes().len()).unwrap_or(u64::MAX));
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use crate::{BqfBuilder, FrameKind};

    use super::*;

    fn frame(epoch: u64, max_bytes: usize) -> BqfFrame {
        BqfBuilder::new(FrameKind::Completeness, epoch, epoch, 0)
            .finish(max_bytes)
            .unwrap()
    }

    #[test]
    fn slow_client_has_one_frame_in_flight_and_one_latest_snapshot() {
        let mut gate = LiveFrameGate::new(1024, 30).unwrap();
        assert!(matches!(
            gate.offer(0, frame(1, 1024)).unwrap(),
            LiveFrameOffer::Send(_)
        ));
        for epoch in 2..=10_000 {
            assert_eq!(
                gate.offer(epoch, frame(epoch, 1024)).unwrap(),
                LiveFrameOffer::Deferred
            );
        }
        assert!(gate.in_flight());
        assert!(gate.has_pending());
        assert_eq!(gate.frames_sent(), 1);

        gate.acknowledge();
        let latest = gate
            .poll(1_000_000_000)
            .expect("latest frame becomes sendable");
        assert_eq!(latest.header().unwrap().data_epoch, 10_000);
        assert!(gate.in_flight());
        assert!(!gate.has_pending());
    }

    #[test]
    fn c13_rate_and_byte_bound_is_independent_of_offer_rate() {
        fn simulate(offers_per_second: u64) -> (u64, u64) {
            const MAX_BYTES: usize = 1024;
            const RATE_HZ: u8 = 30;
            let mut gate = LiveFrameGate::new(MAX_BYTES, RATE_HZ).unwrap();
            let interval_ns = 1_000_000_000_u64.div_ceil(u64::from(RATE_HZ));
            let mut epoch = 0_u64;
            // The host samples latest state at the protocol rate. Event rate
            // changes only the observed epoch between samples, never the
            // number or size of wire frames.
            for slot in 0..u64::from(RATE_HZ) * 10 {
                epoch = epoch.saturating_add((offers_per_second / u64::from(RATE_HZ)).max(1));
                let now_ns = slot.saturating_mul(interval_ns);
                assert!(matches!(
                    gate.offer(now_ns, frame(epoch, MAX_BYTES)).unwrap(),
                    LiveFrameOffer::Send(_)
                ));
                gate.acknowledge();
            }
            let max_frames = u64::from(RATE_HZ) * 10;
            assert!(gate.frames_sent() <= max_frames);
            assert!(gate.bytes_sent() <= max_frames * MAX_BYTES as u64);
            (gate.frames_sent(), gate.bytes_sent())
        }

        let ordinary = simulate(60);
        let hot_loop = simulate(60_000);
        assert_eq!(ordinary, hot_loop);
    }
}
