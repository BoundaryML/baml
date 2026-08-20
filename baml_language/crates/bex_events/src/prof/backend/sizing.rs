use std::path::PathBuf;

/// The only disk-policy inputs accepted by the MVP backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskBudget {
    pub max_project_bytes: u64,
    pub minimum_free_bytes: u64,
}

/// Five-input profiler configuration. Only `enabled` and `store_root` are
/// host-facing; tests may inject the three resource values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilerConfig {
    pub enabled: bool,
    pub store_root: PathBuf,
    pub process_memory_bytes: u64,
    pub disk: DiskBudget,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            enabled: cfg!(not(target_arch = "wasm32")),
            store_root: PathBuf::from(".baml/profiles-v1"),
            process_memory_bytes: 256 * 1024 * 1024,
            disk: DiskBudget {
                max_project_bytes: 10 * 1024 * 1024 * 1024,
                minimum_free_bytes: 1024 * 1024 * 1024,
            },
        }
    }
}

/// Frozen accounted layout measurements used by sizing policy v1. Values are
/// capacity charges, so they include the documented allocator/queue margin
/// rather than claiming allocator-exact occupied sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredLayouts {
    pub boundary_slot_bytes: u64,
    pub population_item_min_bytes: u64,
    pub transport_segment_bytes: u64,
    pub active_thread_min_bytes: u64,
    pub active_call_min_bytes: u64,
    pub unresolved_fact_min_bytes: u64,
    pub evidence_item_min_bytes: u64,
    pub value_root_min_bytes: u64,
    pub writer_batch_min_bytes: u64,
    pub fixed_control_bytes: u64,
}

