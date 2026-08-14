//! Shared raw-record to `.bamlprof` event transcode helpers.

use crate::prof::{
    clock::TickConverter,
    pb,
    record::{FunctionEndStatus, RawRecord, ThreadEndStatus},
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
    };
    pb::DiskEventV1 { event: Some(event) }
}
