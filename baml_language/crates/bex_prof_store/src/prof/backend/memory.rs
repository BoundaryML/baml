//! Process-wide reserve-before-allocate profiler memory governor.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use super::{DerivedSizing, MeasuredLayouts};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReservationClass {
    Control,
    Manual,
    General,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Transport,
    Population,
    ActiveCalls,
    UnresolvedJoins,
    Evidence,
    Values,
    Writer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryDenied {
    pub class: ReservationClass,
    pub owner: Owner,
    pub requested_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug)]
struct Pool {
    limit: u64,
    used: AtomicU64,
}

#[derive(Debug)]
struct GovernorInner {
    control: Pool,
    manual: Pool,
    general: Pool,
    layouts: MeasuredLayouts,
}

#[derive(Clone, Debug)]
pub struct ProfilerMemoryGovernor {
    inner: Arc<GovernorInner>,
}

#[derive(Debug)]
pub struct Reservation {
    inner: Arc<GovernorInner>,
    class: ReservationClass,
    owner: Owner,
    accounted_bytes: u64,
}

impl ProfilerMemoryGovernor {
    #[must_use]
    pub fn new(sizing: DerivedSizing, layouts: MeasuredLayouts) -> Self {
        debug_assert_eq!(
            sizing.total_bytes,
            sizing
                .control_reserve_bytes
                .saturating_add(sizing.manual_reserve_bytes)
                .saturating_add(sizing.general_bytes)
        );
        Self {
            inner: Arc::new(GovernorInner {
                control: Pool {
                    limit: sizing.control_reserve_bytes,
                    used: AtomicU64::new(0),
                },
                manual: Pool {
                    limit: sizing.manual_reserve_bytes,
                    used: AtomicU64::new(0),
                },
                general: Pool {
                    limit: sizing.general_bytes,
                    used: AtomicU64::new(0),
                },
                layouts,
            }),
        }
    }

    pub fn try_reserve(
        &self,
        class: ReservationClass,
        owner: Owner,
        accounted_bytes: u64,
    ) -> Result<Reservation, MemoryDenied> {
        let requested_bytes = accounted_bytes.max(self.minimum_charge(owner));
        reserve(&self.inner, class, owner, requested_bytes)?;
        Ok(Reservation {
            inner: Arc::clone(&self.inner),
            class,
            owner,
            accounted_bytes: requested_bytes,
        })
    }

    /// Explicit-LocalId work tries shared General capacity before the protected
    /// Manual reserve. No other path may call this helper.
    pub fn try_reserve_manual_work(
        &self,
        owner: Owner,
        accounted_bytes: u64,
    ) -> Result<Reservation, [MemoryDenied; 2]> {
        match self.try_reserve(ReservationClass::General, owner, accounted_bytes) {
            Ok(reservation) => Ok(reservation),
            Err(general) => self
                .try_reserve(ReservationClass::Manual, owner, accounted_bytes)
                .map_err(|manual| [general, manual]),
        }
    }

    #[must_use]
    pub fn used_bytes(&self, class: ReservationClass) -> u64 {
        pool(&self.inner, class).used.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn available_bytes(&self, class: ReservationClass) -> u64 {
        let pool = pool(&self.inner, class);
        pool.limit.saturating_sub(pool.used.load(Ordering::Acquire))
    }

    fn minimum_charge(&self, owner: Owner) -> u64 {
        match owner {
            Owner::Transport => self.inner.layouts.transport_segment_bytes,
            Owner::Population => self.inner.layouts.population_item_min_bytes,
            Owner::ActiveCalls => self.inner.layouts.active_call_min_bytes,
            Owner::UnresolvedJoins => self.inner.layouts.unresolved_fact_min_bytes,
            Owner::Evidence => self.inner.layouts.evidence_item_min_bytes,
            Owner::Values => self.inner.layouts.value_root_min_bytes,
            Owner::Writer => self.inner.layouts.writer_batch_min_bytes,
        }
    }
}

impl Reservation {
    #[must_use]
    pub const fn class(&self) -> ReservationClass {
        self.class
    }

    #[must_use]
    pub const fn owner(&self) -> Owner {
        self.owner
    }

    #[must_use]
    pub const fn accounted_bytes(&self) -> u64 {
        self.accounted_bytes
    }

    /// Reserves a capacity delta before the owner grows its allocation.
    pub fn try_grow(&mut self, additional_bytes: u64) -> Result<(), MemoryDenied> {
        if additional_bytes == 0 {
            return Ok(());
        }
        reserve(&self.inner, self.class, self.owner, additional_bytes)?;
        self.accounted_bytes = self.accounted_bytes.saturating_add(additional_bytes);
        Ok(())
    }

