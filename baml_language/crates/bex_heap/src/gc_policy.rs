//! Allocation spending requests a full collection at the next VM safe point.
//!
//! This is headroom between collections, not a hard heap or RSS limit. Spending
//! is the heap's net growth since the last full collection: reserved object
//! slots (including unused TLAB capacity) plus the memory objects keep alive
//! outside their slots, as reported by [`Object::measure`](bex_vm_types::Object::measure).
//! Each allocator keeps a local balance and settles it here in quanta, so a
//! running VM can hold a bounded amount of unsettled spending. Storage shared
//! between objects is charged by every object that references it, so spending
//! overestimates sharing; the census taken by a full collection does not.
//!
//! How much growth a collection allows is set by [`GcSettings`]: the next
//! full collection is due once spending reaches `growth` times the bytes the
//! last one found live, but never less than `min_growth`. Both are read only
//! when the budget is set (at construction and after each full collection);
//! settling spending never consults them.
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
    },
};

use crate::BexHeap;

const MIB: u64 = 1024 * 1024;
/// Number of existing VM control-flow checks between pressure/park polls.
pub const POLL_INTERVAL: u64 = 4096;

/// How much a heap may grow between full collections.
///
/// After each full collection the next one is due once the heap has grown by
/// `growth` times the data that survived, and by at least `min_growth` bytes;
/// `min_growth` also sets when the first collection happens. Memory in use is
/// then roughly the live data plus the allowed growth, and a collection needs
/// extra room while it copies. Lower values use less memory and more collector
/// time: collector work per allocated byte is about `1 / growth`.
///
/// Every value of this type is valid: [`Growth`] and [`GcSettings::new`]
/// enforce the ranges, so no setting can make the heap collect forever.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GcSettings {
    growth: Growth,
    /// Bytes; at least [`GcSettings::MIN_GROWTH_FLOOR`].
    min_growth: usize,
}

impl GcSettings {
    /// Today's `min_growth`: 32 MiB.
    pub const DEFAULT_MIN_GROWTH: usize = 32 * 1024 * 1024;
    /// The smallest `min_growth`: 1 MiB. Allocators settle spending in
    /// quanta of up to 64 KiB of payload (and object-slot reservations of a
    /// similar size), so a smaller allowance would make almost every
    /// settlement request a collection.
    pub const MIN_GROWTH_FLOOR: usize = 1024 * 1024;
    /// The largest `min_growth` the budget counter can represent.
    pub const MIN_GROWTH_CEILING: usize = isize::MAX.unsigned_abs();

    pub fn new(growth: Growth, min_growth: usize) -> Result<Self, GcSettingError> {
        if min_growth < Self::MIN_GROWTH_FLOOR {
            return Err(GcSettingError::MinGrowthOutOfRange { bytes: min_growth });
        }
        if min_growth > Self::MIN_GROWTH_CEILING {
            return Err(GcSettingError::SizeTooLarge {
                text: format!("{min_growth} bytes"),
            });
        }
        Ok(Self { growth, min_growth })
    }

    pub fn growth(self) -> Growth {
        self.growth
    }

    /// Bytes.
    pub fn min_growth(self) -> usize {
        self.min_growth
    }

    /// The allowance after a full collection that found `live_bytes` live.
    fn budget_after(self, live_bytes: usize) -> usize {
        let scaled = live_bytes as u128 * u128::from(self.growth.hundredths) / 100;
        usize::try_from(scaled)
            .unwrap_or(usize::MAX)
            .max(self.min_growth)
    }
}

impl Default for GcSettings {
    fn default() -> Self {
        Self {
            growth: Growth::DEFAULT,
            min_growth: Self::DEFAULT_MIN_GROWTH,
        }
    }
}

/// Allowed growth between full collections as a multiple of the data that
/// survived the last one, in steps of 0.01, from 0.25 to 1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Growth {
    hundredths: u32,
}

impl Growth {
    /// Today's growth: 4.
    pub const DEFAULT: Self = Self { hundredths: 400 };
    /// 0.25: collections cost about four times the allocation they free.
    pub const MIN: Self = Self { hundredths: 25 };
    /// 1000: a guard against typos, not a technical limit.
    pub const MAX: Self = Self {
        hundredths: 100_000,
    };

