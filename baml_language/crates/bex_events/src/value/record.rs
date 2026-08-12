use std::io;

use super::BlobRef;
use crate::{
    ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
    run::{
        CancellationState, ProjectGeneration, ProjectId, RunError, RunErrorClass,
        RunRequestSummary, RunStatus, RunTarget, RunTimeAnchor, SourceLocation, TraceCallKey,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueCodec {
    BamlOutboundValue,
}

impl ValueCodec {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::BamlOutboundValue => "bamlOutboundValue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueAvailability {
    Pending,
    Available,
    Missing,
    Omitted,
    Lost,
}

impl ValueAvailability {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Omitted => "omitted",
            Self::Lost => "lost",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRef {
    pub id: String,
    pub codec: ValueCodec,
    pub availability: ValueAvailability,
    pub original_size_bytes: Option<usize>,
    pub retained_size_bytes: Option<usize>,
    pub diagnostic: Option<String>,
}

impl ValueRef {
    #[must_use]
    pub fn available(
        id: impl Into<String>,
        codec: ValueCodec,
        original_size_bytes: usize,
        retained_size_bytes: usize,
    ) -> Self {
        Self {
            id: id.into(),
            codec,
            availability: ValueAvailability::Available,
            original_size_bytes: Some(original_size_bytes),
            retained_size_bytes: Some(retained_size_bytes),
            diagnostic: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueCaptureKind {
    RootInput,
    RootOutput,
    RootError,
    LogBody,
    CallOutput,
    CallError,
    CallInput,
}

impl ValueCaptureKind {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::RootInput => "rootInput",
            Self::RootOutput => "rootOutput",
            Self::RootError => "rootError",
            Self::LogBody => "logBody",
            Self::CallOutput => "callOutput",
            Self::CallError => "callError",
            Self::CallInput => "callInput",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueCapture {
    pub kind: ValueCaptureKind,
    pub call: TraceCallKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunStartedRecord {
    pub request: RunRequestSummary,
    pub created_at_ms: u64,
    pub time_anchor: RunTimeAnchor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunCompletedRecord {
    pub status: RunStatus,
    pub completed_at_ms: u64,
    pub renderer_hint: Option<String>,
    pub result_value_ref: Option<ValueRef>,
    pub error: Option<RunError>,
    pub cancellation: Option<CancellationState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRecord {
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
    pub blob_ref: Option<BlobRef>,
    pub capture: Option<ValueCapture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEventRecord {
    pub call: TraceCallKey,
    pub level: Option<String>,
    pub source: Option<SourceLocation>,
    pub timestamp_ms: u64,
    pub message_preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogRecord {
    pub value_ref: ValueRef,
    pub body: Vec<u8>,
    pub blob_ref: Option<BlobRef>,
    pub event: LogEventRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureLossKind {
    Value,
    Log,
}

impl CaptureLossKind {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Log => "log",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureLossReason {
    QueueFull,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureLossRecord {
    pub kind: CaptureLossKind,
    pub reason: CaptureLossReason,
    pub skipped_count: u64,
    pub call: Option<TraceCallKey>,
    pub message: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueFileRecord {
    CapturedValue(ValueRecord),
    LogEvent(LogRecord),
    CaptureLoss(CaptureLossRecord),
    RunStarted(RunStartedRecord),
    RunCompleted(RunCompletedRecord),
}

impl TryFrom<crate::value::pb::ValueMetadataV1> for ValueRef {
    type Error = io::Error;

    fn try_from(metadata: crate::value::pb::ValueMetadataV1) -> Result<Self, Self::Error> {
        let codec = match metadata.codec() {
            crate::value::pb::ValueCodec::BamlOutboundValue => ValueCodec::BamlOutboundValue,
            crate::value::pb::ValueCodec::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value metadata omitted codec",
                ));
            }
        };
        let availability = match metadata.availability() {
            crate::value::pb::ValueAvailability::Pending => ValueAvailability::Pending,
            crate::value::pb::ValueAvailability::Available => ValueAvailability::Available,
            crate::value::pb::ValueAvailability::Missing => ValueAvailability::Missing,
            crate::value::pb::ValueAvailability::Omitted => ValueAvailability::Omitted,
            crate::value::pb::ValueAvailability::Lost => ValueAvailability::Lost,
            crate::value::pb::ValueAvailability::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value metadata omitted availability",
                ));
            }
        };
        Ok(Self {
            id: metadata.id,
            codec,
            availability,
            original_size_bytes: metadata
                .original_size_bytes
                .map(usize::try_from)
                .transpose()
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "original size does not fit usize",
                    )
                })?,
            retained_size_bytes: metadata
                .retained_size_bytes
                .map(usize::try_from)
                .transpose()
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "retained size does not fit usize",
                    )
                })?,
            diagnostic: metadata.diagnostic,
        })
    }
}

impl From<&ValueRef> for crate::value::pb::ValueMetadataV1 {
    fn from(value_ref: &ValueRef) -> Self {
        Self {
            id: value_ref.id.clone(),
            codec: match value_ref.codec {
                ValueCodec::BamlOutboundValue => {
                    crate::value::pb::ValueCodec::BamlOutboundValue as i32
                }
            },
            availability: match value_ref.availability {
                ValueAvailability::Pending => crate::value::pb::ValueAvailability::Pending as i32,
                ValueAvailability::Available => {
                    crate::value::pb::ValueAvailability::Available as i32
                }
                ValueAvailability::Missing => crate::value::pb::ValueAvailability::Missing as i32,
                ValueAvailability::Omitted => crate::value::pb::ValueAvailability::Omitted as i32,
                ValueAvailability::Lost => crate::value::pb::ValueAvailability::Lost as i32,
            },
            original_size_bytes: value_ref
                .original_size_bytes
                .and_then(|value| u64::try_from(value).ok()),
            retained_size_bytes: value_ref
                .retained_size_bytes
                .and_then(|value| u64::try_from(value).ok()),
            diagnostic: value_ref.diagnostic.clone(),
        }
    }
}

impl TryFrom<crate::value::pb::BlobRefV1> for BlobRef {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::BlobRefV1) -> Result<Self, Self::Error> {
        let size_bytes = usize::try_from(value.size_bytes).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "blob size does not fit usize")
        })?;
        Ok(Self {
            algorithm: value.algorithm,
            digest: value.digest,
            size_bytes,
        })
    }
}