impl MeasuredLayouts {
    pub const V1: Self = Self {
        boundary_slot_bytes: 8 * 1024,
        population_item_min_bytes: 256,
        transport_segment_bytes: 256 * 1024,
        active_thread_min_bytes: 192,
        active_call_min_bytes: 320,
        unresolved_fact_min_bytes: 256,
        evidence_item_min_bytes: 256,
        value_root_min_bytes: 512,
        writer_batch_min_bytes: 64 * 1024,
        fixed_control_bytes: 256 * 1024,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedSizing {
    pub policy_version: u16,
    pub total_bytes: u64,
    pub control_reserve_bytes: u64,
    pub manual_reserve_bytes: u64,
    pub general_bytes: u64,
    pub boundary_slots: u32,
    pub transport_segment_bytes: u64,
    pub transport_freelist_segments: u32,
    pub cct_epoch_target_bytes: u64,
    pub segment_target_bytes: u64,
    pub single_value_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidMemoryBudget {
    pub requested_bytes: u64,
    pub minimum_bytes: u64,
}

pub struct ProfilerSizingPolicy;

impl ProfilerSizingPolicy {
    pub const VERSION: u16 = 1;
    const CONTROL_MIN: u64 = 2 * 1024 * 1024;
    const CONTROL_MAX: u64 = 16 * 1024 * 1024;
    const MANUAL_MIN: u64 = 2 * 1024 * 1024;
    const MANUAL_MAX: u64 = 16 * 1024 * 1024;
    const GENERAL_MIN: u64 = 4 * 1024 * 1024;
    const SEGMENT_MIN: u64 = 256 * 1024;
    const SEGMENT_MAX: u64 = 4 * 1024 * 1024;
    const EPOCH_MIN: u64 = 1024 * 1024;
    const EPOCH_MAX: u64 = 64 * 1024 * 1024;
    const SINGLE_VALUE_MAX: u64 = 16 * 1024 * 1024;

    /// Pure sizing policy. Integer division and clamps are the only adaptive
    /// operations; there are no hidden counts or environment inputs.
    pub fn derive(
        process_memory_bytes: u64,
        measured: MeasuredLayouts,
    ) -> Result<DerivedSizing, InvalidMemoryBudget> {
        let minimum_bytes = measured
            .fixed_control_bytes
            .saturating_add(measured.boundary_slot_bytes)
            .saturating_add(measured.transport_segment_bytes)
            .saturating_add(Self::CONTROL_MIN)
            .saturating_add(Self::MANUAL_MIN)
            .saturating_add(Self::GENERAL_MIN);
        if process_memory_bytes < minimum_bytes {
            return Err(InvalidMemoryBudget {
                requested_bytes: process_memory_bytes,
                minimum_bytes,
            });
        }

        let control_reserve_bytes =
            (process_memory_bytes / 16).clamp(Self::CONTROL_MIN, Self::CONTROL_MAX);
        let manual_reserve_bytes =
            (process_memory_bytes / 16).clamp(Self::MANUAL_MIN, Self::MANUAL_MAX);
        let general_bytes = process_memory_bytes
            .checked_sub(control_reserve_bytes)
            .and_then(|n| n.checked_sub(manual_reserve_bytes))
            .expect("minimum budget preserves general capacity");

        let slot_bytes = control_reserve_bytes.saturating_sub(measured.fixed_control_bytes)
            / measured.boundary_slot_bytes.max(1);
        let boundary_slots = u32::try_from(slot_bytes.clamp(1, 4096)).unwrap_or(4096);
        let segment_target_bytes = (general_bytes / 32)
            .clamp(Self::SEGMENT_MIN, Self::SEGMENT_MAX)
            .max(measured.writer_batch_min_bytes);
        let cct_epoch_target_bytes = (general_bytes / 4).clamp(Self::EPOCH_MIN, Self::EPOCH_MAX);
        let value_working_bytes = general_bytes / 4;
        let single_value_bytes = (value_working_bytes / 4)
            .min(Self::SINGLE_VALUE_MAX)
            .max(measured.value_root_min_bytes);
        Ok(DerivedSizing {
            policy_version: Self::VERSION,
            total_bytes: process_memory_bytes,
            control_reserve_bytes,
            manual_reserve_bytes,
            general_bytes,
            boundary_slots,
            transport_segment_bytes: measured.transport_segment_bytes,
            transport_freelist_segments: 2,
            cct_epoch_target_bytes,
            segment_target_bytes,
            single_value_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;
    use crate::{
        ids::{CallRef, ThreadRef},
        prof::{
            backend::{CapturePlan, ContextKey, ContextTuple, RootProfiler},
            record::{CallSiteSourceSpan, RawRecord},
        },
    };

    #[test]
    fn phase_zero_rust_layouts_are_frozen_on_64_bit_targets() {
        if usize::BITS != 64 {
            return;
        }
        assert_eq!(size_of::<ThreadRef>(), 32);
        assert_eq!(size_of::<CallRef>(), 40);
        assert_eq!(size_of::<CallSiteSourceSpan>(), 16);
        assert_eq!(size_of::<RawRecord<'static>>(), 72);
        assert_eq!(size_of::<CapturePlan>(), 3);
        assert_eq!(size_of::<ContextKey>(), 32);
        assert_eq!(size_of::<ContextTuple>(), 76);
        assert_eq!(size_of::<RootProfiler>(), 17);
        assert_eq!(size_of::<crate::prof::backend::ProfilerSession>(), 8);
    }

    #[test]
    fn representative_256_mib_fixture_is_frozen() {
        assert_eq!(
            ProfilerSizingPolicy::derive(256 * 1024 * 1024, MeasuredLayouts::V1),
            Ok(DerivedSizing {
                policy_version: 1,
                total_bytes: 268_435_456,
                control_reserve_bytes: 16_777_216,
                manual_reserve_bytes: 16_777_216,
                general_bytes: 234_881_024,
                boundary_slots: 2016,
                transport_segment_bytes: 262_144,
                transport_freelist_segments: 2,
                cct_epoch_target_bytes: 58_720_256,
                segment_target_bytes: 4_194_304,
                single_value_bytes: 14_680_064,
            })
        );
    }

    #[test]
    fn representative_32_mib_fixture_is_frozen() {
        assert_eq!(
            ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1),
            Ok(DerivedSizing {
                policy_version: 1,
                total_bytes: 33_554_432,
                control_reserve_bytes: 2_097_152,
                manual_reserve_bytes: 2_097_152,
                general_bytes: 29_360_128,
                boundary_slots: 224,
                transport_segment_bytes: 262_144,
                transport_freelist_segments: 2,
                cct_epoch_target_bytes: 7_340_032,
                segment_target_bytes: 917_504,
                single_value_bytes: 1_835_008,
            })
        );
    }

    #[test]
    fn too_small_budget_is_rejected_without_partial_sizing() {
        let minimum = MeasuredLayouts::V1.fixed_control_bytes
            + MeasuredLayouts::V1.boundary_slot_bytes
            + MeasuredLayouts::V1.transport_segment_bytes
            + ProfilerSizingPolicy::CONTROL_MIN
            + ProfilerSizingPolicy::MANUAL_MIN
            + ProfilerSizingPolicy::GENERAL_MIN;
        assert_eq!(
            ProfilerSizingPolicy::derive(minimum - 1, MeasuredLayouts::V1),
            Err(InvalidMemoryBudget {
                requested_bytes: minimum - 1,
                minimum_bytes: minimum,
            })
        );
        assert!(ProfilerSizingPolicy::derive(minimum, MeasuredLayouts::V1).is_ok());
    }

    #[test]
    fn defaults_are_exactly_five_policy_inputs() {
        let config = ProfilerConfig::default();
        assert_eq!(config.store_root, PathBuf::from(".baml/profiles-v1"));
        assert_eq!(config.process_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(config.disk.max_project_bytes, 10 * 1024 * 1024 * 1024);
        assert_eq!(config.disk.minimum_free_bytes, 1024 * 1024 * 1024);
    }
}