    pub fn from_hundredths(hundredths: u32) -> Result<Self, GcSettingError> {
        let growth = Self { hundredths };
        if (Self::MIN..=Self::MAX).contains(&growth) {
            Ok(growth)
        } else {
            Err(GcSettingError::GrowthOutOfRange { growth })
        }
    }

    pub fn hundredths(self) -> u32 {
        self.hundredths
    }

    /// A decimal number with at most two decimal places: `4`, `2.5`, `0.25`.
    pub fn parse(text: &str) -> Result<Self, GcSettingError> {
        let invalid = || GcSettingError::InvalidGrowth {
            text: text.to_owned(),
        };
        let text = text.trim();
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if whole.is_empty() || !digits(whole) || !digits(fraction) || text.ends_with('.') {
            return Err(invalid());
        }
        if fraction.len() > 2 {
            return Err(GcSettingError::GrowthTooPrecise {
                text: text.to_owned(),
            });
        }
        let whole: u32 = whole.parse().map_err(|_| GcSettingError::GrowthTooLarge {
            text: text.to_owned(),
        })?;
        let fraction: u32 = format!("{fraction:0<2}").parse().map_err(|_| invalid())?;
        let hundredths = whole
            .checked_mul(100)
            .and_then(|whole| whole.checked_add(fraction))
            .ok_or_else(|| GcSettingError::GrowthTooLarge {
                text: text.to_owned(),
            })?;
        Self::from_hundredths(hundredths)
    }

    /// A number from a typed source (a TOML float). It must be a multiple of
    /// 0.01, within the precision of a double.
    pub fn from_f64(value: f64) -> Result<Self, GcSettingError> {
        let text = value.to_string();
        if !value.is_finite() || value < 0.0 {
            return Err(GcSettingError::InvalidGrowth { text });
        }
        let scaled = value * 100.0;
        let rounded = scaled.round();
        if (scaled - rounded).abs() > 1e-6 {
            return Err(GcSettingError::GrowthTooPrecise { text });
        }
        if rounded > f64::from(Self::MAX.hundredths) {
            return Err(GcSettingError::GrowthTooLarge { text });
        }
        // In range and integral: the cast is exact.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked non-negative, integral and at most Growth::MAX above"
        )]
        let hundredths = rounded as u32;
        Self::from_hundredths(hundredths)
    }
}

impl fmt::Display for Growth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (whole, fraction) = (self.hundredths / 100, self.hundredths % 100);
        match fraction {
            0 => write!(f, "{whole}"),
            _ if fraction % 10 == 0 => write!(f, "{whole}.{}", fraction / 10),
            _ => write!(f, "{whole}.{fraction:02}"),
        }
    }
}

/// Parse a `min_growth` size: a whole number of bytes, optionally followed by
/// a binary unit `B`, `KiB`, `MiB` or `GiB` (case-insensitive, optional space
/// before it): `"32MiB"`, `"512 KiB"`, `"33554432"`. Decimal units (`MB`) are
/// rejected rather than guessed.
pub fn parse_min_growth(text: &str) -> Result<usize, GcSettingError> {
    let trimmed = text.trim();
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let unit = unit.trim_start();
    let invalid = || GcSettingError::InvalidSize {
        text: text.to_owned(),
    };
    if number.is_empty() {
        return Err(invalid());
    }
    let multiplier: u64 = match unit.to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "kib" => 1024,
        "mib" => MIB,
        "gib" => 1024 * MIB,
        "k" | "kb" => return Err(decimal_unit(text, "KiB")),
        "m" | "mb" => return Err(decimal_unit(text, "MiB")),
        "g" | "gb" => return Err(decimal_unit(text, "GiB")),
        _ => return Err(invalid()),
    };
    let too_large = || GcSettingError::SizeTooLarge {
        text: text.to_owned(),
    };
    let bytes = number
        .parse::<u64>()
        .map_err(|_| too_large())?
        .checked_mul(multiplier)
        .ok_or_else(too_large)?;
    let bytes = usize::try_from(bytes).map_err(|_| too_large())?;
    if bytes < GcSettings::MIN_GROWTH_FLOOR && unit.is_empty() {
        return Err(GcSettingError::MinGrowthBareNumber { bytes });
    }
    Ok(bytes)
}