impl From<&BlobRef> for crate::value::pb::BlobRefV1 {
    fn from(value: &BlobRef) -> Self {
        Self {
            algorithm: value.algorithm.clone(),
            digest: value.digest.clone(),
            size_bytes: u64::try_from(value.size_bytes).unwrap_or(u64::MAX),
        }
    }
}

impl TryFrom<crate::value::pb::TraceCallKeyV1> for TraceCallKey {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::TraceCallKeyV1) -> Result<Self, Self::Error> {
        let process_id: [u8; 16] = value.process_id.as_slice().try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "trace call process id must be 16 bytes, got {}",
                    value.process_id.len()
                ),
            )
        })?;
        Ok(Self {
            process_euid: ProcessEuid(process_id),
            engine_id: EngineId(value.engine_id),
            thread_id: BexThreadId(value.thread_id),
            call_id: BexCallId(value.call_id),
        })
    }
}

impl From<TraceCallKey> for crate::value::pb::TraceCallKeyV1 {
    fn from(value: TraceCallKey) -> Self {
        Self {
            process_id: value.process_euid.0.to_vec(),
            engine_id: value.engine_id.0,
            thread_id: value.thread_id.0,
            call_id: value.call_id.0,
        }
    }
}

impl TryFrom<crate::value::pb::ValueCaptureV1> for ValueCapture {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::ValueCaptureV1) -> Result<Self, Self::Error> {
        let kind = match value.kind() {
            crate::value::pb::ValueCaptureKind::RootInput => ValueCaptureKind::RootInput,
            crate::value::pb::ValueCaptureKind::RootOutput => ValueCaptureKind::RootOutput,
            crate::value::pb::ValueCaptureKind::RootError => ValueCaptureKind::RootError,
            crate::value::pb::ValueCaptureKind::LogBody => ValueCaptureKind::LogBody,
            crate::value::pb::ValueCaptureKind::CallOutput => ValueCaptureKind::CallOutput,
            crate::value::pb::ValueCaptureKind::CallError => ValueCaptureKind::CallError,
            crate::value::pb::ValueCaptureKind::CallInput => ValueCaptureKind::CallInput,
            crate::value::pb::ValueCaptureKind::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value capture omitted kind",
                ));
            }
        };
        let call = value.call.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "value capture omitted call")
        })?;
        Ok(Self {
            kind,
            call: call.try_into()?,
        })
    }
}

