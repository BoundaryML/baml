use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    ByteRange, ByteSource, ByteView, FileId, QueryError, RangeCacheSource, SourceSnapshot,
    WASM_CACHE_BYTES,
};

/// Static-file metadata supplied by an extension host or an HTTP manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpFile {
    pub file: FileId,
    pub url: String,
    pub committed_len: u64,
    pub generation: u64,
    /// Validator sent back as `If-Range`; a changed validator must be
    /// registered as a new generation before bytes are accepted.
    pub validator: Option<String>,
}

/// One inclusive HTTP Range request planned from a sans-I/O `NeedData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRangeRequest {
    pub file: FileId,
    pub url: String,
    pub start: u64,
    pub end_exclusive: u64,
    pub range_header: String,
    pub if_range: Option<String>,
    pub generation: u64,
}

/// Host response to a planned range. Bodies are accepted only when they
/// exactly cover the requested range and still match its generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRangeResponse {
    pub file: FileId,
    pub generation: u64,
    pub start: u64,
    pub end_exclusive: u64,
    pub total_len: u64,
    pub validator: Option<String>,
    pub body: Vec<u8>,
}

/// Byte-budgeted CacheSource plus deterministic HTTP Range planning.
///
/// This type performs no I/O. Browser `fetch`, an extension `readRange`
/// bridge, and native HTTP clients all consume the same request records and
/// feed validated responses back through [`Self::accept`].
#[derive(Debug)]
pub struct HttpRangeSource {
    cache: RangeCacheSource,
    files: std::sync::Mutex<BTreeMap<FileId, HttpFile>>,
    max_request_bytes: u64,
    cache_bytes: usize,
}

impl Default for HttpRangeSource {
    fn default() -> Self {
        Self::new(WASM_CACHE_BYTES, 1024 * 1024)
    }
}

impl HttpRangeSource {
    #[must_use]
    pub fn new(cache_bytes: usize, max_request_bytes: u64) -> Self {
        Self {
            cache: RangeCacheSource::new(cache_bytes),
            files: std::sync::Mutex::new(BTreeMap::new()),
            max_request_bytes: max_request_bytes.max(1),
            cache_bytes,
        }
    }

    pub fn register(&self, file: HttpFile) -> Result<(), QueryError> {
        if file.url.is_empty() {
            return Err(QueryError::invalid_request(
                "HTTP Range file URL must not be empty",
            ));
        }
        let snapshot = SourceSnapshot {
            committed_len: file.committed_len,
            generation: file.generation,
        };
        self.cache.set_snapshot(file.file, snapshot);
        self.files
            .lock()
            .expect("HTTP range file registry mutex poisoned")
            .insert(file.file, file);
        Ok(())
    }

    pub fn unregister(&self, file: FileId) {
        self.files
            .lock()
            .expect("HTTP range file registry mutex poisoned")
            .remove(&file);
        self.cache.set_snapshot(file, SourceSnapshot::default());
    }

    /// Coalesces overlapping/adjacent needs, then splits them to a bounded
    /// request size. Results are stable by `(file,start,end)`.
    pub fn plan(&self, ranges: &[ByteRange]) -> Result<Vec<HttpRangeRequest>, QueryError> {
        let files = self
            .files
            .lock()
            .expect("HTTP range file registry mutex poisoned");
        let mut grouped = BTreeMap::<FileId, Vec<(u64, u64)>>::new();
        for range in ranges {
            let metadata = files.get(&range.file).ok_or_else(|| {
                QueryError::NotFound(format!("HTTP metadata for file {}", range.file.0))
            })?;
            if range.end > metadata.committed_len {
                return Err(QueryError::invalid_request(format!(
                    "requested byte {} exceeds committed length {} for file {}",
                    range.end, metadata.committed_len, range.file.0
                )));
            }
            if !range.is_empty() {
                grouped
                    .entry(range.file)
                    .or_default()
                    .push((range.start, range.end));
            }
        }

        let mut output = Vec::new();
        for (file, mut needs) in grouped {
            needs.sort_unstable();
            let metadata = &files[&file];
            let mut merged = Vec::<(u64, u64)>::new();
            for (start, end) in needs {
                if let Some(last) = merged.last_mut()
                    && start <= last.1
                {
                    last.1 = last.1.max(end);
                    continue;
                }
                merged.push((start, end));
            }
            for (start, end) in merged {
                let mut cursor = start;
                while cursor < end {
                    let chunk_end = cursor.saturating_add(self.max_request_bytes).min(end);
                    output.push(HttpRangeRequest {
                        file,
                        url: metadata.url.clone(),
                        start: cursor,
                        end_exclusive: chunk_end,
                        range_header: format!("bytes={cursor}-{}", chunk_end - 1),
                        if_range: metadata.validator.clone(),
                        generation: metadata.generation,
                    });
                    cursor = chunk_end;
                }
            }
        }
        Ok(output)
    }