fn decimal_unit(text: &str, suggestion: &'static str) -> GcSettingError {
    GcSettingError::DecimalUnit {
        text: text.to_owned(),
        suggestion,
    }
}

/// A GC setting that cannot work. Messages name the value, not where it came
/// from; callers prefix the variable or `baml.toml` key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GcSettingError {
    #[error(
        "`{text}` is not a growth factor; use a number from 0.25 to 1000 with at most two decimals, for example 2.5"
    )]
    InvalidGrowth { text: String },
    #[error("`{text}` has more than two decimals; growth goes in steps of 0.01, for example 2.5")]
    GrowthTooPrecise { text: String },
    #[error("growth `{text}` is above the maximum of 1000")]
    GrowthTooLarge { text: String },
    #[error(
        "growth {growth} is out of range; use 0.25 to 1000 (below 1 is allowed but costly: collector work per allocated byte is about 1/growth)"
    )]
    GrowthOutOfRange { growth: Growth },
    #[error(
        "`{text}` is not a size; use a whole number with B, KiB, MiB or GiB, for example \"32MiB\""
    )]
    InvalidSize { text: String },
    #[error("`{text}` uses a decimal unit; use binary units, for example `{suggestion}`")]
    DecimalUnit {
        text: String,
        suggestion: &'static str,
    },
    #[error(
        "{bytes} is a number of bytes, below the minimum of 1 MiB; add a unit, for example \"32MiB\""
    )]
    MinGrowthBareNumber { bytes: usize },
    #[error("min_growth must be at least 1 MiB (got {bytes} bytes)")]
    MinGrowthOutOfRange { bytes: usize },
    #[error("size `{text}` is too large")]
    SizeTooLarge { text: String },
}

/// Diagnostic snapshot. Concurrent allocation may advance spending while read.
#[derive(Debug, Clone, Copy)]
pub struct GcBudgetSnapshot {
    /// Net bytes the heap grew since the last full GC: slots reserved plus
    /// payload allocated, minus payload released. Excludes each allocator's
    /// unsettled balance. Negative when more was released than allocated.
    pub bytes_since_full_gc: isize,
    pub full_budget_bytes: usize,
    pub full_collections: usize,
}

pub(crate) struct AllocationBudget {
    spent: AtomicIsize,
    budget: AtomicUsize,
    full_collections: AtomicUsize,
    pressure: Arc<AtomicBool>,
    /// Fixed for the heap's lifetime; read only where the budget is set.
    settings: GcSettings,
}

impl AllocationBudget {
    pub(crate) fn new(settings: GcSettings) -> Self {
        Self {
            spent: AtomicIsize::new(0),
            budget: AtomicUsize::new(settings.min_growth),
            full_collections: AtomicUsize::new(0),
            pressure: Arc::new(AtomicBool::new(false)),
            settings,
        }
    }

    /// Settle `delta` bytes of net growth. Returns whether this settlement
    /// took spending from below the budget to at or above it, so the caller
    /// that caused the crossing can reach a safe point promptly.
    pub(crate) fn adjust(&self, delta: isize) -> bool {
        if delta == 0 {
            return false;
        }
        let old = self
            .spent
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(delta))
            })
            .unwrap_or_else(|_| unreachable!("the update never declines"));
        let new = old.saturating_add(delta);
        let budget = Self::budget_as_isize(self.budget.load(Ordering::Relaxed));
        // No collection here: allocation and mutation can hold unpublished
        // pointers or container locks. The VM cooperates at a safe point.
        // A release can also take spending back below the budget; the flag
        // follows it so no VM yields for a collection that is no longer due.
        self.pressure.store(new >= budget, Ordering::Relaxed);
        old < budget && new >= budget
    }

    pub(crate) fn due(&self) -> bool {
        self.spent.load(Ordering::Relaxed)
            >= Self::budget_as_isize(self.budget.load(Ordering::Relaxed))
    }

    fn budget_as_isize(budget: usize) -> isize {
        isize::try_from(budget).unwrap_or(isize::MAX)
    }

    /// Called with all mutators parked, after full collection has moved
    /// survivors. `live_bytes` is what the survivors occupy: their slots and
    /// what they keep alive outside them. Collector moves are not allocation
    /// spending.
    pub(crate) fn after_full(&self, live_bytes: usize) {
        self.spent.store(0, Ordering::Relaxed);
        self.budget
            .store(self.settings.budget_after(live_bytes), Ordering::Relaxed);
        self.pressure.store(false, Ordering::Relaxed);
        self.full_collections.fetch_add(1, Ordering::Relaxed);
    }
}