impl From<&ValueCapture> for crate::value::pb::ValueCaptureV1 {
    fn from(value: &ValueCapture) -> Self {
        Self {
            kind: match value.kind {
                ValueCaptureKind::RootInput => crate::value::pb::ValueCaptureKind::RootInput as i32,
                ValueCaptureKind::RootOutput => {
                    crate::value::pb::ValueCaptureKind::RootOutput as i32
                }
                ValueCaptureKind::RootError => crate::value::pb::ValueCaptureKind::RootError as i32,
                ValueCaptureKind::LogBody => crate::value::pb::ValueCaptureKind::LogBody as i32,
                ValueCaptureKind::CallOutput => {
                    crate::value::pb::ValueCaptureKind::CallOutput as i32
                }
                ValueCaptureKind::CallError => crate::value::pb::ValueCaptureKind::CallError as i32,
                ValueCaptureKind::CallInput => crate::value::pb::ValueCaptureKind::CallInput as i32,
            },
            call: Some(value.call.into()),
        }
    }
}

impl TryFrom<crate::value::pb::SourceLocationV1> for SourceLocation {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::SourceLocationV1) -> Result<Self, Self::Error> {
        Ok(Self {
            file_path: value.file_path,
            file_id: value.file_id,
            line: value.line,
            column: value.column,
            end_line: value.end_line,
            end_column: value.end_column,
            start_offset: value.start_offset,
            end_offset: value.end_offset,
        })
    }
}

impl From<&SourceLocation> for crate::value::pb::SourceLocationV1 {
    fn from(value: &SourceLocation) -> Self {
        Self {
            file_path: value.file_path.clone(),
            file_id: value.file_id,
            line: value.line,
            column: value.column,
            end_line: value.end_line,
            end_column: value.end_column,
            start_offset: value.start_offset,
            end_offset: value.end_offset,
        }
    }
}

impl TryFrom<crate::value::pb::LogEventV1> for LogEventRecord {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::LogEventV1) -> Result<Self, Self::Error> {
        let call = value
            .call
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "log event omitted call"))?;
        Ok(Self {
            call: call.try_into()?,
            level: value.level,
            source: value.source.map(TryInto::try_into).transpose()?,
            timestamp_ms: value.timestamp_ms,
            message_preview: value.message_preview,
        })
    }
}

impl From<&LogEventRecord> for crate::value::pb::LogEventV1 {
    fn from(value: &LogEventRecord) -> Self {
        Self {
            call: Some(value.call.into()),
            level: value.level.clone(),
            source: value.source.as_ref().map(Into::into),
            timestamp_ms: value.timestamp_ms,
            message_preview: value.message_preview.clone(),
        }
    }
}

impl TryFrom<crate::value::pb::CaptureLossV1> for CaptureLossRecord {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::CaptureLossV1) -> Result<Self, Self::Error> {
        let kind = match value.kind() {
            crate::value::pb::CaptureLossKind::Value => CaptureLossKind::Value,
            crate::value::pb::CaptureLossKind::Log => CaptureLossKind::Log,
            crate::value::pb::CaptureLossKind::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "capture loss omitted kind",
                ));
            }
        };
        let reason = match value.reason() {
            crate::value::pb::CaptureLossReason::QueueFull => CaptureLossReason::QueueFull,
            crate::value::pb::CaptureLossReason::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "capture loss omitted reason",
                ));
            }
        };
        Ok(Self {
            kind,
            reason,
            skipped_count: value.skipped_count,
            call: value.call.map(TryInto::try_into).transpose()?,
            message: value.message,
            timestamp_ms: value.timestamp_ms,
        })
    }
}

impl From<&CaptureLossRecord> for crate::value::pb::CaptureLossV1 {
    fn from(value: &CaptureLossRecord) -> Self {
        Self {
            kind: match value.kind {
                CaptureLossKind::Value => crate::value::pb::CaptureLossKind::Value as i32,
                CaptureLossKind::Log => crate::value::pb::CaptureLossKind::Log as i32,
            },
            reason: match value.reason {
                CaptureLossReason::QueueFull => {
                    crate::value::pb::CaptureLossReason::QueueFull as i32
                }
            },
            skipped_count: value.skipped_count,
            call: value.call.map(Into::into),
            message: value.message.clone(),
            timestamp_ms: value.timestamp_ms,
        }
    }
}

