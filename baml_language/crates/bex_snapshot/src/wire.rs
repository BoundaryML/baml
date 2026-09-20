//! Borsh layout of the state section.
//!
//! ```text
//! compile_time_len  u64      size of the source heap's compile-time region
//! run_state         Vec<u8>  opaque engine state (readable without a heap)
//! object_count      u32
//! objects           object_count x Vec<u8>   Borsh(Object), one blob each
//! threads           Vec<ThreadWire>
//! ```
//!
//! The object count comes first because the loader has to reserve every
//! object's slot before it can translate any reference. Each object is its own
//! length-prefixed blob so that a damaged object cannot shift the decoding of
//! the ones after it, and so that a reader can size objects by kind.
//!
//! Every `Value` and `HeapPtr` inside these structures is written through the
//! translation context of `bex_vm_types::snapshot_ctx`, as a `SnapRef`.

use baml_type::Span;
use bex_vm::{
    StackFrame,
    snapshot::{FrameState, ThrowContextState, VmThreadState},
};
use bex_vm_types::{HeapPtr, RealizedTy, Value};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct FrameWire {
    pub function: HeapPtr,
    pub function_name: String,
    pub instruction_ptr: u64,
    pub faulting_pc: u64,
    pub compact_pc: bool,
    pub locals_offset: u64,
    pub type_args: Vec<RealizedTy>,
    pub type_metadata: Option<Vec<Option<RealizedTy>>>,
    pub call_id: u64,
    pub parent_call_id: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct TraceFrameWire {
    pub function_name: String,
    pub file_path: String,
    pub function_span: Span,
    pub error_line: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct ThrowContextWire {
    pub trace: Vec<TraceFrameWire>,
    pub cause: Value,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct VmStateWire {
    pub frames: Vec<FrameWire>,
    pub stack: Vec<Value>,
    pub cur_pc: u64,
    pub call_id_counter: u64,
    pub current_call_id: u64,
    pub id_overrides: Vec<(u64, String)>,
    pub thrown_value_causes: Vec<(Value, Value)>,
    pub thrown_value_contexts: Vec<(Value, ThrowContextWire)>,
    pub preserved_throw_contexts: Vec<(Value, ThrowContextWire)>,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct ThreadWire {
    pub thread_id: u64,
    pub parent_thread: Option<u64>,
    pub name: String,
    pub parked_kind: String,
    pub parked_payload: Vec<u8>,
    pub extra_roots: Vec<Value>,
    pub state: VmStateWire,
}

fn usize_of(value: u64, what: &str) -> std::io::Result<usize> {
    usize::try_from(value).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{what} does not fit this platform's address size"),
        )
    })
}

fn context_to_wire(context: &ThrowContextState) -> ThrowContextWire {
    ThrowContextWire {
        trace: context
            .trace
            .iter()
            .map(|frame| TraceFrameWire {
                function_name: frame.function_name.clone(),
                file_path: frame.file_path.clone(),
                function_span: frame.function_span,
                error_line: frame.error_line as u64,
            })
            .collect(),
        cause: context.cause,
    }
}

fn context_from_wire(wire: ThrowContextWire) -> std::io::Result<ThrowContextState> {
    Ok(ThrowContextState {
        trace: wire
            .trace
            .into_iter()
            .map(|frame| {
                Ok(StackFrame {
                    function_name: frame.function_name,
                    file_path: frame.file_path,
                    function_span: frame.function_span,
                    error_line: usize_of(frame.error_line, "trace line")?,
                })
            })
            .collect::<std::io::Result<_>>()?,
        cause: wire.cause,
    })
}

impl VmStateWire {
    pub(crate) fn from_state(state: &VmThreadState) -> Self {
        Self {
            frames: state
                .frames
                .iter()
                .map(|frame| FrameWire {
                    function: frame.function,
                    function_name: frame.function_name.clone(),
                    instruction_ptr: frame.instruction_ptr as u64,
                    faulting_pc: frame.faulting_pc as u64,
                    compact_pc: frame.compact_pc,
                    locals_offset: frame.locals_offset as u64,
                    type_args: frame.type_args.clone(),
                    type_metadata: frame.type_metadata.clone(),
                    call_id: frame.call_id,
                    parent_call_id: frame.parent_call_id,
                })
                .collect(),
            stack: state.stack.clone(),
            cur_pc: state.cur_pc as u64,
            call_id_counter: state.call_id_counter,
            current_call_id: state.current_call_id,
            id_overrides: state.id_overrides.clone(),
            thrown_value_causes: state.thrown_value_causes.clone(),
            thrown_value_contexts: state
                .thrown_value_contexts
                .iter()
                .map(|(value, context)| (*value, context_to_wire(context)))
                .collect(),
            preserved_throw_contexts: state
                .preserved_throw_contexts
                .iter()
                .map(|(value, context)| (*value, context_to_wire(context)))
                .collect(),
        }
    }

    pub(crate) fn into_state(self) -> std::io::Result<VmThreadState> {
        let contexts = |list: Vec<(Value, ThrowContextWire)>| {
            list.into_iter()
                .map(|(value, context)| Ok((value, context_from_wire(context)?)))
                .collect::<std::io::Result<Vec<_>>>()
        };
        Ok(VmThreadState {
            frames: self
                .frames
                .into_iter()
                .map(|frame| {
                    Ok(FrameState {
                        function: frame.function,
                        function_name: frame.function_name,
                        instruction_ptr: usize_of(frame.instruction_ptr, "program counter")?,
                        faulting_pc: usize_of(frame.faulting_pc, "program counter")?,
                        compact_pc: frame.compact_pc,
                        locals_offset: usize_of(frame.locals_offset, "locals offset")?,
                        type_args: frame.type_args,
                        type_metadata: frame.type_metadata,
                        call_id: frame.call_id,
                        parent_call_id: frame.parent_call_id,
                    })
                })
                .collect::<std::io::Result<_>>()?,
            stack: self.stack,
            cur_pc: usize_of(self.cur_pc, "program counter")?,
            call_id_counter: self.call_id_counter,
            current_call_id: self.current_call_id,
            id_overrides: self.id_overrides,
            thrown_value_causes: self.thrown_value_causes,
            thrown_value_contexts: contexts(self.thrown_value_contexts)?,
            preserved_throw_contexts: contexts(self.preserved_throw_contexts)?,
        })
    }
}