impl BexHeap {
    /// The collection policy this heap was built with.
    pub fn gc_settings(&self) -> GcSettings {
        self.gc_policy.settings
    }

    pub fn gc_budget(&self) -> GcBudgetSnapshot {
        GcBudgetSnapshot {
            bytes_since_full_gc: self.gc_policy.spent.load(Ordering::Relaxed),
            full_budget_bytes: self.gc_policy.budget.load(Ordering::Relaxed),
            full_collections: self.gc_policy.full_collections.load(Ordering::Relaxed),
        }
    }

    pub fn gc_pressure(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.gc_policy.pressure)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{OBJECT_CELL_WORDS, Object};

    use super::*;
    use crate::Tlab;

    const MIN_FULL_BUDGET: usize = GcSettings::DEFAULT_MIN_GROWTH;
    const BUDGET: isize = MIN_FULL_BUDGET as isize;
    const MIB: usize = 1024 * 1024;

    fn settings(growth: &str, min_growth: usize) -> GcSettings {
        GcSettings::new(Growth::parse(growth).unwrap(), min_growth).unwrap()
    }

    #[test]
    fn defaults_are_the_numbers_chosen_by_the_policy_study() {
        let defaults = GcSettings::default();
        assert_eq!(defaults.growth(), Growth::DEFAULT);
        assert_eq!(defaults.growth().hundredths(), 400);
        assert_eq!(defaults.min_growth(), 32 * MIB);
    }

    /// Growth scales the allowance with the survivors, `min_growth` floors it
    /// and sets the first one; fractions of growth are exact.
    #[test]
    fn non_default_settings_shape_the_budget() {
        let p = AllocationBudget::new(settings("0.25", 2 * MIB));
        assert_eq!(p.budget.load(Ordering::Relaxed), 2 * MIB, "first budget");
        assert!(p.adjust((2 * MIB) as isize), "crosses at min_growth");
        p.after_full(64 * MIB);
        assert_eq!(p.budget.load(Ordering::Relaxed), 16 * MIB);
        p.after_full(4 * MIB);
        assert_eq!(p.budget.load(Ordering::Relaxed), 2 * MIB, "floored");

        let p = AllocationBudget::new(settings("2.5", MIB));
        p.after_full(10 * MIB);
        assert_eq!(p.budget.load(Ordering::Relaxed), 25 * MIB);
        p.after_full(3);
        assert_eq!(p.budget.load(Ordering::Relaxed), MIB);

        let p = AllocationBudget::new(settings("1000", 512 * MIB));
        p.after_full(MIB);
        assert_eq!(p.budget.load(Ordering::Relaxed), 1000 * MIB);
        p.after_full(usize::MAX);
        assert_eq!(p.budget.load(Ordering::Relaxed), usize::MAX, "saturates");
    }

    #[test]
    fn growth_parsing_and_range() {
        for (text, hundredths) in [
            ("4", 400),
            ("2.5", 250),
            ("2.50", 250),
            ("0.25", 25),
            (" 1.05 ", 105),
            ("1000", 100_000),
        ] {
            assert_eq!(
                Growth::parse(text).unwrap().hundredths(),
                hundredths,
                "{text}"
            );
        }
        for text in [
            "",
            "x",
            "4x",
            "-1",
            "+1",
            ".5",
            "4.",
            "1e3",
            "1.2.3",
            "0",
            "0.24",
            "1000.01",
            "2.555",
            "99999999999",
        ] {
            assert!(Growth::parse(text).is_err(), "{text}");
        }
        assert_eq!(Growth::from_f64(2.5).unwrap().hundredths(), 250);
        assert_eq!(Growth::from_f64(0.29).unwrap().hundredths(), 29);
        for value in [f64::NAN, f64::INFINITY, -1.0, 0.1, 1000.5, 1.234] {
            assert!(Growth::from_f64(value).is_err(), "{value}");
        }
        for text in ["4", "2.5", "0.25", "1.05"] {
            assert_eq!(Growth::parse(text).unwrap().to_string(), text);
        }
    }

