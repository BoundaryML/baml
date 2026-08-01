//! WASM-friendly ObserveEngine facade.
//!
//! The facade owns no runtime and performs no I/O. Every method either returns
//! a transferable BQF1 buffer or exact HTTP/readRange requests for its host.

use std::sync::Arc;

use crate::{
    BqfFrame, ByteRange, DiffRequest, FileId, FunctionDictionary, HttpFile, HttpRangeRequest,
    HttpRangeResponse, HttpRangeSource, LeftHeavyRequest, QueryEngine, QueryError, QueryPoll,
    SandwichRequest, SearchRequest, Viewport, WASM_CACHE_BYTES, diff_cct, sandwich,
    search_functions,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservePoll {
    Frame(Vec<u8>),
    NeedData { requests: Vec<HttpRangeRequest> },
}

impl ObservePoll {
    fn frame(frame: BqfFrame) -> Self {
        Self::Frame(frame.into_bytes())
    }
}

pub struct ObserveEngine {
    engine: QueryEngine<HttpRangeSource>,
}

impl Default for ObserveEngine {
    fn default() -> Self {
        Self::new(WASM_CACHE_BYTES, 1024 * 1024)
    }
}

impl ObserveEngine {
    #[must_use]
    pub fn new(cache_bytes: usize, max_range_bytes: u64) -> Self {
        Self {
            engine: QueryEngine::with_cache_budget(
                HttpRangeSource::new(cache_bytes, max_range_bytes),
                cache_bytes,
            ),
        }
    }

    pub fn register_file(&self, file: HttpFile) -> Result<(), QueryError> {
        self.engine.source().register(file)
    }

    pub fn unregister_file(&self, file: FileId) {
        self.engine.source().unregister(file);
    }

    pub fn supply_range(&self, response: HttpRangeResponse) -> Result<(), QueryError> {
        self.engine.source().accept(response)
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.engine
            .source()
            .retained_bytes()
            .saturating_add(self.engine.cache_retained_bytes())
    }

    pub fn timeline(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        viewport: Viewport,
        request_id: u64,
    ) -> Result<ObservePoll, QueryError> {
        match self.engine.timeline(files, partition_id, viewport)? {
            QueryPoll::Ready(response) => Ok(ObservePoll::frame(
                response.to_bqf(request_id, viewport.max_bytes)?,
            )),
            QueryPoll::NeedData { ranges } => self.needs(&ranges),
        }
    }

    pub fn left_heavy(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        request: LeftHeavyRequest,
        request_id: u64,
    ) -> Result<ObservePoll, QueryError> {
        match self.engine.left_heavy(files, partition_id, request)? {
            QueryPoll::Ready(response) => Ok(ObservePoll::frame(
                response.to_bqf(request_id, request.max_bytes)?,
            )),
            QueryPoll::NeedData { ranges } => self.needs(&ranges),
        }
    }

    pub fn sandwich(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        request: SandwichRequest,
        request_id: u64,
    ) -> Result<ObservePoll, QueryError> {
        match self.engine.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => {
                let response = sandwich(&cct, request)?;
                Ok(ObservePoll::frame(
                    response.to_bqf(request_id, request.max_bytes)?,
                ))
            }
            QueryPoll::NeedData { ranges } => self.needs(&ranges),
        }
    }

    pub fn search(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
        dictionary: &FunctionDictionary,
        request: &SearchRequest,
        request_id: u64,
    ) -> Result<ObservePoll, QueryError> {
        match self.engine.open_run(files, partition_id)? {
            QueryPoll::Ready(cct) => {
                let response = search_functions(&cct, dictionary, request)?;
                Ok(ObservePoll::frame(
                    response.to_bqf(request_id, request.max_bytes)?,
                ))
            }
            QueryPoll::NeedData { ranges } => self.needs(&ranges),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn diff(
        &self,
        left_files: &[FileId],
        left_partition: Option<u32>,
        left_dictionary: &FunctionDictionary,
        right_files: &[FileId],
        right_partition: Option<u32>,
        right_dictionary: &FunctionDictionary,
        request: DiffRequest,
        request_id: u64,
    ) -> Result<ObservePoll, QueryError> {
        let left = self.engine.open_run(left_files, left_partition)?;
        let right = self.engine.open_run(right_files, right_partition)?;
        match (left, right) {
            (QueryPoll::Ready(left), QueryPoll::Ready(right)) => {
                let response = diff_cct(&left, left_dictionary, &right, right_dictionary, request)?;
                Ok(ObservePoll::frame(
                    response.to_bqf(request_id, request.max_bytes)?,
                ))
            }
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
                self.needs(&ranges)
            }
        }
    }

    /// Parse/typecheck BQL in wasm without issuing I/O. Execution uses the
    /// named methods above until the query references only resident sources.
    pub fn explain_bql(&self, source: &str) -> Result<crate::bql::ScriptPlan, QueryError> {
        crate::bql::parse_and_plan(source, &std::collections::BTreeMap::new()).map(|(_, plan)| plan)
    }

    fn needs(&self, ranges: &[ByteRange]) -> Result<ObservePoll, QueryError> {
        Ok(ObservePoll::NeedData {
            requests: self.engine.source().plan(ranges)?,
        })
    }

    #[must_use]
    pub fn source(&self) -> &HttpRangeSource {
        self.engine.source()
    }

    pub fn open_run(
        &self,
        files: &[FileId],
        partition_id: Option<u32>,
    ) -> Result<QueryPoll<Arc<crate::FoldedCct>>, QueryError> {
        self.engine.open_run(files, partition_id)
    }
}

#[cfg(test)]
mod tests {
    use bex_events::prof::storage::{BcctHeader, BcctWriter, ClockDescriptor};

    use super::*;

    #[test]
    fn observe_engine_requests_ranges_then_returns_bqf() {
        let header = BcctHeader {
            process_euid: [1; 16],
            engine_id: 1,
            session_seg_seq: 1,
            started_epoch_ns: 1,
            clock: ClockDescriptor {
                kind: 1,
                quality: 1,
                tick_ns_numer: 1,
                tick_ns_denom: 1,
            },
            revision_id: [2; 32],
        };
        let mut writer = BcctWriter::create(Vec::new(), &header).unwrap();
        writer.seal().unwrap();
        let bytes = writer.into_inner();
        let engine = ObserveEngine::new(64 * 1024, 64 * 1024);
        let file = FileId(11);
        engine
            .register_file(HttpFile {
                file,
                url: "https://example.test/seg".to_owned(),
                committed_len: bytes.len() as u64,
                generation: 1,
                validator: None,
            })
            .unwrap();
        let frame = loop {
            match engine
                .left_heavy(
                    &[file],
                    None,
                    LeftHeavyRequest {
                        pixel_width: 10,
                        max_bytes: 4096,
                    },
                    7,
                )
                .unwrap()
            {
                ObservePoll::Frame(frame) => break frame,
                ObservePoll::NeedData { requests } => {
                    for request in requests {
                        let start = request.start as usize;
                        let end = request.end_exclusive as usize;
                        engine
                            .supply_range(HttpRangeResponse {
                                file,
                                generation: 1,
                                start: request.start,
                                end_exclusive: request.end_exclusive,
                                total_len: bytes.len() as u64,
                                validator: None,
                                body: bytes[start..end].to_vec(),
                            })
                            .unwrap();
                    }
                }
            }
        };
        assert_eq!(&frame[..4], b"BQF1");
        let ready = engine
            .left_heavy(
                &[file],
                None,
                LeftHeavyRequest {
                    pixel_width: 10,
                    max_bytes: 4096,
                },
                7,
            )
            .unwrap();
        let ObservePoll::Frame(cached) = ready else {
            panic!("resident retry must be ready");
        };
        assert_eq!(cached, frame);
    }
}