impl TryFrom<crate::value::pb::RunStartedV1> for RunStartedRecord {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::RunStartedV1) -> Result<Self, Self::Error> {
        let target = value.target.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "run started record omitted target",
            )
        })?;
        let time_anchor = value.time_anchor.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "run started record omitted time anchor",
            )
        })?;
        Ok(Self {
            request: RunRequestSummary {
                project_id: ProjectId(value.project_id),
                project_generation: ProjectGeneration(value.project_generation),
                target: run_target_from_proto(target)?,
                args_summary: value.args_summary,
                options_summary: value.options_summary,
            },
            created_at_ms: value.created_at_ms,
            time_anchor: RunTimeAnchor {
                epoch_created_at_ms: time_anchor.epoch_created_at_ms,
                trace_zero_ns: time_anchor.trace_zero_ns,
            },
        })
    }
}

impl From<&RunStartedRecord> for crate::value::pb::RunStartedV1 {
    fn from(value: &RunStartedRecord) -> Self {
        Self {
            project_id: value.request.project_id.0.clone(),
            project_generation: value.request.project_generation.0,
            target: Some(run_target_to_proto(&value.request.target)),
            args_summary: value.request.args_summary.clone(),
            options_summary: value.request.options_summary.clone(),
            created_at_ms: value.created_at_ms,
            time_anchor: Some(crate::value::pb::TimeAnchorV1 {
                epoch_created_at_ms: value.time_anchor.epoch_created_at_ms,
                trace_zero_ns: value.time_anchor.trace_zero_ns,
            }),
        }
    }
}

impl TryFrom<crate::value::pb::RunCompletedV1> for RunCompletedRecord {
    type Error = io::Error;

    fn try_from(value: crate::value::pb::RunCompletedV1) -> Result<Self, Self::Error> {
        let status = match value.status() {
            crate::value::pb::RunStatus::Pending => RunStatus::Pending,
            crate::value::pb::RunStatus::Running => RunStatus::Running,
            crate::value::pb::RunStatus::WaitingForInput => RunStatus::WaitingForInput,
            crate::value::pb::RunStatus::WaitingForEnv => RunStatus::WaitingForEnv,
            crate::value::pb::RunStatus::Cancelling => RunStatus::Cancelling,
            crate::value::pb::RunStatus::Succeeded => RunStatus::Succeeded,
            crate::value::pb::RunStatus::Failed => RunStatus::Failed,
            crate::value::pb::RunStatus::Cancelled => RunStatus::Cancelled,
            crate::value::pb::RunStatus::Panicked => RunStatus::Panicked,
            crate::value::pb::RunStatus::Unspecified => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "run completed record omitted status",
                ));
            }
        };
        Ok(Self {
            status,
            completed_at_ms: value.completed_at_ms,
            renderer_hint: value.renderer_hint,
            result_value_ref: value.result_value_ref.map(ValueRef::try_from).transpose()?,
            error: value.error.map(run_error_from_proto).transpose()?,
            cancellation: value.cancellation.map(cancellation_from_proto),
        })
    }
}

impl From<&RunCompletedRecord> for crate::value::pb::RunCompletedV1 {
    fn from(value: &RunCompletedRecord) -> Self {
        Self {
            status: run_status_to_proto(value.status) as i32,
            completed_at_ms: value.completed_at_ms,
            renderer_hint: value.renderer_hint.clone(),
            result_value_ref: value.result_value_ref.as_ref().map(Into::into),
            error: value.error.as_ref().map(run_error_to_proto),
            cancellation: value.cancellation.as_ref().map(cancellation_to_proto),
        }
    }
}

fn run_target_from_proto(value: crate::value::pb::RunTargetV1) -> io::Result<RunTarget> {
    use crate::value::pb::run_target_v1::Target;
    match value
        .target
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "run target omitted variant"))?
    {
        Target::Function(target) => Ok(RunTarget::Function {
            function_name: target.function_name,
        }),
        Target::Test(target) => Ok(RunTarget::Test {
            generation: ProjectGeneration(target.generation),
            test_name: target.test_name,
        }),
        Target::Preview(target) => Ok(RunTarget::Preview {
            parent_function_name: target.parent_function_name,
            helper: target.helper,
        }),
        Target::Companion(target) => {
            let parent_boundary_id = target
                .parent_boundary_id
                .as_deref()
                .map(boundary_id_from_slice)
                .transpose()?;
            Ok(RunTarget::Companion {
                parent_boundary_id,
                function_name: target.function_name,
            })
        }
        Target::Internal(target) => Ok(RunTarget::Internal { name: target.name }),
    }
}

