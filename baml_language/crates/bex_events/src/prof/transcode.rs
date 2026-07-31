//! Shared raw-record to `.bamlprof` event transcode helpers.

use crate::prof::{
    clock::TickConverter,
    pb,
    record::{FunctionEndStatus, RawRecord, SuspendReason, ThreadEndStatus},
};

/// Transcoding converts raw ticks to nanoseconds (`conv`); everything else
/// is a pure encoding change. The event content stays identical to the ring
/// record, with the `0 = none` conventions becoming `Option`s.
#[must_use]
pub fn to_disk_event(raw: &RawRecord<'_>, conv: &TickConverter) -> pb::DiskEventV1 {
    use pb::disk_event_v1::Event;
    let event = match *raw {
        RawRecord::CallFunction {
            flags: _,
            thread_id,
            call_id,
            parent_call_id,
            function_id,
            call_site,
            ts_ticks,
        } => Event::CallFunction(pb::CallFunction {
            thread_id: thread_id.0,
            call_id: call_id.0,
            parent_call_id: (parent_call_id.0 != 0).then_some(parent_call_id.0),
            function_id: function_id.0,
            timestamp_ns: conv.to_ns(ts_ticks),
            call_site_file_id: call_site.map(|span| span.file_id),
            call_site_start_offset: call_site.map(|span| span.start_offset),
            call_site_end_offset: call_site.map(|span| span.end_offset),
            call_site_line: call_site.map(|span| span.line),
        }),
        RawRecord::EndFunction {
            status,
            thread_id,
            call_id,
            ts_ticks,
        } => Event::EndFunction(pb::EndFunction {
            thread_id: thread_id.0,
            call_id: call_id.0,
            status: match status {
                FunctionEndStatus::Ok => pb::FunctionEndStatus::Ok,
                FunctionEndStatus::Errored => pb::FunctionEndStatus::Errored,
                FunctionEndStatus::Cancelled => pb::FunctionEndStatus::Cancelled,
                FunctionEndStatus::Exited => pb::FunctionEndStatus::Exited,
            } as i32,
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::StartThread {
            flags: _,
            thread_id,
            parent_thread_id,
            parent_call_id,
            ts_ticks,
            name,
        } => Event::StartThread(pb::StartThread {
            thread_id: thread_id.0,
            parent_thread_id: (parent_thread_id.0 != 0).then_some(parent_thread_id.0),
            parent_call_id: (parent_call_id.0 != 0).then_some(parent_call_id.0),
            name: (!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()),
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::EndThread {
            status,
            thread_id,
            ts_ticks,
        } => Event::EndThread(pb::EndThread {
            thread_id: thread_id.0,
            status: match status {
                ThreadEndStatus::Completed => pb::ThreadEndStatus::Completed,
                ThreadEndStatus::Cancelled => pb::ThreadEndStatus::Cancelled,
                ThreadEndStatus::Errored => pb::ThreadEndStatus::Errored,
            } as i32,
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::SetFunctionId {
            thread_id,
            call_id,
            id,
            ts_ticks,
        } => Event::SetFunctionId(pb::SetFunctionId {
            thread_id: thread_id.0,
            call_id: call_id.0,
            id: id.to_vec(),
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::SuspendThread {
            reason,
            thread_id,
            suspend_seq,
            ts_ticks,
        } => Event::SuspendThread(pb::SuspendThread {
            thread_id: thread_id.0,
            suspend_seq,
            reason: match reason {
                SuspendReason::SysOp => pb::SuspendReason::SysOp,
                SuspendReason::Await => pb::SuspendReason::Await,
                SuspendReason::AwaitAny => pb::SuspendReason::AwaitAny,
                SuspendReason::EarlyYield => pb::SuspendReason::EarlyYield,
            } as i32,
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::ResumeThread {
            thread_id,
            suspend_seq,
            suspend_ts_ticks,
            ts_ticks,
        } => Event::ResumeThread(pb::ResumeThread {
            thread_id: thread_id.0,
            suspend_seq,
            suspend_timestamp_ns: conv.to_ns(suspend_ts_ticks),
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
        RawRecord::LlmCallMeta {
            thread_id,
            call_id,
            model_id,
            tokens_in,
            tokens_out,
            flags,
            ts_ticks,
        } => Event::LlmCallMeta(pb::LlmCallMeta {
            thread_id: thread_id.0,
            call_id: call_id.0,
            model_id,
            tokens_in,
            tokens_out,
            flags: u32::from(flags),
            timestamp_ns: conv.to_ns(ts_ticks),
        }),
    };
    pb::DiskEventV1 { event: Some(event) }
}

#[cfg(test)]
mod tests {
    use crate::{
        ids::{BexCallId, BexThreadId},
        prof::{
            clock::TickConverter,
            pb::disk_event_v1::Event,
            record::{RawRecord, SuspendReason},
        },
    };

    use super::to_disk_event;

    #[test]
    fn cold_enrichment_records_transcode_without_losing_fields() {
        let conv = TickConverter::identity();
        let suspend = to_disk_event(
            &RawRecord::SuspendThread {
                reason: SuspendReason::AwaitAny,
                thread_id: BexThreadId(7),
                suspend_seq: 8,
                ts_ticks: 9,
            },
            &conv,
        );
        assert!(matches!(
            suspend.event,
            Some(Event::SuspendThread(ref event))
                if event.thread_id == 7 && event.suspend_seq == 8 && event.timestamp_ns == 9
        ));

        let resume = to_disk_event(
            &RawRecord::ResumeThread {
                thread_id: BexThreadId(7),
                suspend_seq: 8,
                suspend_ts_ticks: 9,
                ts_ticks: 19,
            },
            &conv,
        );
        assert!(matches!(
            resume.event,
            Some(Event::ResumeThread(ref event))
                if event.suspend_timestamp_ns == 9 && event.timestamp_ns == 19
        ));

        let llm = to_disk_event(
            &RawRecord::LlmCallMeta {
                thread_id: BexThreadId(7),
                call_id: BexCallId(10),
                model_id: 11,
                tokens_in: 12,
                tokens_out: 13,
                flags: 5,
                ts_ticks: 14,
            },
            &conv,
        );
        assert!(matches!(
            llm.event,
            Some(Event::LlmCallMeta(ref event))
                if event.call_id == 10
                    && event.model_id == 11
                    && event.tokens_in == 12
                    && event.tokens_out == 13
                    && event.flags == 5
        ));
    }
}