    /// Transfers an independently admitted charge into this reservation.
    /// Both reservations must belong to the same governor, class, and owner.
    pub fn absorb(&mut self, mut other: Self) -> Result<(), Self> {
        if !Arc::ptr_eq(&self.inner, &other.inner)
            || self.class != other.class
            || self.owner != other.owner
        {
            return Err(other);
        }
        let Some(accounted_bytes) = self.accounted_bytes.checked_add(other.accounted_bytes) else {
            return Err(other);
        };
        self.accounted_bytes = accounted_bytes;
        other.accounted_bytes = 0;
        Ok(())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let previous = pool(&self.inner, self.class)
            .used
            .fetch_sub(self.accounted_bytes, Ordering::AcqRel);
        debug_assert!(previous >= self.accounted_bytes);
    }
}

fn pool(inner: &GovernorInner, class: ReservationClass) -> &Pool {
    match class {
        ReservationClass::Control => &inner.control,
        ReservationClass::Manual => &inner.manual,
        ReservationClass::General => &inner.general,
    }
}

fn reserve(
    inner: &Arc<GovernorInner>,
    class: ReservationClass,
    owner: Owner,
    requested_bytes: u64,
) -> Result<(), MemoryDenied> {
    let pool = pool(inner, class);
    let mut used = pool.used.load(Ordering::Acquire);
    loop {
        let Some(next) = used.checked_add(requested_bytes) else {
            return Err(denied(pool, class, owner, requested_bytes, used));
        };
        if next > pool.limit {
            return Err(denied(pool, class, owner, requested_bytes, used));
        }
        match pool
            .used
            .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => return Ok(()),
            Err(actual) => used = actual,
        }
    }
}

fn denied(
    pool: &Pool,
    class: ReservationClass,
    owner: Owner,
    requested_bytes: u64,
    used: u64,
) -> MemoryDenied {
    MemoryDenied {
        class,
        owner,
        requested_bytes,
        available_bytes: pool.limit.saturating_sub(used),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::prof::backend::ProfilerSizingPolicy;

    fn governor() -> ProfilerMemoryGovernor {
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1)
    }

    #[test]
    fn tiny_items_pay_the_owner_minimum_and_drop_releases_it() {
        let governor = governor();
        let reservation = governor
            .try_reserve(ReservationClass::General, Owner::ActiveCalls, 1)
            .unwrap();
        assert_eq!(reservation.accounted_bytes(), 320);
        assert_eq!(governor.used_bytes(ReservationClass::General), 320);
        drop(reservation);
        assert_eq!(governor.used_bytes(ReservationClass::General), 0);
    }

    #[test]
    fn reservation_classes_cannot_borrow_each_others_capacity() {
        let governor = governor();
        let general_available = governor.available_bytes(ReservationClass::General);
        let _general = governor
            .try_reserve(ReservationClass::General, Owner::Writer, general_available)
            .unwrap();
        let denied = governor
            .try_reserve(ReservationClass::General, Owner::Evidence, 1)
            .unwrap_err();
        assert_eq!(denied.available_bytes, 0);
        assert!(
            governor
                .try_reserve(ReservationClass::Control, Owner::Population, 1)
                .is_ok()
        );
        assert!(
            governor
                .try_reserve(ReservationClass::Manual, Owner::Evidence, 1)
                .is_ok()
        );
    }

    #[test]
    fn manual_work_falls_back_only_after_general_denial() {
        let governor = governor();
        let available = governor.available_bytes(ReservationClass::General);
        let _general = governor
            .try_reserve(ReservationClass::General, Owner::Writer, available)
            .unwrap();
        let manual = governor
            .try_reserve_manual_work(Owner::Evidence, 1)
            .unwrap();
        assert_eq!(manual.class(), ReservationClass::Manual);
    }

    #[test]
    fn growth_is_reserved_before_capacity_changes() {
        let governor = governor();
        let mut reservation = governor
            .try_reserve(ReservationClass::General, Owner::Values, 1024)
            .unwrap();
        reservation.try_grow(2048).unwrap();
        assert_eq!(reservation.accounted_bytes(), 3072);
        assert_eq!(governor.used_bytes(ReservationClass::General), 3072);
    }

    #[test]
    fn concurrent_reservations_never_cross_the_pool_limit() {
        let governor = Arc::new(governor());
        let limit = governor.available_bytes(ReservationClass::General);
        let mut threads = Vec::new();
        for _ in 0..16 {
            let governor = Arc::clone(&governor);
            threads.push(std::thread::spawn(move || {
                governor.try_reserve(ReservationClass::General, Owner::Values, limit / 4)
            }));
        }
        let reservations: Vec<_> = threads
            .into_iter()
            .filter_map(|thread| thread.join().unwrap().ok())
            .collect();
        assert_eq!(reservations.len(), 4);
        assert_eq!(governor.used_bytes(ReservationClass::General), limit);
    }
}
