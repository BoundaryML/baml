use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{
    BqfBuilder, BqfFrame, ByteSource, Column, Completeness, Counters, DEFAULT_MAX_BYTES, FoldedCct,
    FrameFlags, FrameKind, HARD_MAX_BYTES, QueryEngine, QueryError, QueryPoll,
};

const ADVANCED_RESPONSE_OVERHEAD: usize = 1024;
const ADVANCED_ROW_BYTES: usize = 192;
const MAX_ADVANCED_ROWS: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionIdentity {
    pub function_id: u32,
    pub definition_key: String,
    pub fqn: String,
    pub def_content_hash: [u8; 32],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionDictionary {
    pub functions: Vec<FunctionIdentity>,
}

impl FunctionDictionary {
    /// Projects the persisted revision dictionary into the compact identity
    /// rows used by the aggregate search and cross-revision diff fast paths.
    #[must_use]
    pub fn from_revision_dictionary(
        dictionary: &bex_events::revision_dictionary::RevisionDictionary,
    ) -> Self {
        Self {
            functions: dictionary
                .functions
                .iter()
                .map(|function| FunctionIdentity {
                    function_id: function.function_id,
                    definition_key: function.definition_key.clone(),
                    fqn: function.fqn.clone(),
                    def_content_hash: function.def_content_hash,
                })
                .collect(),
        }
    }

    fn by_id(&self) -> Result<BTreeMap<u32, &FunctionIdentity>, QueryError> {
        let mut output = BTreeMap::new();
        for function in &self.functions {
            if output.insert(function.function_id, function).is_some() {
                return Err(QueryError::invalid_data(format!(
                    "duplicate function id {} in dictionary",
                    function.function_id
                )));
            }
            if function.definition_key.is_empty() {
                return Err(QueryError::invalid_data(
                    "function dictionary contains an empty definition_key",
                ));
            }
        }
        Ok(output)
    }

    fn by_definition_key(&self) -> Result<BTreeMap<&str, &FunctionIdentity>, QueryError> {
        let mut output = BTreeMap::new();
        for function in &self.functions {
            if output
                .insert(function.definition_key.as_str(), function)
                .is_some()
            {
                return Err(QueryError::invalid_data(format!(
                    "duplicate definition_key `{}` in dictionary",
                    function.definition_key
                )));
            }
        }
        Ok(output)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SandwichDirection {
    Caller = 1,
    Selected = 2,
    Callee = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandwichRequest {
    pub function_id: u32,
    pub caller_depth: u16,
    pub callee_depth: u16,
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl Default for SandwichRequest {
    fn default() -> Self {
        Self {
            function_id: 0,
            caller_depth: 8,
            callee_depth: 8,
            max_rows: 1_000,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandwichRow {
    pub direction: SandwichDirection,
    /// Distance from the selected function. Selected rows have depth zero.
    pub depth: u16,
    pub function_id: u32,
    pub counters: Counters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandwichResponse {
    pub selected_function_id: u32,
    pub rows: Vec<SandwichRow>,
    pub meta: Completeness,
}

impl SandwichResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let nrows = u32::try_from(self.rows.len())
            .map_err(|_| QueryError::invalid_request("too many Sandwich rows"))?;
        let mut builder = BqfBuilder::new(
            FrameKind::Sandwich,
            request_id,
            crate::bqf::data_epoch(&self.meta),
            nrows,
        )
        .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::U8 {
            id: 1,
            values: self.rows.iter().map(|row| row.direction as u8).collect(),
        })?;
        builder.push(Column::U16 {
            id: 2,
            values: self.rows.iter().map(|row| row.depth).collect(),
        })?;
        builder.push(Column::U32 {
            id: 3,
            values: self.rows.iter().map(|row| row.function_id).collect(),
        })?;
        push_counter_columns(&mut builder, &self.rows, |row| row.counters)?;
        builder.finish(max_bytes)
    }
}

/// Pure CCT Sandwich fold: callers above and callees below every matching
/// context, grouped by `(direction, depth, function_id)`.
pub fn sandwich(cct: &FoldedCct, request: SandwichRequest) -> Result<SandwichResponse, QueryError> {
    validate_advanced_budget(request.max_rows, request.max_bytes)?;
    let selected = cct
        .nodes
        .values()
        .filter(|node| node.function_id == request.function_id)
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    let mut meta = cct.meta.clone();
    if selected.is_empty() {
        meta.warnings.push(format!(
            "function id {} does not occur in this run",
            request.function_id
        ));
        meta.finalize();
        return Ok(SandwichResponse {
            selected_function_id: request.function_id,
            rows: Vec::new(),
            meta,
        });
    }

    let mut children = HashMap::<u32, Vec<u32>>::new();
    for node in cct.nodes.values() {
        children
            .entry(node.parent_node_id)
            .or_default()
            .push(node.node_id);
    }
    let mut grouped = BTreeMap::<(u8, u16, u32), Counters>::new();
    for node_id in selected {
        let node = &cct.nodes[&node_id];
        add_counters(
            grouped
                .entry((SandwichDirection::Selected as u8, 0, node.function_id))
                .or_default(),
            node.counters,
        );

        let mut ancestor = node.parent_node_id;
        for depth in 1..=request.caller_depth {
            let Some(parent) = cct.nodes.get(&ancestor) else {
                break;
            };
            add_counters(
                grouped
                    .entry((SandwichDirection::Caller as u8, depth, parent.function_id))
                    .or_default(),
                parent.counters,
            );
            ancestor = parent.parent_node_id;
        }

        let mut frontier = children.get(&node_id).cloned().unwrap_or_default();
        for depth in 1..=request.callee_depth {
            let mut next = Vec::new();
            for child_id in frontier {
                let Some(child) = cct.nodes.get(&child_id) else {
                    continue;
                };
                add_counters(
                    grouped
                        .entry((SandwichDirection::Callee as u8, depth, child.function_id))
                        .or_default(),
                    child.counters,
                );
                next.extend(children.get(&child_id).into_iter().flatten().copied());
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
    }

    let row_budget = request
        .max_rows
        .min(byte_row_budget(request.max_bytes))
        .min(MAX_ADVANCED_ROWS);
    let total_rows = grouped.len();
    let mut rows = grouped
        .into_iter()
        .map(|((direction, depth, function_id), counters)| SandwichRow {
            direction: match direction {
                1 => SandwichDirection::Caller,
                2 => SandwichDirection::Selected,
                _ => SandwichDirection::Callee,
            },
            depth,
            function_id,
            counters,
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| {
        (
            row.direction as u8,
            row.depth,
            std::cmp::Reverse(row.counters.total_ns),
            row.function_id,
        )
    });
    if rows.len() > row_budget {
        rows.truncate(row_budget);
        meta.truncated = true;
        meta.warnings.push(format!(
            "Sandwich view retained {row_budget} of {total_rows} grouped rows"
        ));
    }
    meta.finalize();
    Ok(SandwichResponse {
        selected_function_id: request.function_id,
        rows,
        meta,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchRequest {
    pub text: String,
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            max_rows: 100,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchRow {
    pub function: FunctionIdentity,
    pub counters: Counters,
    pub contexts: u32,
    /// 3 exact, 2 prefix, 1 substring.
    pub relevance: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResponse {
    pub rows: Vec<SearchRow>,
    pub meta: Completeness,
}

impl SearchResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let nrows = u32::try_from(self.rows.len())
            .map_err(|_| QueryError::invalid_request("too many search rows"))?;
        let mut builder = BqfBuilder::new(
            FrameKind::Search,
            request_id,
            crate::bqf::data_epoch(&self.meta),
            nrows,
        )
        .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::U32 {
            id: 1,
            values: self
                .rows
                .iter()
                .map(|row| row.function.function_id)
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 2,
            values: self
                .rows
                .iter()
                .map(|row| row.function.definition_key.clone())
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 3,
            values: self
                .rows
                .iter()
                .map(|row| row.function.fqn.clone())
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 4,
            values: self.rows.iter().map(|row| row.contexts).collect(),
        })?;
        builder.push(Column::U8 {
            id: 5,
            values: self.rows.iter().map(|row| row.relevance).collect(),
        })?;
        push_counter_columns_with_base(&mut builder, &self.rows, 10, |row| row.counters)?;
        builder.finish(max_bytes)
    }
}

/// Recognized search fast path over the revision dictionary and folded
/// aggregates. It never scans event/value bodies.
pub fn search_functions(
    cct: &FoldedCct,
    dictionary: &FunctionDictionary,
    request: &SearchRequest,
) -> Result<SearchResponse, QueryError> {
    validate_advanced_budget(request.max_rows, request.max_bytes)?;
    let by_id = dictionary.by_id()?;
    let query = request.text.trim().to_lowercase();
    let mut aggregate = BTreeMap::<u32, (Counters, u32)>::new();
    for node in cct.nodes.values() {
        let entry = aggregate.entry(node.function_id).or_default();
        add_counters(&mut entry.0, node.counters);
        entry.1 = entry.1.saturating_add(1);
    }
    let mut rows = Vec::new();
    for (function_id, (counters, contexts)) in aggregate {
        let Some(function) = by_id.get(&function_id) else {
            continue;
        };
        let fqn = function.fqn.to_lowercase();
        let definition_key = function.definition_key.to_lowercase();
        let relevance = if query.is_empty() {
            1
        } else if fqn == query || definition_key == query {
            3
        } else if fqn.starts_with(&query) || definition_key.starts_with(&query) {
            2
        } else if fqn.contains(&query) || definition_key.contains(&query) {
            1
        } else {
            continue;
        };
        rows.push(SearchRow {
            function: (*function).clone(),
            counters,
            contexts,
            relevance,
        });
    }
    rows.sort_by_key(|row| {
        (
            std::cmp::Reverse(row.relevance),
            std::cmp::Reverse(row.counters.total_ns),
            row.function.fqn.clone(),
        )
    });
    let row_budget = request.max_rows.min(byte_row_budget(request.max_bytes));
    let mut meta = cct.meta.clone();
    if rows.len() > row_budget {
        rows.truncate(row_budget);
        meta.truncated = true;
        meta.warnings
            .push("function search was truncated by its bounded-size contract".to_owned());
    }
    meta.finalize();
    Ok(SearchResponse { rows, meta })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignedCounters {
    pub calls: i64,
    pub errors: i64,
    pub total_ns: i64,
    pub self_ns: i64,
    pub awaiting_ns: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DiffPresence {
    Both = 0,
    Added = 1,
    Removed = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    pub definition_key: String,
    pub fqn: String,
    pub left_function_id: Option<u32>,
    pub right_function_id: Option<u32>,
    pub presence: DiffPresence,
    pub definition_changed: bool,
    pub left: Counters,
    pub right: Counters,
    pub delta: SignedCounters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffRequest {
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl Default for DiffRequest {
    fn default() -> Self {
        Self {
            max_rows: 1_000,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffResponse {
    pub rows: Vec<DiffRow>,
    pub meta: Completeness,
}

impl DiffResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let nrows = u32::try_from(self.rows.len())
            .map_err(|_| QueryError::invalid_request("too many diff rows"))?;
        let mut builder = BqfBuilder::new(
            FrameKind::Diff,
            request_id,
            crate::bqf::data_epoch(&self.meta),
            nrows,
        )
        .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::Utf8 {
            id: 1,
            values: self
                .rows
                .iter()
                .map(|row| row.definition_key.clone())
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 2,
            values: self.rows.iter().map(|row| row.fqn.clone()).collect(),
        })?;
        builder.push(Column::U32 {
            id: 3,
            values: self
                .rows
                .iter()
                .map(|row| row.left_function_id.unwrap_or(u32::MAX))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 4,
            values: self
                .rows
                .iter()
                .map(|row| row.right_function_id.unwrap_or(u32::MAX))
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 5,
            values: self.rows.iter().map(|row| row.presence as u8).collect(),
        })?;
        builder.push(Column::U8 {
            id: 6,
            values: self
                .rows
                .iter()
                .map(|row| u8::from(row.definition_changed))
                .collect(),
        })?;
        builder.push(Column::I64 {
            id: 10,
            values: self.rows.iter().map(|row| row.delta.calls).collect(),
        })?;
        builder.push(Column::I64 {
            id: 11,
            values: self.rows.iter().map(|row| row.delta.errors).collect(),
        })?;
        builder.push(Column::I64 {
            id: 12,
            values: self.rows.iter().map(|row| row.delta.total_ns).collect(),
        })?;
        builder.push(Column::I64 {
            id: 13,
            values: self.rows.iter().map(|row| row.delta.self_ns).collect(),
        })?;
        builder.push(Column::I64 {
            id: 14,
            values: self.rows.iter().map(|row| row.delta.awaiting_ns).collect(),
        })?;
        builder.finish(max_bytes)
    }
}

/// Cross-revision aggregate diff aligned by stable `definition_key`.
pub fn diff_cct(
    left: &FoldedCct,
    left_dictionary: &FunctionDictionary,
    right: &FoldedCct,
    right_dictionary: &FunctionDictionary,
    request: DiffRequest,
) -> Result<DiffResponse, QueryError> {
    validate_advanced_budget(request.max_rows, request.max_bytes)?;
    let left_by_key = left_dictionary.by_definition_key()?;
    let right_by_key = right_dictionary.by_definition_key()?;
    let left_aggregate = aggregate_by_function(left);
    let right_aggregate = aggregate_by_function(right);
    let keys = left_by_key
        .keys()
        .chain(right_by_key.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::new();
    for key in keys {
        let left_identity = left_by_key.get(key).copied();
        let right_identity = right_by_key.get(key).copied();
        let left_counters = left_identity
            .and_then(|identity| left_aggregate.get(&identity.function_id).copied())
            .unwrap_or_default();
        let right_counters = right_identity
            .and_then(|identity| right_aggregate.get(&identity.function_id).copied())
            .unwrap_or_default();
        // Dictionary entries not exercised by either side add no signal.
        if left_counters == Counters::default() && right_counters == Counters::default() {
            continue;
        }
        let presence = match (left_identity, right_identity) {
            (Some(_), Some(_)) => DiffPresence::Both,
            (None, Some(_)) => DiffPresence::Added,
            (Some(_), None) => DiffPresence::Removed,
            (None, None) => unreachable!("key came from at least one dictionary"),
        };
        let fqn = right_identity
            .or(left_identity)
            .map_or_else(|| key.to_owned(), |identity| identity.fqn.clone());
        rows.push(DiffRow {
            definition_key: key.to_owned(),
            fqn,
            left_function_id: left_identity.map(|identity| identity.function_id),
            right_function_id: right_identity.map(|identity| identity.function_id),
            presence,
            definition_changed: left_identity
                .zip(right_identity)
                .is_some_and(|(old, new)| old.def_content_hash != new.def_content_hash),
            left: left_counters,
            right: right_counters,
            delta: SignedCounters {
                calls: signed_delta(right_counters.enters, left_counters.enters),
                errors: signed_delta(right_counters.errors(), left_counters.errors()),
                total_ns: signed_delta(right_counters.total_ns, left_counters.total_ns),
                self_ns: signed_delta(right_counters.self_ns, left_counters.self_ns),
                awaiting_ns: signed_delta(right_counters.await_ns, left_counters.await_ns),
            },
        });
    }
    rows.sort_by_key(|row| {
        (
            std::cmp::Reverse(row.delta.total_ns.unsigned_abs()),
            std::cmp::Reverse(row.delta.errors.unsigned_abs()),
            row.definition_key.clone(),
        )
    });
    let row_budget = request.max_rows.min(byte_row_budget(request.max_bytes));
    let mut meta = merge_meta(&left.meta, &right.meta);
    if rows.len() > row_budget {
        rows.truncate(row_budget);
        meta.truncated = true;
        meta.warnings
            .push("diff was truncated by its bounded-size contract".to_owned());
    }
    meta.finalize();
    Ok(DiffResponse { rows, meta })
}

impl<S: ByteSource> QueryEngine<S> {
    pub fn sandwich(
        &self,
        files: &[crate::FileId],
        partition_id: Option<u32>,
        request: SandwichRequest,
    ) -> Result<QueryPoll<SandwichResponse>, QueryError> {
        match self.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => Ok(QueryPoll::Ready(sandwich(&cct, request)?)),
            QueryPoll::NeedData { ranges } => Ok(QueryPoll::NeedData { ranges }),
        }
    }

    pub fn search(
        &self,
        files: &[crate::FileId],
        partition_id: Option<u32>,
        dictionary: &FunctionDictionary,
        request: &SearchRequest,
    ) -> Result<QueryPoll<SearchResponse>, QueryError> {
        match self.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => Ok(QueryPoll::Ready(search_functions(
                &cct, dictionary, request,
            )?)),
            QueryPoll::NeedData { ranges } => Ok(QueryPoll::NeedData { ranges }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn diff(
        &self,
        left_files: &[crate::FileId],
        left_partition: Option<u32>,
        left_dictionary: &FunctionDictionary,
        right_files: &[crate::FileId],
        right_partition: Option<u32>,
        right_dictionary: &FunctionDictionary,
        request: DiffRequest,
    ) -> Result<QueryPoll<DiffResponse>, QueryError> {
        let left = self.open_run(left_files, left_partition)?;
        let right = self.open_run(right_files, right_partition)?;
        match (left, right) {
            (QueryPoll::Ready(left), QueryPoll::Ready(right)) => Ok(QueryPoll::Ready(diff_cct(
                &left,
                left_dictionary,
                &right,
                right_dictionary,
                request,
            )?)),
            (left, right) => {
                let mut ranges = Vec::new();
                if let QueryPoll::NeedData {
                    ranges: left_ranges,
                } = left
                {
                    ranges.extend(left_ranges);
                }
                if let QueryPoll::NeedData {
                    ranges: right_ranges,
                } = right
                {
                    ranges.extend(right_ranges);
                }
                ranges.sort_by_key(|range| (range.file, range.start, range.end));
                ranges.dedup();
                Ok(QueryPoll::NeedData { ranges })
            }
        }
    }
}

fn aggregate_by_function(cct: &FoldedCct) -> BTreeMap<u32, Counters> {
    let mut output = BTreeMap::<u32, Counters>::new();
    for node in cct.nodes.values() {
        add_counters(output.entry(node.function_id).or_default(), node.counters);
    }
    output
}

fn merge_meta(left: &Completeness, right: &Completeness) -> Completeness {
    let mut meta = left.clone();
    meta.complete &= right.complete;
    meta.watermarks.extend_from_slice(&right.watermarks);
    meta.capture_loss.extend(right.capture_loss.clone());
    meta.sources_consulted
        .extend_from_slice(&right.sources_consulted);
    meta.truncated |= right.truncated;
    meta.lod_degraded |= right.lod_degraded;
    meta.partial_tail |= right.partial_tail;
    meta.more_lanes |= right.more_lanes;
    meta.warnings.extend(right.warnings.clone());
    meta.snapshot.extend_from_slice(&right.snapshot);
    meta
}

fn validate_advanced_budget(max_rows: usize, max_bytes: usize) -> Result<(), QueryError> {
    if max_rows == 0 || max_rows > MAX_ADVANCED_ROWS {
        return Err(QueryError::invalid_request(format!(
            "max_rows must be in 1..={MAX_ADVANCED_ROWS}"
        )));
    }
    if !(ADVANCED_RESPONSE_OVERHEAD..=HARD_MAX_BYTES).contains(&max_bytes) {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must be in {ADVANCED_RESPONSE_OVERHEAD}..={HARD_MAX_BYTES}"
        )));
    }
    Ok(())
}

fn byte_row_budget(max_bytes: usize) -> usize {
    max_bytes
        .saturating_sub(ADVANCED_RESPONSE_OVERHEAD)
        .checked_div(ADVANCED_ROW_BYTES)
        .unwrap_or(0)
        .max(1)
}

fn signed_delta(right: u64, left: u64) -> i64 {
    let delta = i128::from(right) - i128::from(left);
    delta.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
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

fn push_counter_columns<T>(
    builder: &mut BqfBuilder,
    rows: &[T],
    counters: impl Fn(&T) -> Counters,
) -> Result<(), QueryError> {
    push_counter_columns_with_base(builder, rows, 10, counters)
}

fn push_counter_columns_with_base<T>(
    builder: &mut BqfBuilder,
    rows: &[T],
    base: u16,
    counters: impl Fn(&T) -> Counters,
) -> Result<(), QueryError> {
    builder.push(Column::U64 {
        id: base,
        values: rows.iter().map(|row| counters(row).enters).collect(),
    })?;
    builder.push(Column::U64 {
        id: base + 1,
        values: rows.iter().map(|row| counters(row).errors()).collect(),
    })?;
    builder.push(Column::U64 {
        id: base + 2,
        values: rows.iter().map(|row| counters(row).total_ns).collect(),
    })?;
    builder.push(Column::U64 {
        id: base + 3,
        values: rows.iter().map(|row| counters(row).self_ns).collect(),
    })?;
    builder.push(Column::U64 {
        id: base + 4,
        values: rows.iter().map(|row| counters(row).await_ns).collect(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::FoldedNode;

    use super::*;

    fn node(node_id: u32, parent: u32, function_id: u32, total_ns: u64) -> FoldedNode {
        FoldedNode {
            node_id,
            parent_node_id: parent,
            function_id,
            counters: Counters {
                enters: 1,
                total_ns,
                self_ns: total_ns,
                ..Counters::default()
            },
            ..FoldedNode::default()
        }
    }

    fn dictionary(hash: u8) -> FunctionDictionary {
        FunctionDictionary {
            functions: vec![
                FunctionIdentity {
                    function_id: 10,
                    definition_key: "fn:root".to_owned(),
                    fqn: "app.root".to_owned(),
                    def_content_hash: [hash; 32],
                },
                FunctionIdentity {
                    function_id: 20,
                    definition_key: "fn:work".to_owned(),
                    fqn: "app.work".to_owned(),
                    def_content_hash: [hash; 32],
                },
                FunctionIdentity {
                    function_id: 30,
                    definition_key: "fn:leaf".to_owned(),
                    fqn: "app.leaf".to_owned(),
                    def_content_hash: [hash; 32],
                },
            ],
        }
    }

    #[test]
    fn sandwich_groups_callers_and_callees_by_depth() {
        let mut cct = FoldedCct::default();
        cct.nodes.insert(1, node(1, 0, 10, 100));
        cct.nodes.insert(2, node(2, 1, 20, 80));
        cct.nodes.insert(3, node(3, 2, 30, 50));
        let response = sandwich(
            &cct,
            SandwichRequest {
                function_id: 20,
                max_bytes: 4096,
                ..SandwichRequest::default()
            },
        )
        .unwrap();
        assert!(
            response
                .rows
                .iter()
                .any(|row| { row.direction == SandwichDirection::Caller && row.function_id == 10 })
        );
        assert!(
            response
                .rows
                .iter()
                .any(|row| { row.direction == SandwichDirection::Callee && row.function_id == 30 })
        );
        let frame = response.to_bqf(9, 4096).unwrap();
        assert_eq!(frame.header().unwrap().kind, FrameKind::Sandwich);
    }

    #[test]
    fn search_uses_dictionary_and_diff_aligns_by_definition_key() {
        let mut left = FoldedCct::default();
        left.nodes.insert(1, node(1, 0, 10, 100));
        left.nodes.insert(2, node(2, 1, 20, 50));
        let search = search_functions(
            &left,
            &dictionary(1),
            &SearchRequest {
                text: "work".to_owned(),
                max_rows: 10,
                max_bytes: 4096,
            },
        )
        .unwrap();
        assert_eq!(search.rows[0].function.definition_key, "fn:work");

        let mut right = FoldedCct::default();
        right.nodes.insert(9, node(9, 0, 20, 90));
        let diff = diff_cct(
            &left,
            &dictionary(1),
            &right,
            &dictionary(2),
            DiffRequest {
                max_rows: 10,
                max_bytes: 4096,
            },
        )
        .unwrap();
        let work = diff
            .rows
            .iter()
            .find(|row| row.definition_key == "fn:work")
            .unwrap();
        assert_eq!(work.delta.total_ns, 40);
        assert!(work.definition_changed);
        let frame = diff.to_bqf(10, 4096).unwrap();
        assert_eq!(frame.header().unwrap().kind, FrameKind::Diff);
    }
}