    #[test]
    fn min_growth_parsing_and_range() {
        for (text, bytes) in [
            ("32MiB", 32 * MIB),
            ("32 MiB", 32 * MIB),
            ("32mib", 32 * MIB),
            ("1GiB", 1024 * MIB),
            ("2048KiB", 2 * MIB),
            ("1048576", MIB),
            ("1048576B", MIB),
        ] {
            assert_eq!(parse_min_growth(text).unwrap(), bytes, "{text}");
        }
        assert!(matches!(
            parse_min_growth("32MB"),
            Err(GcSettingError::DecimalUnit {
                suggestion: "MiB",
                ..
            })
        ));
        assert!(matches!(
            parse_min_growth("32"),
            Err(GcSettingError::MinGrowthBareNumber { bytes: 32 })
        ));
        for text in ["", "MiB", "-1MiB", "1.5MiB", "32 megs", "99999999999999GiB"] {
            assert!(parse_min_growth(text).is_err(), "{text}");
        }
        assert!(
            GcSettings::new(Growth::DEFAULT, 0).is_err(),
            "never collect-forever"
        );
        assert!(GcSettings::new(Growth::DEFAULT, MIB - 1).is_err());
        assert!(GcSettings::new(Growth::DEFAULT, MIB).is_ok());
        assert!(GcSettings::new(Growth::DEFAULT, usize::MAX).is_err());
        assert!(Growth::from_hundredths(24).is_err());
        assert!(Growth::from_hundredths(100_001).is_err());
    }

    #[test]
    fn full_budget_tracks_spending_and_live_headroom() {
        let p = AllocationBudget::new(GcSettings::default());
        assert!(!p.adjust(0));
        assert!(!p.due());
        assert!(!p.adjust(BUDGET - 1));
        assert!(!p.due());
        assert!(p.adjust(1), "this settlement crosses the budget");
        assert!(p.due());
        assert!(p.pressure.load(Ordering::Relaxed));
        assert!(!p.adjust(1), "spending was already over the budget");
        p.after_full(MIN_FULL_BUDGET);
        assert!(!p.due());
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert_eq!(p.budget.load(Ordering::Relaxed), 4 * MIN_FULL_BUDGET);
        p.after_full(0);
        assert_eq!(p.budget.load(Ordering::Relaxed), MIN_FULL_BUDGET);
        p.adjust(isize::MAX);
        p.adjust(1);
        assert_eq!(p.spent.load(Ordering::Relaxed), isize::MAX);
        p.after_full(usize::MAX);
        assert_eq!(p.budget.load(Ordering::Relaxed), usize::MAX);
    }

    #[test]
    fn a_release_takes_spending_and_pressure_back_below_the_budget() {
        let p = AllocationBudget::new(GcSettings::default());
        assert!(p.adjust(BUDGET));
        assert!(p.due());
        assert!(!p.adjust(-1));
        assert!(!p.due(), "released bytes are no longer spent");
        assert!(!p.pressure.load(Ordering::Relaxed));
        assert!(p.adjust(1), "and spending them again is a fresh crossing");
        p.adjust(-3 * BUDGET);
        assert!(
            p.spent.load(Ordering::Relaxed) < 0,
            "net growth can be negative"
        );
        assert!(!p.adjust(2 * BUDGET));
        assert!(!p.due());
    }

    /// A full collection settles what was spent: the budget restarts from
    /// zero, the collection is counted, and nothing is due any more.
    #[test]
    fn a_full_collection_resets_spending() {
        let heap = BexHeap::new(vec![]);
        heap.gc_policy.adjust(BUDGET);
        assert!(heap.should_gc());
        assert!(heap.gc_pressure().load(Ordering::Relaxed));
        // SAFETY: standalone heap, no concurrent users or retained pointers.
        unsafe {
            heap.collect_garbage(&[]);
        }
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
        assert_eq!(heap.gc_budget().full_collections, 1);
        assert!(!heap.should_gc());
    }

