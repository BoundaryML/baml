//! Host adapters for portable profiler log evidence and value identities.

use std::{collections::HashMap, io};

use super::{HistoryValueBody, HistoryValueReadResult};
use crate::{
    prof::backend::{
        CodecVersion, EncodedEvidenceBatch, EvidenceFact, LogEvent, ValueCid, ValueState,
        decode_evidence_payload, encode_evidence_facts,
    },
    run::{
        DiagnosticSeverity, LogPayload, PayloadEvent, PayloadId, PayloadKind, RedactionMetadata,
        RunDiagnostic, SourceLocation,
    },
    value::{ValueCodec, ValueRef},
};

pub fn log_value_ref(body: &[u8]) -> ValueRef {
    value_state_ref(&encoded_value_state(body)).expect("encoded bytes have a value reference")
}

pub fn encoded_value_state(body: &[u8]) -> ValueState {
    let codec = CodecVersion(1);
    ValueState::Available {
        cid: ValueCid::for_encoded(codec, body),
        codec,
        encoded_bytes: body.len() as u64,
    }
}

pub fn value_state_ref(state: &ValueState) -> Option<ValueRef> {
    let ValueState::Available {
        cid,
        codec,
        encoded_bytes,
    } = state
    else {
        return None;
    };
    if codec.0 != 1 {
        return None;
    }
    let size = usize::try_from(*encoded_bytes).unwrap_or(usize::MAX);
    Some(ValueRef::available(
        format!("cas:{}", hex::encode(cid.0)),
        ValueCodec::BamlOutboundValue,
        size,
        size,
    ))
}

pub fn abandoned_log_diagnostic(count: u64) -> RunDiagnostic {
    RunDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: Some("logCaptureAbandoned".to_owned()),
        message: format!("{count} log capture(s) were abandoned before delivery"),
        payload_id: None,
    }
}

pub fn value_ref_cid(id: &str) -> Option<ValueCid> {
    let mut bytes = [0; 32];
    hex::decode_to_slice(id.strip_prefix("cas:")?, &mut bytes).ok()?;
    Some(ValueCid(bytes))
}

pub fn log_payload(event: &LogEvent, index: usize) -> PayloadEvent {
    PayloadEvent {
        id: PayloadId((index as u64).saturating_add(1)),
        timestamp_ms: event.timestamp_ms,
        kind: PayloadKind::Log(LogPayload {
            level: event.level.clone(),
            message: event
                .message_preview
                .clone()
                .unwrap_or_else(|| "captured log".into()),
            source: event.source.map(|source| SourceLocation {
                file_path: None,
                file_id: Some(u64::from(source.file_id)),
                line: source.line,
                column: event.source_column.unwrap_or(0),
                end_line: None,
                end_column: None,
                start_offset: Some(source.start_offset),
                end_offset: Some(source.end_offset),
            }),
            value_ref: value_state_ref(&event.data),
        }),
        redaction: RedactionMetadata::display_safe(),
        body: None,
    }
}

/// Browser retention uses the native evidence codec and CAS identity, without
/// acquiring a filesystem store or writing legacy value records.
#[derive(Debug, Default)]
pub struct MemoryLogHistory {
    evidence: Vec<EncodedEvidenceBatch>,
    values: HashMap<ValueCid, Vec<u8>>,
}

impl MemoryLogHistory {
    pub fn append(&mut self, mut event: LogEvent, body: Option<Vec<u8>>, context: Option<Vec<u8>>) {
        event.data = self.retain(event.data, body);
        event.context = self.retain(event.context, context);
        self.evidence
            .push(encode_evidence_facts(&[EvidenceFact::LogEvent(event)]));
    }

    fn retain(&mut self, state: ValueState, body: Option<Vec<u8>>) -> ValueState {
        match body {
            Some(body) => self.insert(body),
            None => match state {
                ValueState::Available { cid, .. } if !self.values.contains_key(&cid) => {
                    ValueState::Lost(crate::prof::backend::ValueLossReason::StoreUnavailable)
                }
                state => state,
            },
        }
    }

    fn insert(&mut self, body: Vec<u8>) -> ValueState {
        let state = encoded_value_state(&body);
        if let ValueState::Available { cid, .. } = state {
            self.values.entry(cid).or_insert(body);
        }
        state
    }

    pub fn replay(&self) -> io::Result<Vec<PayloadEvent>> {
        let mut logs = Vec::new();
        for batch in &self.evidence {
            let facts = decode_evidence_payload(&batch.payload, batch.record_count)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{err:?}")))?;
            logs.extend(facts.into_iter().filter_map(|fact| match fact {
                EvidenceFact::LogEvent(log) => Some(log),
                _ => None,
            }));
        }
        logs.sort_by_key(|log| log.timestamp_ms);
        Ok(logs
            .iter()
            .enumerate()
            .map(|(index, log)| log_payload(log, index))
            .collect())
    }

