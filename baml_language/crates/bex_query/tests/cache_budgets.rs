#![cfg(any(feature = "native", feature = "wasm"))]

use bex_query::{MemorySource, QueryEngine};

#[cfg(all(feature = "native", not(feature = "wasm")))]
use bex_query::NATIVE_CACHE_BYTES;
#[cfg(feature = "wasm")]
use bex_query::{
    ByteRange, ByteSource, FileId, HttpFile, HttpRangeResponse, HttpRangeSource, WASM_CACHE_BYTES,
};

#[cfg(all(feature = "native", not(feature = "wasm")))]
#[test]
fn native_query_engine_defaults_to_256_mib_cache() {
    let engine = QueryEngine::new(MemorySource::new());
    assert_eq!(NATIVE_CACHE_BYTES, 256 * 1024 * 1024);
    assert_eq!(engine.cache_max_bytes(), NATIVE_CACHE_BYTES);
}

#[cfg(feature = "wasm")]
#[test]
fn wasm_query_engine_defaults_to_32_mib_cache() {
    let engine = QueryEngine::new(MemorySource::new());
    assert_eq!(WASM_CACHE_BYTES, 32 * 1024 * 1024);
    assert_eq!(engine.cache_max_bytes(), WASM_CACHE_BYTES);
}

#[cfg(feature = "wasm")]
#[test]
fn wasm_http_range_source_evicts_at_its_byte_budget() {
    let source = HttpRangeSource::default();
    let file = FileId(7);
    let half_budget = WASM_CACHE_BYTES / 2;
    let total_len = u64::try_from(WASM_CACHE_BYTES + 1).unwrap();
    source
        .register(HttpFile {
            file,
            url: "https://example.test/session.bamlseg".to_owned(),
            committed_len: total_len,
            generation: 1,
            validator: None,
        })
        .unwrap();

    source
        .accept(response(file, 0, half_budget, total_len))
        .unwrap();
    source
        .accept(response(
            file,
            u64::try_from(half_budget).unwrap(),
            half_budget,
            total_len,
        ))
        .unwrap();
    assert_eq!(source.retained_bytes(), WASM_CACHE_BYTES);

    source
        .accept(response(
            file,
            u64::try_from(WASM_CACHE_BYTES).unwrap(),
            1,
            total_len,
        ))
        .unwrap();
    assert!(source.retained_bytes() <= WASM_CACHE_BYTES);

    assert!(source.view(&ByteRange::new(file, 0, 1).unwrap()).is_none());
    let second_half = u64::try_from(half_budget).unwrap();
    assert!(
        source
            .view(&ByteRange::new(file, second_half, second_half + 1).unwrap())
            .is_some()
    );
    let final_byte = u64::try_from(WASM_CACHE_BYTES).unwrap();
    assert!(
        source
            .view(&ByteRange::new(file, final_byte, final_byte + 1).unwrap())
            .is_some()
    );
}

#[cfg(feature = "wasm")]
fn response(file: FileId, start: u64, len: usize, total_len: u64) -> HttpRangeResponse {
    HttpRangeResponse {
        file,
        generation: 1,
        start,
        end_exclusive: start + u64::try_from(len).unwrap(),
        total_len,
        validator: None,
        body: vec![0; len],
    }
}