    pub fn accept(&self, response: HttpRangeResponse) -> Result<(), QueryError> {
        let files = self
            .files
            .lock()
            .expect("HTTP range file registry mutex poisoned");
        let metadata = files.get(&response.file).ok_or_else(|| {
            QueryError::NotFound(format!("HTTP metadata for file {}", response.file.0))
        })?;
        if response.generation != metadata.generation {
            return Err(QueryError::invalid_data(
                "stale HTTP Range response generation",
            ));
        }
        if response.total_len != metadata.committed_len {
            return Err(QueryError::invalid_data(
                "HTTP Content-Range total differs from committed length",
            ));
        }
        if response.validator != metadata.validator {
            return Err(QueryError::invalid_data(
                "HTTP Range response validator changed without a generation bump",
            ));
        }
        if response.end_exclusive < response.start
            || response.end_exclusive > response.total_len
            || response.end_exclusive - response.start
                != u64::try_from(response.body.len()).unwrap_or(u64::MAX)
        {
            return Err(QueryError::invalid_data(
                "HTTP Range response body does not exactly cover Content-Range",
            ));
        }
        let inserted = self.cache.insert(
            response.file,
            response.generation,
            response.start,
            Arc::<[u8]>::from(response.body),
        );
        if !inserted {
            return Err(QueryError::BudgetExceeded {
                required: usize::try_from(response.end_exclusive - response.start)
                    .unwrap_or(usize::MAX),
                max_bytes: self.cache_retained_limit(),
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.cache.retained_bytes()
    }

    fn cache_retained_limit(&self) -> usize {
        self.cache_bytes
    }

    #[must_use]
    pub fn registered_files(&self) -> BTreeSet<FileId> {
        self.files
            .lock()
            .expect("HTTP range file registry mutex poisoned")
            .keys()
            .copied()
            .collect()
    }
}

impl ByteSource for HttpRangeSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.cache.committed_len(file)
    }

    fn generation(&self, file: FileId) -> u64 {
        self.cache.generation(file)
    }

    fn view(&self, range: &ByteRange) -> Option<ByteView> {
        self.cache.view(range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_splits_and_validates_range_responses() {
        let source = HttpRangeSource::new(64, 8);
        let file = FileId(7);
        source
            .register(HttpFile {
                file,
                url: "https://example.test/seg.bamlseg".to_owned(),
                committed_len: 32,
                generation: 3,
                validator: Some("\"abc\"".to_owned()),
            })
            .unwrap();
        let requests = source
            .plan(&[
                ByteRange::new(file, 0, 5).unwrap(),
                ByteRange::new(file, 4, 14).unwrap(),
            ])
            .unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].range_header, "bytes=0-7");
        assert_eq!(requests[1].range_header, "bytes=8-13");

        source
            .accept(HttpRangeResponse {
                file,
                generation: 3,
                start: 0,
                end_exclusive: 8,
                total_len: 32,
                validator: Some("\"abc\"".to_owned()),
                body: (0_u8..8).collect(),
            })
            .unwrap();
        assert_eq!(
            source
                .view(&ByteRange::new(file, 2, 6).unwrap())
                .unwrap()
                .as_ref(),
            &[2, 3, 4, 5]
        );
    }

    #[test]
    fn stale_or_short_responses_fail_closed() {
        let source = HttpRangeSource::new(64, 64);
        let file = FileId(8);
        source
            .register(HttpFile {
                file,
                url: "https://example.test/file".to_owned(),
                committed_len: 10,
                generation: 2,
                validator: None,
            })
            .unwrap();
        let error = source
            .accept(HttpRangeResponse {
                file,
                generation: 1,
                start: 0,
                end_exclusive: 4,
                total_len: 10,
                validator: None,
                body: vec![0; 4],
            })
            .unwrap_err();
        assert!(error.to_string().contains("stale"));
    }
}