    pub fn read_value(&self, id: &str) -> HistoryValueReadResult {
        value_ref_cid(id)
            .and_then(|cid| self.values.get(&cid))
            .map_or(HistoryValueReadResult::Missing, |body| {
                HistoryValueReadResult::Available(HistoryValueBody {
                    codec: ValueCodec::BamlOutboundValue,
                    body: body.clone(),
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::BoundaryId,
        prof::{backend::ValueLossReason, record::CallSiteSourceSpan},
        value::{LiveValueBody, LiveValueCache, LiveValueLookup},
    };

    fn event(timestamp_ms: u64) -> LogEvent {
        LogEvent {
            call_ref: None,
            timestamp_ms,
            level: Some("info".into()),
            source: Some(CallSiteSourceSpan {
                file_id: 2,
                start_offset: 4,
                end_offset: 8,
                line: 3,
            }),
            source_column: Some(5),
            message_preview: Some("hello".into()),
            event_name: Some("named.event".into()),
            distinct_id: Some("distinct".into()),
            context: ValueState::Lost(ValueLossReason::StoreUnavailable),
            data: ValueState::Lost(ValueLossReason::StoreUnavailable),
        }
    }

    #[test]
    fn memory_history_uses_portable_evidence_and_shared_cas_identity() {
        let mut history = MemoryLogHistory::default();
        let body = b"\x1a\x05hello".to_vec();
        let context = b"\x1a\x03ctx".to_vec();
        history.append(event(20), Some(body.clone()), Some(context.clone()));
        history.append(event(10), Some(body.clone()), None);
        assert_eq!(
            history.values.len(),
            2,
            "identical bodies share one CAS entry"
        );
        let first = &history.evidence[0];
        assert_eq!(first.payload[8], 8, "log evidence uses tag 8");
        let decoded = decode_evidence_payload(&first.payload, first.record_count).unwrap();
        let EvidenceFact::LogEvent(log) = &decoded[0] else {
            panic!("expected log evidence");
        };
        assert_eq!(log.data, encoded_value_state(&body));
        assert_eq!(log.context, encoded_value_state(&context));
        let replayed = history.replay().unwrap();
        assert_eq!(
            replayed
                .iter()
                .map(|log| log.timestamp_ms)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
        let PayloadKind::Log(log) = &replayed[0].kind else {
            panic!("expected log payload");
        };
        assert_eq!(log.message, "hello");
        assert_eq!(log.source.as_ref().unwrap().column, 5);
        let value_ref = log.value_ref.as_ref().unwrap();
        assert_eq!(value_ref, &log_value_ref(&body));
        assert_eq!(
            history.read_value(&value_ref.id),
            HistoryValueReadResult::Available(HistoryValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body
            })
        );
        assert_eq!(
            history.read_value(&log_value_ref(&context).id),
            HistoryValueReadResult::Available(HistoryValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: context
            })
        );
        assert_eq!(
            history.read_value("cas:not-a-cid"),
            HistoryValueReadResult::Missing
        );
        assert_eq!(
            history.read_value(&log_value_ref(b"absent").id),
            HistoryValueReadResult::Missing
        );
    }

    #[test]
    fn live_only_values_need_no_history_writer() {
        let body = b"\x1a\x05hello".to_vec();
        let value_ref = log_value_ref(&body);
        let boundary_id = BoundaryId::from_bytes([1; 16]);
        let mut cache = LiveValueCache::with_max_bytes(1024);
        cache.insert(
            boundary_id,
            &value_ref,
            LiveValueBody {
                codec: value_ref.codec,
                body: body.clone(),
            },
        );
        assert!(matches!(
            cache.get(boundary_id, &value_ref.id),
            LiveValueLookup::Available(_)
        ));
        assert_eq!(
            value_ref_cid(&value_ref.id),
            Some(ValueCid::for_encoded(CodecVersion(1), &body))
        );
    }

    #[test]
    fn missing_evidence_value_does_not_claim_available_bytes() {
        assert!(value_state_ref(&event(0).data).is_none());
        let mut history = MemoryLogHistory::default();
        history.append(event(0), Some(vec![0]), None);
        history.evidence[0].payload.pop();
        assert_eq!(
            history.replay().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn lost_log_data_preserves_evidence_and_retained_context() {
        let mut history = MemoryLogHistory::default();
        let mut log = event(4);
        log.data = ValueState::Lost(ValueLossReason::ValueTooLarge);
        history.append(log.clone(), None, Some(vec![1, 2, 3]));
        let replay = history.replay().unwrap();
        let PayloadKind::Log(payload) = &replay[0].kind else {
            panic!("log expected")
        };
        assert_eq!(payload.message, "hello");
        assert_eq!(payload.source.as_ref().unwrap().column, 5);
        assert!(payload.value_ref.is_none());
        let batch = &history.evidence[0];
        let facts = decode_evidence_payload(&batch.payload, batch.record_count).unwrap();
        let EvidenceFact::LogEvent(retained) = &facts[0] else {
            panic!("log expected")
        };
        assert_eq!(retained.data, log.data);
        assert!(matches!(
            history.read_value(&log_value_ref(&[1, 2, 3]).id),
            HistoryValueReadResult::Available(_)
        ));
    }
}
