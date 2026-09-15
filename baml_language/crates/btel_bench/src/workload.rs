//! Finite fixtures: construction, wire validation, and disk loading happen before timing.
use std::{fs, path::Path};

use btel_core::{
    ids::{BexCallId, BexThreadId, FunctionId},
    marker::{self, FunctionEndStatus, Marker, ThreadEndStatus},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub source_id: u64,
    pub calls: u64,
    pub depth: usize,
    pub markers: u64,
    pub bytes: u64,
}

pub struct Fixture {
    pub manifest: Manifest,
    pub bytes: Vec<u8>,
    /// Record lengths are preparation metadata; replay does not decode markers.
    pub lengths: Vec<u16>,
}

impl Fixture {
    pub fn prepare(
        source_id: u64,
        calls: u64,
        depth: usize,
        directory: Option<&Path>,
    ) -> Result<Self, String> {
        if depth == 0 {
            return Err("depth must be positive".into());
        }
        let bytes = calls
            .checked_mul((marker::FUNCTION_ENTER_LEN + marker::FUNCTION_EXIT_LEN) as u64)
            .and_then(|n| {
                n.checked_add((marker::START_THREAD_FIXED_LEN + marker::END_THREAD_LEN) as u64)
            })
            .ok_or("fixture byte count overflow")?;
        let markers = calls
            .checked_mul(2)
            .and_then(|n| n.checked_add(2))
            .ok_or("marker count overflow")?;
        let manifest = Manifest {
            version: 1,
            source_id,
            calls,
            depth,
            markers,
            bytes,
        };
        let paths = directory.map(|dir| {
            let base = format!("source-{source_id}-calls-{calls}-depth-{depth}-v1");
            (
                dir.join(format!("{base}.json")),
                dir.join(format!("{base}.markers")),
            )
        });
        if let Some((meta_path, byte_path)) = &paths {
            if meta_path.exists() || byte_path.exists() {
                let saved: Manifest =
                    serde_json::from_slice(&fs::read(meta_path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                if saved != manifest {
                    return Err("fixture manifest mismatch".into());
                }
                return Self::validated(manifest, fs::read(byte_path).map_err(|e| e.to_string())?);
            }
        }
        let capacity = usize::try_from(bytes).map_err(|_| "fixture exceeds address space")?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(capacity)
            .map_err(|e| e.to_string())?;
        let append = |record: Marker<'_>| {
            let mut scratch = [0u8; marker::MAX_RECORD_LEN];
            let len = record.encode(&mut scratch);
            encoded.extend_from_slice(&scratch[..len]);
        };
        for_each_template(&manifest, append);
        let fixture = Self::validated(manifest, encoded)?;
        if let Some((meta_path, byte_path)) = paths {
            fs::create_dir_all(meta_path.parent().ok_or("fixture parent missing")?)
                .map_err(|e| e.to_string())?;
            fs::write(byte_path, &fixture.bytes).map_err(|e| e.to_string())?;
            fs::write(
                meta_path,
                serde_json::to_vec_pretty(&fixture.manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(fixture)
    }

    pub fn validated(manifest: Manifest, bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() as u64 != manifest.bytes {
            return Err("fixture byte length mismatch".into());
        }
        let mut lengths = Vec::new();
        let mut stack = Vec::new();
        let mut calls = 0;
        let mut ended = false;
        for (index, raw) in marker::iter(&bytes).enumerate() {
            let expected_tick = u64::try_from(index).map_err(|_| "marker index overflow")?;
            let raw = raw.map_err(|e| format!("invalid marker: {e:?}"))?;
            let valid = match raw {
                Marker::BexThreadStart {
                    thread_id,
                    ts_ticks,
                    parent_thread_id,
                    parent_call_id,
                    ..
                } => {
                    lengths.is_empty()
                        && thread_id.0 == manifest.source_id
                        && ts_ticks == 0
                        && parent_thread_id.0 == 0
                        && parent_call_id.0 == 0
                }
                Marker::FunctionEnter {
                    thread_id,
                    call_id,
                    parent_call_id,
                    ts_ticks,
                    ..
                } if !ended && !lengths.is_empty() => {
                    calls += 1;
                    let parent = stack.last().copied().unwrap_or(0);
                    let valid = thread_id.0 == manifest.source_id
                        && call_id.0 == calls
                        && parent_call_id.0 == parent
                        && ts_ticks == expected_tick;
                    stack.push(call_id.0);
                    valid && stack.len() <= manifest.depth
                }
                Marker::FunctionExit {
                    thread_id,
                    call_id,
                    ts_ticks,
                    status,
                } if !ended => {
                    stack.pop() == Some(call_id.0)
                        && thread_id.0 == manifest.source_id
                        && ts_ticks == expected_tick
                        && status == FunctionEndStatus::Ok
                }
                Marker::BexThreadEnd {
                    thread_id,
                    ts_ticks,
                    status,
                } if !ended => {
                    ended = true;
                    stack.is_empty()
                        && calls == manifest.calls
                        && thread_id.0 == manifest.source_id
                        && ts_ticks == expected_tick
                        && status == ThreadEndStatus::Completed
                }
                _ => false,
            };
            if !valid {
                return Err(format!(
                    "fixture sequence invalid at marker {}",
                    lengths.len()
                ));
            }
            lengths.push(u16::try_from(raw.encoded_len()).map_err(|_| "marker length overflow")?);
        }
        if !ended || lengths.len() as u64 != manifest.markers {
            return Err("fixture completion mismatch".into());
        }
        Ok(Self {
            manifest,
            bytes,
            lengths,
        })
    }
}

/// Structural fields are prepared once; producer modes share this exact workload.
pub fn for_each_template(manifest: &Manifest, mut append: impl FnMut(Marker<'static>)) {
    let Manifest {
        source_id,
        calls,
        depth,
        ..
    } = *manifest;
    let thread_id = BexThreadId(source_id);
    append(Marker::BexThreadStart {
        flags: 0,
        thread_id,
        parent_thread_id: BexThreadId(0),
        parent_call_id: BexCallId(0),
        ts_ticks: 0,
        name: &[],
    });
    let mut ticks = 0;
    let mut first = 1;
    while first <= calls {
        let last = first.saturating_add(depth as u64 - 1).min(calls);
        for call in first..=last {
            ticks += 1;
            append(Marker::FunctionEnter {
                flags: 0,
                thread_id,
                call_id: BexCallId(call),
                parent_call_id: BexCallId(if call == first { 0 } else { call - 1 }),
                function_id: FunctionId(1),
                call_site: None,
                ts_ticks: ticks,
            });
        }
        for call in (first..=last).rev() {
            ticks += 1;
            append(Marker::FunctionExit {
                status: FunctionEndStatus::Ok,
                thread_id,
                call_id: BexCallId(call),
                ts_ticks: ticks,
            });
        }
        first = last + 1;
    }
    append(Marker::BexThreadEnd {
        status: ThreadEndStatus::Completed,
        thread_id,
        ts_ticks: ticks + 1,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_nested_fixture_round_trips_and_rejects_truncation() {
        let f = Fixture::prepare(7, 65, 8, None).unwrap();
        assert_eq!(f.manifest.bytes, 65 * 80 + 54);
        assert_eq!(f.manifest.markers, 132);
        let mut bad = f.bytes.clone();
        bad.pop();
        assert!(Fixture::validated(f.manifest.clone(), bad).is_err());
        let mut bad = f.bytes;
        bad[0] = 255;
        assert!(Fixture::validated(f.manifest, bad).is_err());
    }
}