fn run_target_to_proto(value: &RunTarget) -> crate::value::pb::RunTargetV1 {
    use crate::value::pb::run_target_v1::Target;
    crate::value::pb::RunTargetV1 {
        target: Some(match value {
            RunTarget::Function { function_name } => {
                Target::Function(crate::value::pb::FunctionRunTargetV1 {
                    function_name: function_name.clone(),
                })
            }
            RunTarget::Test {
                generation,
                test_name,
            } => Target::Test(crate::value::pb::TestRunTargetV1 {
                generation: generation.0,
                test_name: test_name.clone(),
            }),
            RunTarget::Preview {
                parent_function_name,
                helper,
            } => Target::Preview(crate::value::pb::PreviewRunTargetV1 {
                parent_function_name: parent_function_name.clone(),
                helper: helper.clone(),
            }),
            RunTarget::Companion {
                parent_boundary_id,
                function_name,
            } => Target::Companion(crate::value::pb::CompanionRunTargetV1 {
                parent_boundary_id: parent_boundary_id.map(|id| id.as_bytes().to_vec()),
                function_name: function_name.clone(),
            }),
            RunTarget::Internal { name } => {
                Target::Internal(crate::value::pb::InternalRunTargetV1 { name: name.clone() })
            }
        }),
    }
}

fn boundary_id_from_slice(value: &[u8]) -> io::Result<BoundaryId> {
    let bytes: [u8; 16] = value.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("boundary id must be 16 bytes, got {}", value.len()),
        )
    })?;
    Ok(BoundaryId::from_bytes(bytes))
}

fn run_status_to_proto(status: RunStatus) -> crate::value::pb::RunStatus {
    match status {
        RunStatus::Pending => crate::value::pb::RunStatus::Pending,
        RunStatus::Running => crate::value::pb::RunStatus::Running,
        RunStatus::WaitingForInput => crate::value::pb::RunStatus::WaitingForInput,
        RunStatus::WaitingForEnv => crate::value::pb::RunStatus::WaitingForEnv,
        RunStatus::Cancelling => crate::value::pb::RunStatus::Cancelling,
        RunStatus::Succeeded => crate::value::pb::RunStatus::Succeeded,
        RunStatus::Failed => crate::value::pb::RunStatus::Failed,
        RunStatus::Cancelled => crate::value::pb::RunStatus::Cancelled,
        RunStatus::Panicked => crate::value::pb::RunStatus::Panicked,
    }
}

fn run_error_from_proto(value: crate::value::pb::RunErrorV1) -> io::Result<RunError> {
    Ok(RunError {
        class: match value.class.as_str() {
            "validation" => RunErrorClass::Validation,
            "runtime" => RunErrorClass::Runtime,
            "host" => RunErrorClass::Host,
            "panic" => RunErrorClass::Panic,
            "cancelled" => RunErrorClass::Cancelled,
            "internal" => RunErrorClass::Internal,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown run error class: {other}"),
                ));
            }
        },
        message: value.message,
        details: value.details,
        value_ref: value.value_ref.map(ValueRef::try_from).transpose()?,
    })
}

fn run_error_to_proto(value: &RunError) -> crate::value::pb::RunErrorV1 {
    crate::value::pb::RunErrorV1 {
        class: match value.class {
            RunErrorClass::Validation => "validation",
            RunErrorClass::Runtime => "runtime",
            RunErrorClass::Host => "host",
            RunErrorClass::Panic => "panic",
            RunErrorClass::Cancelled => "cancelled",
            RunErrorClass::Internal => "internal",
        }
        .to_string(),
        message: value.message.clone(),
        details: value.details.clone(),
        value_ref: value.value_ref.as_ref().map(Into::into),
    }
}

fn cancellation_from_proto(value: crate::value::pb::RunCancellationV1) -> CancellationState {
    CancellationState {
        requested_at_ms: value.requested_at_ms,
        completed_at_ms: value.completed_at_ms,
        reason: value.reason,
    }
}

fn cancellation_to_proto(value: &CancellationState) -> crate::value::pb::RunCancellationV1 {
    crate::value::pb::RunCancellationV1 {
        requested_at_ms: value.requested_at_ms,
        completed_at_ms: value.completed_at_ms,
        reason: value.reason.clone(),
    }
}