    /// A full collection makes every outstanding balance stale: what it
    /// counted is in the census now, so invalidating the allocator discards
    /// it rather than settling it a second time.
    #[test]
    fn invalidation_discards_the_balance_after_a_full_collection() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        assert_eq!(tlab.alloc_debt().balance(), 1000);
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        assert_eq!(tlab.alloc_debt().balance(), 0);
        assert_eq!(heap.gc_budget().bytes_since_full_gc, 0);
    }

    #[test]
    fn reservations_grow_without_wasting_alignment_slots() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        for i in 1usize..=4096 {
            tlab.alloc_string("inline");
            let slots = if i <= 1024 {
                i.next_power_of_two().max(32)
            } else {
                i.div_ceil(1024) * 1024
            };
            assert_eq!(
                heap.gc_budget().bytes_since_full_gc,
                (slots * size_of::<Object>()) as isize
            );
        }
        tlab.alloc_string("inline");
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            (5120 * size_of::<Object>()) as isize
        );
    }

    #[test]
    fn collection_invalidates_capacity_but_preserves_reservation_growth() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        for _ in 0..65 {
            tlab.alloc_string("inline");
        }
        assert_eq!(tlab.remaining(), 63 * OBJECT_CELL_WORDS);
        // SAFETY: single-threaded heap with no surviving roots.
        unsafe {
            heap.collect_garbage(&[]);
        }
        tlab.invalidate();
        tlab.alloc_string("inline");
        assert_eq!(tlab.remaining(), 127 * OBJECT_CELL_WORDS);
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            (128 * size_of::<Object>()) as isize
        );
    }

    /// Payload spends the budget as soon as it is allocated: one buffer the
    /// size of the whole allowance is due on its own, and every handle to a
    /// shared buffer is charged in full (an overestimate), while a slice
    /// charges only what it views.
    #[test]
    fn backing_storage_spends_the_budget() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        tlab.alloc_uint8array(vec![0; MIN_FULL_BUDGET]);
        assert!(heap.should_gc(), "one full-budget buffer is due on its own");
        assert!(
            tlab.take_budget_crossed(),
            "the allocator that crossed the budget is told so"
        );
        assert!(!tlab.take_budget_crossed(), "and told once");

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let text = bex_str::BexStr::from("x".repeat(1 << 20));
        let whole = text.unshared_heap_bytes() as isize;
        for _ in 0..8 {
            tlab.alloc_string(text.clone());
            tlab.alloc_string(text.byte_slice(1..128));
        }
        tlab.flush_alloc_debt();
        let spent = heap.gc_budget().bytes_since_full_gc;
        let slots = (32 * size_of::<Object>()) as isize;
        assert_eq!(spent, slots + 8 * whole + 8 * 127);
        assert!(!heap.should_gc());
    }

    /// A payload too small to settle on its own is settled when the TLAB
    /// refills, when its owner asks, and when the TLAB is dropped. The TLAB
    /// starts with a chunk, so the first allocation does not refill.
    #[test]
    fn small_payloads_settle_at_refill_on_request_and_on_drop() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap.clone());
        tlab.alloc_uint8array(vec![0; 1000]);
        let slots = (32 * size_of::<Object>()) as isize;
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots);
        assert_eq!(tlab.alloc_debt().balance(), 1000);

        tlab.flush_alloc_debt();
        assert_eq!(heap.gc_budget().bytes_since_full_gc, slots + 1000);
        assert_eq!(tlab.alloc_debt().balance(), 0);

        for _ in 0..32 {
            tlab.alloc_uint8array(vec![0; 10]);
        }
        // The 33rd allocation refills, settling the 320 bytes with the slots.
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            slots + 1000 + (32 * size_of::<Object>()) as isize + 320
        );

        tlab.alloc_uint8array(vec![0; 10]);
        drop(tlab);
        assert_eq!(
            heap.gc_budget().bytes_since_full_gc,
            slots + 1000 + (32 * size_of::<Object>()) as isize + 330
        );
    }

    /// The budget after a full collection covers the survivors' payload, not
    /// just their slots.
    #[test]
    fn the_budget_after_a_full_collection_includes_live_payload() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new_empty(heap.clone());
        let live = tlab.alloc_uint8array(vec![0; 16 * MIN_FULL_BUDGET]);
        // SAFETY: single-threaded heap; `live` is the only root.
        unsafe {
            heap.collect_garbage(&[live]);
        }
        let budget = heap.gc_budget().full_budget_bytes;
        assert!(
            budget >= 4 * 16 * MIN_FULL_BUDGET,
            "{budget} bytes of headroom for a 16-budget live buffer"
        );
    }
}
