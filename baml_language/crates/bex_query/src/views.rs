use std::collections::{BTreeMap, HashMap};

use crate::{Completeness, Counters, FoldedCct, QueryError};

pub const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const HARD_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PIXEL_WIDTH: u32 = 8192;
pub const MAX_LANES: u16 = 256;

const LEFT_HEAVY_ROW_BYTES: usize = 80;
const TIMELINE_ROW_BYTES: usize = 96;
const RESPONSE_OVERHEAD_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub start_ns: u64,
    pub end_ns: u64,
    pub pixel_width: u32,
    pub lanes: u16,
    pub max_bytes: usize,
}

impl Viewport {
    pub fn validate(self) -> Result<Self, QueryError> {
        if self.end_ns <= self.start_ns {
            return Err(QueryError::invalid_request(
                "viewport end_ns must be greater than start_ns",
            ));
        }
        if self.pixel_width == 0 || self.pixel_width > MAX_PIXEL_WIDTH {
            return Err(QueryError::invalid_request(format!(
                "pixel_width must be in 1..={MAX_PIXEL_WIDTH}"
            )));
        }
        if self.lanes == 0 || self.lanes > MAX_LANES {
            return Err(QueryError::invalid_request(format!(
                "lanes must be in 1..={MAX_LANES}"
            )));
        }
        validate_max_bytes(self.max_bytes)?;
        Ok(self)
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            start_ns: 0,
            end_ns: 1,
            pixel_width: 1024,
            lanes: 64,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeftHeavyRequest {
    pub pixel_width: u32,
    pub max_bytes: usize,
}

impl Default for LeftHeavyRequest {
    fn default() -> Self {
        Self {
            pixel_width: 1024,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftHeavyNode {
    pub node_id: u32,
    /// `u32::MAX` denotes a root.
    pub parent_row: u32,
    pub function_id: u32,
    pub depth: u16,
    pub extent_ppm: u32,
    pub counters: Counters,
    pub synthetic_smaller: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftHeavyResponse {
    pub nodes: Vec<LeftHeavyNode>,
    pub effective_pixel_width: u32,
    pub meta: Completeness,
}

pub fn left_heavy(
    cct: &FoldedCct,
    request: LeftHeavyRequest,
) -> Result<LeftHeavyResponse, QueryError> {
    if request.pixel_width == 0 || request.pixel_width > MAX_PIXEL_WIDTH {
        return Err(QueryError::invalid_request(format!(
            "pixel_width must be in 1..={MAX_PIXEL_WIDTH}"
        )));
    }
    validate_max_bytes(request.max_bytes)?;
    let row_budget = response_row_budget(request.max_bytes, LEFT_HEAVY_ROW_BYTES);
    let mut effective_width = request.pixel_width;
    let mut nodes = build_left_heavy(cct, effective_width, row_budget);
    while nodes.len() > row_budget && effective_width > 1 {
        effective_width = effective_width.div_ceil(2);
        nodes = build_left_heavy(cct, effective_width, row_budget);
    }
    let mut meta = cct.meta.clone();
    if effective_width != request.pixel_width {
        meta.lod_degraded = true;
        meta.warnings.push(format!(
            "Left Heavy LOD reduced from {} to {} effective pixels to honor max_bytes",
            request.pixel_width, effective_width
        ));
    }
    if nodes.len() > row_budget {
        nodes.truncate(row_budget);
        meta.truncated = true;
        meta.lod_degraded = true;
        meta.warnings
            .push("Left Heavy roots exceeded the response byte budget".to_owned());
    }
    meta.finalize();
    Ok(LeftHeavyResponse {
        nodes,
        effective_pixel_width: effective_width,
        meta,
    })
}

fn build_left_heavy(cct: &FoldedCct, pixel_width: u32, row_budget: usize) -> Vec<LeftHeavyNode> {
    let mut children = HashMap::<u32, Vec<u32>>::new();
    let mut roots = Vec::new();
    for node in cct.nodes.values() {
        if node.parent_node_id == 0 || !cct.nodes.contains_key(&node.parent_node_id) {
            roots.push(node.node_id);
        } else {
            children
                .entry(node.parent_node_id)
                .or_default()
                .push(node.node_id);
        }
    }
    let sort_nodes = |nodes: &mut Vec<u32>| {
        nodes.sort_by_key(|node_id| {
            let node = &cct.nodes[node_id];
            (std::cmp::Reverse(node.counters.total_ns), *node_id)
        });
    };
    sort_nodes(&mut roots);
    for nodes in children.values_mut() {
        sort_nodes(nodes);
    }
    let total_root_ns = roots
        .iter()
        .map(|node_id| cct.nodes[node_id].counters.total_ns)
        .max()
        .unwrap_or(0)
        .max(1);
    let mut output = Vec::new();
    for root in roots {
        if output.len() >= row_budget.saturating_add(1) {
            break;
        }
        emit_left_heavy(
            cct,
            &children,
            root,
            u32::MAX,
            0,
            total_root_ns,
            pixel_width,
            row_budget.saturating_add(1),
            &mut output,
        );
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn emit_left_heavy(
    cct: &FoldedCct,
    children: &HashMap<u32, Vec<u32>>,
    node_id: u32,
    parent_row: u32,
    depth: u16,
    root_ns: u64,
    pixel_width: u32,
    row_budget: usize,
    output: &mut Vec<LeftHeavyNode>,
) {
    if output.len() >= row_budget {
        return;
    }
    let node = &cct.nodes[&node_id];
    let row_index = u32::try_from(output.len()).unwrap_or(u32::MAX);
    output.push(LeftHeavyNode {
        node_id,
        parent_row,
        function_id: node.function_id,
        depth,
        extent_ppm: ratio_ppm(node.counters.total_ns, root_ns),
        counters: node.counters,
        synthetic_smaller: false,
    });
    let Some(child_ids) = children.get(&node_id) else {
        return;
    };
    let mut smaller = Counters::default();
    let mut has_smaller = false;
    for child_id in child_ids {
        let child = &cct.nodes[child_id];
        let visible = u128::from(child.counters.total_ns)
            .saturating_mul(u128::from(pixel_width))
            .saturating_mul(2)
            >= u128::from(root_ns);
        if visible {
            emit_left_heavy(
                cct,
                children,
                *child_id,
                row_index,
                depth.saturating_add(1),
                root_ns,
                pixel_width,
                row_budget,
                output,
            );
        } else {
            add_counters(&mut smaller, child.counters);
            has_smaller = true;
        }
    }
    if has_smaller && output.len() < row_budget {
        output.push(LeftHeavyNode {
            node_id: u32::MAX,
            parent_row: row_index,
            function_id: 0,
            depth: depth.saturating_add(1),
            extent_ppm: ratio_ppm(smaller.total_ns, root_ns),
            counters: smaller,
            synthetic_smaller: true,
        });
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineTiers {
    pub exact_recency: bool,
    pub aggregate_bands: bool,
    pub exact_evidence: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineBand {
    pub lane: u16,
    pub bucket: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub busy_ppm: u32,
    pub awaiting_ppm: u32,
    pub dominant_function_id: u32,
    pub error_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExactTimelineTier {
    Recency = 1,
    Evidence = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactTimelineCall {
    pub tier: ExactTimelineTier,
    pub logical_thread_id: u64,
    pub call_id: u64,
    pub node_id: u32,
    pub function_id: u32,
    pub start_ns: u64,
    /// Zero denotes an open call whose visible end follows the viewport tail.
    pub end_ns: u64,
    pub status: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimelineOverlay {
    pub exact_calls: Vec<ExactTimelineCall>,
    pub evicted_recent_calls: u64,
}

impl TimelineOverlay {
    /// Converts the exact completed-call ring of a live CCT snapshot into the
    /// query tier. Open calls are not yet exposed by `CctSnapshot`; callers may
    /// append them explicitly with `end_ns = 0`.
    #[must_use]
    pub fn from_cct_snapshot(snapshot: &bex_events::prof::cct::CctSnapshot) -> Self {
        let function_by_node = snapshot
            .nodes
            .iter()
            .map(|node| (node.node_id, node.identity.function_id.0))
            .collect::<HashMap<_, _>>();
        Self {
            exact_calls: snapshot
                .recent_calls
                .iter()
                .map(|call| ExactTimelineCall {
                    tier: ExactTimelineTier::Recency,
                    logical_thread_id: call.thread_id,
                    call_id: call.call_id,
                    node_id: call.node_id,
                    function_id: function_by_node.get(&call.node_id).copied().unwrap_or(0),
                    start_ns: call.start_ns,
                    end_ns: call.end_ns,
                    status: call.status as u8,
                })
                .collect(),
            evicted_recent_calls: snapshot.health.evicted_calls,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineRect {
    pub tier: ExactTimelineTier,
    pub lane: u16,
    pub logical_thread_id: u64,
    pub call_id: u64,
    pub node_id: u32,
    pub function_id: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status: u8,
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineLane {
    pub lane: u16,
    pub logical_thread_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineResponse {
    pub lanes: Vec<TimelineLane>,
    pub bands: Vec<TimelineBand>,
    pub exact_rects: Vec<TimelineRect>,
    pub evicted_recent_calls: u64,
    pub requested_pixel_width: u32,
    pub effective_pixel_width: u32,
    pub aggregate_resolution_ns: u64,
    pub tiers: TimelineTiers,
    pub meta: Completeness,
}

#[derive(Clone, Copy, Debug, Default)]
struct BandAccumulator {
    busy_ns: u64,
    awaiting_ns: u64,
    error_count: u64,
    dominant_weight: u64,
    dominant_function_id: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct WindowAccumulator {
    busy_ns: u64,
    awaiting_ns: u64,
    error_count: u64,
    dominant_weight: u64,
    dominant_function_id: u32,
}

pub fn timeline(cct: &FoldedCct, viewport: Viewport) -> Result<TimelineResponse, QueryError> {
    let viewport = viewport.validate()?;
    let mut thread_weights = BTreeMap::<u64, u64>::new();
    for node in cct.nodes.values() {
        let weight = thread_weights.entry(node.logical_thread_id).or_default();
        *weight = weight.saturating_add(node.counters.self_ns);
    }
    let distinct_thread_count = thread_weights.len();
    let mut threads = thread_weights.into_iter().collect::<Vec<_>>();
    threads.sort_by_key(|(thread_id, weight)| (std::cmp::Reverse(*weight), *thread_id));
    let requested_lane_count = usize::from(viewport.lanes).min(threads.len());
    let row_budget = response_row_budget(viewport.max_bytes, TIMELINE_ROW_BYTES);
    let lane_count = requested_lane_count.min(row_budget.max(1));
    threads.truncate(lane_count);
    let max_buckets = if lane_count == 0 {
        usize::try_from(viewport.pixel_width).unwrap_or(0)
    } else {
        row_budget / lane_count
    };
    let effective_width = usize::try_from(viewport.pixel_width)
        .unwrap_or(usize::MAX)
        .min(max_buckets.max(1))
        .max(1);
    let effective_width = u32::try_from(effective_width).unwrap_or(MAX_PIXEL_WIDTH);
    let duration = viewport.end_ns - viewport.start_ns;
    let bucket_ns = duration.div_ceil(u64::from(effective_width)).max(1);

    let lanes = threads
        .iter()
        .enumerate()
        .map(|(lane, (thread_id, _))| TimelineLane {
            lane: u16::try_from(lane).unwrap_or(u16::MAX),
            logical_thread_id: *thread_id,
        })
        .collect::<Vec<_>>();
    let lane_by_thread = lanes
        .iter()
        .map(|lane| (lane.logical_thread_id, lane.lane))
        .collect::<HashMap<_, _>>();
    let mut source_resolution_ns = u64::MAX;
    let mut source_windows = BTreeMap::<(u16, u64, u64), WindowAccumulator>::new();
    for window in &cct.windows {
        if window.last_ts_ns <= viewport.start_ns || window.first_ts_ns >= viewport.end_ns {
            continue;
        }
        let Some(node) = cct.nodes.get(&window.node_id) else {
            continue;
        };
        let Some(lane) = lane_by_thread.get(&node.logical_thread_id).copied() else {
            continue;
        };
        let window_span = window.last_ts_ns.saturating_sub(window.first_ts_ns);
        if window_span != 0 {
            source_resolution_ns = source_resolution_ns.min(window_span);
        }
        let accumulator = source_windows
            .entry((lane, window.first_ts_ns, window.last_ts_ns))
            .or_default();
        accumulator.busy_ns = accumulator.busy_ns.saturating_add(window.counters.self_ns);
        accumulator.awaiting_ns = accumulator
            .awaiting_ns
            .saturating_add(window.counters.await_ns);
        accumulator.error_count = accumulator
            .error_count
            .saturating_add(window.counters.errors());
        if window.counters.total_ns > accumulator.dominant_weight {
            accumulator.dominant_weight = window.counters.total_ns;
            accumulator.dominant_function_id = node.function_id;
        }
    }
    let mut accumulators = BTreeMap::<(u16, u32), BandAccumulator>::new();
    for ((lane, window_start, window_end), source_window) in source_windows {
        let visible_start = window_start.max(viewport.start_ns);
        let visible_end = window_end.min(viewport.end_ns);
        if visible_end <= visible_start {
            continue;
        }
        let window_span = window_end.saturating_sub(window_start).max(1);
        let first_bucket = ((visible_start - viewport.start_ns) / bucket_ns)
            .min(u64::from(effective_width.saturating_sub(1)));
        let last_bucket = ((visible_end - 1 - viewport.start_ns) / bucket_ns)
            .min(u64::from(effective_width.saturating_sub(1)));
        let error_bucket = window_start
            .saturating_add(window_span / 2)
            .saturating_sub(viewport.start_ns)
            .checked_div(bucket_ns)
            .unwrap_or(0)
            .min(u64::from(effective_width.saturating_sub(1)));
        for bucket in first_bucket..=last_bucket {
            let bucket_start = viewport
                .start_ns
                .saturating_add(bucket.saturating_mul(bucket_ns));
            let bucket_end = bucket_start.saturating_add(bucket_ns).min(viewport.end_ns);
            let overlap = visible_end
                .min(bucket_end)
                .saturating_sub(visible_start.max(bucket_start));
            if overlap == 0 {
                continue;
            }
            let bucket = u32::try_from(bucket).unwrap_or(effective_width.saturating_sub(1));
            let accumulator = accumulators.entry((lane, bucket)).or_default();
            accumulator.busy_ns = accumulator.busy_ns.saturating_add(scale_duration(
                source_window.busy_ns,
                overlap,
                window_span,
            ));
            accumulator.awaiting_ns = accumulator.awaiting_ns.saturating_add(scale_duration(
                source_window.awaiting_ns,
                overlap,
                window_span,
            ));
            if u64::from(bucket) == error_bucket {
                accumulator.error_count = accumulator
                    .error_count
                    .saturating_add(source_window.error_count);
            }
            let dominant_weight =
                scale_duration(source_window.dominant_weight, overlap, window_span);
            if dominant_weight > accumulator.dominant_weight {
                accumulator.dominant_weight = dominant_weight;
                accumulator.dominant_function_id = source_window.dominant_function_id;
            }
        }
    }
    let bands = accumulators
        .into_iter()
        .map(|((lane, bucket), accumulator)| {
            let start_ns = viewport
                .start_ns
                .saturating_add(u64::from(bucket).saturating_mul(bucket_ns));
            let end_ns = start_ns.saturating_add(bucket_ns).min(viewport.end_ns);
            let span = end_ns.saturating_sub(start_ns).max(1);
            TimelineBand {
                lane,
                bucket,
                start_ns,
                end_ns,
                busy_ppm: ratio_ppm(accumulator.busy_ns.min(span), span),
                awaiting_ppm: ratio_ppm(accumulator.awaiting_ns.min(span), span),
                dominant_function_id: accumulator.dominant_function_id,
                error_count: accumulator.error_count,
            }
        })
        .collect::<Vec<_>>();

    let mut meta = cct.meta.clone();
    meta.more_lanes = lane_count < distinct_thread_count;
    if effective_width != viewport.pixel_width || lane_count < requested_lane_count {
        meta.lod_degraded = true;
        meta.warnings.push(format!(
            "timeline LOD reduced to {effective_width} buckets across {lane_count} lanes to honor max_bytes"
        ));
    }
    let source_resolution_ns = if source_resolution_ns == u64::MAX {
        bucket_ns
    } else {
        source_resolution_ns
    };
    if bucket_ns < source_resolution_ns {
        meta.warnings.push(format!(
            "aggregate resolution limit is {source_resolution_ns} ns; zoomed buckets do not imply exact calls"
        ));
    }
    meta.warnings.push(
        "exact-recency calls are unavailable in BCCT storage; rendering aggregate activity bands"
            .to_owned(),
    );
    meta.warnings
        .push("exact-evidence overlays require flight-recorder or full-trace artifacts".to_owned());
    meta.finalize();
    Ok(TimelineResponse {
        lanes,
        bands,
        exact_rects: Vec::new(),
        evicted_recent_calls: 0,
        requested_pixel_width: viewport.pixel_width,
        effective_pixel_width: effective_width,
        aggregate_resolution_ns: source_resolution_ns.max(bucket_ns),
        tiers: TimelineTiers {
            exact_recency: false,
            aggregate_bands: true,
            exact_evidence: false,
        },
        meta,
    })
}

/// Adds honest exact-recency and exact-evidence rectangles to the aggregate
/// timeline. Aggregate bands remain bands; exact calls are never synthesized
/// from counters.
pub fn timeline_with_overlay(
    cct: &FoldedCct,
    viewport: Viewport,
    overlay: &TimelineOverlay,
) -> Result<TimelineResponse, QueryError> {
    let mut response = timeline(cct, viewport)?;
    let lane_by_thread = response
        .lanes
        .iter()
        .map(|lane| (lane.logical_thread_id, lane.lane))
        .collect::<HashMap<_, _>>();
    let mut unavailable_lanes = false;
    let mut exact_rects = overlay
        .exact_calls
        .iter()
        .filter_map(|call| {
            let end_ns = if call.end_ns == 0 {
                viewport.end_ns
            } else {
                call.end_ns
            };
            if end_ns <= viewport.start_ns || call.start_ns >= viewport.end_ns {
                return None;
            }
            let Some(lane) = lane_by_thread.get(&call.logical_thread_id).copied() else {
                unavailable_lanes = true;
                return None;
            };
            Some(TimelineRect {
                tier: call.tier,
                lane,
                logical_thread_id: call.logical_thread_id,
                call_id: call.call_id,
                node_id: call.node_id,
                function_id: call.function_id,
                start_ns: call.start_ns.max(viewport.start_ns),
                end_ns: end_ns.min(viewport.end_ns),
                status: call.status,
                open: call.end_ns == 0,
            })
        })
        .collect::<Vec<_>>();
    exact_rects.sort_by_key(|rect| {
        (
            rect.lane,
            rect.start_ns,
            rect.end_ns,
            rect.tier as u8,
            rect.call_id,
        )
    });

    let row_budget = response_row_budget(viewport.max_bytes, TIMELINE_ROW_BYTES);
    if exact_rects.len() > row_budget {
        // Preserve the newest exact calls, which is also the recency-ring
        // contract, while making the LOD loss explicit.
        exact_rects.drain(..exact_rects.len() - row_budget);
        response.meta.truncated = true;
        response.meta.lod_degraded = true;
        response
            .meta
            .warnings
            .push("exact timeline rectangles were trimmed to the response byte budget".to_owned());
    }
    let aggregate_budget = row_budget.saturating_sub(exact_rects.len());
    if response.bands.len() > aggregate_budget {
        response.bands.truncate(aggregate_budget);
        response.meta.truncated = true;
        response.meta.lod_degraded = true;
        response
            .meta
            .warnings
            .push("aggregate timeline bands were trimmed to preserve exact evidence".to_owned());
    }
    if unavailable_lanes {
        response.meta.more_lanes = true;
        response
            .meta
            .warnings
            .push("exact calls on lanes outside the requested lane budget were omitted".to_owned());
    }
    response
        .meta
        .warnings
        .retain(|warning| !warning.starts_with("exact-recency calls are unavailable"));
    response.meta.warnings.retain(|warning| {
        !warning.starts_with("exact-evidence overlays require")
            || !overlay
                .exact_calls
                .iter()
                .any(|call| call.tier == ExactTimelineTier::Evidence)
    });
    response.tiers.exact_recency = overlay
        .exact_calls
        .iter()
        .any(|call| call.tier == ExactTimelineTier::Recency);
    response.tiers.exact_evidence = overlay
        .exact_calls
        .iter()
        .any(|call| call.tier == ExactTimelineTier::Evidence);
    response.exact_rects = exact_rects;
    response.evicted_recent_calls = overlay.evicted_recent_calls;
    if overlay.evicted_recent_calls != 0 {
        response.meta.warnings.push(format!(
            "showing exact recent calls; {} older calls are represented only by aggregates",
            overlay.evicted_recent_calls
        ));
    }
    response.meta.finalize();
    Ok(response)
}

fn scale_duration(value: u64, overlap: u64, span: u64) -> u64 {
    u64::try_from(
        u128::from(value)
            .saturating_mul(u128::from(overlap))
            .checked_div(u128::from(span.max(1)))
            .unwrap_or(0)
            .min(u128::from(u64::MAX)),
    )
    .unwrap_or(u64::MAX)
}

fn response_row_budget(max_bytes: usize, row_bytes: usize) -> usize {
    max_bytes
        .saturating_sub(RESPONSE_OVERHEAD_BYTES)
        .checked_div(row_bytes)
        .unwrap_or(0)
        .max(1)
}

fn validate_max_bytes(max_bytes: usize) -> Result<(), QueryError> {
    if max_bytes < RESPONSE_OVERHEAD_BYTES {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must be at least {RESPONSE_OVERHEAD_BYTES}"
        )));
    }
    if max_bytes > HARD_MAX_BYTES {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must not exceed {HARD_MAX_BYTES}"
        )));
    }
    Ok(())
}

fn ratio_ppm(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    u32::try_from(
        (u128::from(numerator)
            .saturating_mul(1_000_000)
            .checked_div(u128::from(denominator))
            .unwrap_or(0))
        .min(1_000_000),
    )
    .unwrap_or(1_000_000)
}

fn add_counters(total: &mut Counters, value: Counters) {
    total.enters = total.enters.saturating_add(value.enters);
    total.ends_ok = total.ends_ok.saturating_add(value.ends_ok);
    total.ends_err = total.ends_err.saturating_add(value.ends_err);
    total.ends_cancel = total.ends_cancel.saturating_add(value.ends_cancel);
    total.ends_exit = total.ends_exit.saturating_add(value.ends_exit);
    total.total_ns = total.total_ns.saturating_add(value.total_ns);
    total.self_ns = total.self_ns.saturating_add(value.self_ns);
    total.await_ns = total.await_ns.saturating_add(value.await_ns);
}
