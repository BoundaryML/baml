//! Export and import of the pointer-bearing state of one VM.
//!
//! A heap snapshot (crate `bex_snapshot`) stores, for every paused BAML thread,
//! the call frames and the operand stack of its [`BexVm`]. This module is the
//! boundary between the VM's private layout and that crate:
//!
//! - [`BexVm::export_thread_state`] copies the state out as a [`VmThreadState`].
//! - [`BexVm::import_thread_state`] installs a state into a fresh VM that was
//!   built from the identical program.
//! - [`BexVm::snapshot_roots`] and [`BexVm::snapshot_root_paths`] list the heap
//!   values the state refers to, for the snapshot writer's graph walk.
//! - [`BexVm::snapshot_frame_views`] describes every frame (including native
//!   continuation frames) for state dumps.
//!
//! Process-scoped fields of the VM are not part of the state: the profiler
//! ring and session, capture hooks and pending capture events, the
//! address-keyed caches, `bex_ref_seed`, and the pending sys-op profiling ids.
//! An imported VM keeps whatever its constructor gave it for those fields, and
//! the caches refill lazily.
//!
//! The throw bookkeeping vectors (`thrown_value_causes`,
//! `thrown_value_contexts`, `preserved_throw_contexts`) and the exact
//! runtime-type metadata of a frame (`BytecodeFrame::type_metadata`) are part
//! of the state, so a value that was thrown before a snapshot and is rethrown
//! after the restore keeps its recorded cause chain and trace.

use std::sync::Arc;

use bex_vm_types::{HeapPtr, Object, RealizedTy, Value, types::Function};

pub use crate::package_baml::ContinuationState;
use crate::{
    BexVm, StackFrame,
    vm::{BytecodeFrame, Frame, FrameTypeMetadata, NativeFrame, ThrowContext, VmCaptureMask},
};

/// One call frame in position-independent form (apart from the heap
/// references, which the snapshot crate translates).
///
/// A bytecode frame has `native: None`. A native continuation frame (the
/// frame of `Array.map` while its callback runs, for example) has
/// `native: Some(..)`: `function` is the native function object,
/// `function_name` its name, and the program counters, `locals_offset`, the
/// type fields, and the call ids are zero or empty.
#[derive(Clone, Debug)]
pub struct FrameState {
    /// The state of the continuation of a native frame.
    pub native: Option<ContinuationState>,
    /// The callable the frame runs: an `Object::Function`, or a closure,
    /// bound method, or generic function value that wraps one.
    pub function: HeapPtr,
    /// Name of the underlying function. Import checks it against the target
    /// program, so that a snapshot cannot resume inside a different function.
    pub function_name: String,
    /// Program counter of the next instruction. The unit is whatever the
    /// function's bytecode uses in this build (a byte offset into
    /// `CompactCode` when present, an instruction index otherwise).
    pub instruction_ptr: usize,
    /// Program counter of the call site or of the faulting instruction.
    pub faulting_pc: usize,
    /// True when the program counters are byte offsets into `CompactCode`
    /// (an engine-built heap), false when they are instruction indices (a
    /// heap from `BexVm::from_program`). Import refuses a state whose unit
    /// differs from the target function's.
    pub compact_pc: bool,
    /// Index in the operand stack of the frame's first local (see
    /// [`local_slot`]).
    pub locals_offset: usize,
    /// Realized type arguments of the call.
    pub type_args: Vec<RealizedTy>,
    /// Exact runtime-type metadata of the call (BEP-066): one optional exact
    /// type per explicitly supplied type argument. `None` for an ordinary
    /// call, which carries no metadata.
    pub type_metadata: Option<Vec<Option<RealizedTy>>>,
    /// `$id` call id of this frame.
    pub call_id: u64,
    /// `$id` call id of the caller.
    pub parent_call_id: u64,
}

/// Recorded context of a thrown value.
#[derive(Clone, Debug)]
pub struct ThrowContextState {
    pub trace: Vec<StackFrame>,
    pub cause: Value,
}

/// Pointer-bearing state of one VM: frames, operand stack, exception
/// bookkeeping, and call id counters.
#[derive(Clone, Debug, Default)]
pub struct VmThreadState {
    /// Call frames, outermost first.
    pub frames: Vec<FrameState>,
    /// The flat operand stack. Frame locals live in it.
    pub stack: Vec<Value>,
    /// Start of the instruction that the innermost frame was executing.
    pub cur_pc: usize,
    pub call_id_counter: u64,
    pub current_call_id: u64,
    /// `baml.id.set` overrides, keyed by call id.
    pub id_overrides: Vec<(u64, String)>,
    /// Cause recorded at the fresh throw site of each thrown value.
    pub thrown_value_causes: Vec<(Value, Value)>,
    /// Most recently observed trace and cause of each thrown value.
    pub thrown_value_contexts: Vec<(Value, ThrowContextState)>,
    /// One-shot context transfers registered by `UnknownError` conversions.
    pub preserved_throw_contexts: Vec<(Value, ThrowContextState)>,
}

impl VmThreadState {
    /// Call `f` on every value slot of the state.
    pub fn visit_values(&self, f: &mut impl FnMut(Value)) {
        for value in &self.stack {
            f(*value);
        }
        for (value, cause) in &self.thrown_value_causes {
            f(*value);
            f(*cause);
        }
        for (value, context) in self
            .thrown_value_contexts
            .iter()
            .chain(&self.preserved_throw_contexts)
        {
            f(*value);
            f(context.cause);
        }
    }

    /// Every heap value the state refers to: stack slots, frame callables, and
    /// the declarations named by frame type arguments.
    #[must_use]
    pub fn roots(&self) -> Vec<Value> {
        let mut roots = Vec::new();
        self.visit_values(&mut |value| {
            if value.is_object() {
                roots.push(value);
            }
        });
        for frame in &self.frames {
            push_ptr(&mut roots, frame.function);
            if let Some(native) = &frame.native {
                for ptr in &native.ptrs {
                    push_ptr(&mut roots, *ptr);
                }
                roots.extend(
                    native
                        .values
                        .iter()
                        .copied()
                        .filter(|value| value.is_object()),
                );
            }
            for ty in frame.frame_types() {
                ty.visit_heads(&mut |head| {
                    if head.is_resolved() {
                        push_ptr(&mut roots, head.ptr());
                    }
                });
            }
        }
        roots
    }
}

impl FrameState {
    /// Every type the frame carries: its type arguments, the exact types of
    /// its runtime-type metadata, and the types of a native continuation.
    pub fn frame_types(&self) -> impl Iterator<Item = &RealizedTy> {
        self.type_args
            .iter()
            .chain(self.type_metadata.iter().flatten().flatten())
            .chain(self.native.iter().flat_map(|native| native.types.iter()))
    }

    /// Mutable form of [`Self::frame_types`], for the loader's head binding.
    pub fn frame_types_mut(&mut self) -> impl Iterator<Item = &mut RealizedTy> {
        self.type_args
            .iter_mut()
            .chain(self.type_metadata.iter_mut().flatten().flatten())
            .chain(
                self.native
                    .iter_mut()
                    .flat_map(|native| native.types.iter_mut()),
            )
    }

    /// Call `f` on every value a native continuation holds.
    pub fn visit_native_values(&self, f: &mut impl FnMut(Value)) {
        if let Some(native) = &self.native {
            for ptr in &native.ptrs {
                if !ptr.is_null() {
                    f(Value::object(*ptr));
                }
            }
            for value in &native.values {
                f(*value);
            }
        }
    }
}

fn push_ptr(roots: &mut Vec<Value>, ptr: HeapPtr) {
    if !ptr.is_null() {
        roots.push(Value::object(ptr));
    }
}

/// Read-only description of one call frame, for state dumps and for naming
/// snapshot roots. Unlike [`FrameState`] it also covers native frames.
#[derive(Clone, Debug)]
pub struct FrameView {
    /// The frame's callable object.
    pub function: HeapPtr,
    /// Name of the underlying function, when it resolves.
    pub function_name: Option<String>,
    /// Program counter to attribute a source line to: the live program
    /// counter for the innermost bytecode frame, the call site otherwise.
    pub pc: usize,
    /// Index in the operand stack of the frame's first local (`None` for a
    /// native continuation frame, which owns no stack region).
    pub locals_offset: Option<usize>,
    /// True for a native continuation frame.
    pub native: bool,
}

/// A snapshot root with a human-readable path, outermost element first.
#[derive(Clone, Debug)]
pub struct SnapshotRoot {
    pub value: Value,
    pub path: Vec<String>,
}

/// The local slot that owns stack index `position` in a frame of `function`
/// whose locals start at `locals_offset`, or `None` when the position lies
/// above the frame's locals (an operand-stack temporary) or below them.
///
/// Local slot `s >= 1` lives at stack index `locals_offset + s - 1`. Slot 0 is
/// reserved for the callee and is never materialized on the stack. The locals
/// region covers the parameters and then `real_local_count` further slots.
#[must_use]
pub fn local_slot(function: &Function, locals_offset: usize, position: usize) -> Option<usize> {
    let slot = position.checked_sub(locals_offset)? + 1;
    (slot <= function.arity + function.real_local_count).then_some(slot)
}

/// The name of the source-level local variable in local slot `slot` (see
/// [`local_slot`]) of `function`, from the function's debug information.
/// `None` for an unnamed slot and for a compiler temporary.
///
/// An optimized build may keep a source variable in no slot at all (the
/// compiler inlines single-use bindings), so the named locals of a frame can
/// be fewer than the source declares.
#[must_use]
pub fn local_name(function: &Function, slot: usize) -> Option<&str> {
    function
        .local_names
        .get(slot)
        .map(String::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            function
                .debug_locals
                .iter()
                .find(|local| local.slot == slot)
                .map(|local| local.name.as_str())
        })
        .filter(|name| !is_compiler_temporary(name))
}

/// Compiler temporaries are named `_` followed by digits (`_0`, `_14`).
fn is_compiler_temporary(name: &str) -> bool {
    name.strip_prefix('_')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// Source line (1-based, 0 when unknown) of `pc` in `function`.
#[must_use]
pub fn source_line(function: &Function, pc: usize) -> usize {
    match &function.bytecode.compact {
        Some(compact) => compact.source_line_for_pc(pc),
        None => function.bytecode.source_line_for_pc(pc),
    }
}

fn code_len(function: &Function) -> usize {
    match &function.bytecode.compact {
        Some(compact) => compact.code.len(),
        None => function.bytecode.instructions.len(),
    }
}

/// Whether `pc` (already known to be `<= code_len(function)`) is the start of
/// an instruction. The dispatch loop reads opcodes and operands without bounds
/// checks, so a program counter in the middle of an instruction must never be
/// installed. An instruction index (no compact code) is always a boundary.
fn is_instruction_boundary(function: &Function, pc: usize) -> bool {
    let Some(compact) = &function.bytecode.compact else {
        return true;
    };
    let code = &compact.code;
    let mut at = 0usize;
    while at < pc {
        // `at < pc <= code.len()`, so the index is in bounds.
        let Ok(opcode) = bex_vm_types::bytecode::OpCode::try_from(code[at]) else {
            return false;
        };
        at += opcode.encoded_size();
    }
    at == pc
}

impl BexVm {
    /// The `Object::Function` behind a frame's callable, with its address.
    fn snapshot_resolve_function(&self, callable: HeapPtr) -> Option<(HeapPtr, &Function)> {
        if callable.is_null() {
            return None;
        }
        let function_ptr = match self.get_object(callable) {
            Object::Function(_) => callable,
            Object::Closure(closure) => closure.function,
            Object::BoundMethod(bound) => bound.function,
            Object::GenericFunction(generic) => self.generic_function_authored_ptr(generic).ok()?,
            _ => return None,
        };
        if function_ptr.is_null() {
            return None;
        }
        match self.get_object(function_ptr) {
            Object::Function(function) => Some((function_ptr, function)),
            _ => None,
        }
    }

    /// The `Object::Function` a frame's callable runs, for callers outside the
    /// crate that render frames (state dumps).
    #[must_use]
    pub fn snapshot_function_of(&self, callable: HeapPtr) -> Option<&Function> {
        self.snapshot_resolve_function(callable)
            .map(|(_, function)| function)
    }

    /// Describe every call frame, outermost first.
    #[must_use]
    pub fn snapshot_frame_views(&self) -> Vec<FrameView> {
        let top_bytecode = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame, Frame::Bytecode(_)));
        self.frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let function_name = self
                    .snapshot_resolve_function(frame.function())
                    .map(|(_, function)| function.name.clone());
                match frame {
                    Frame::Bytecode(frame) => FrameView {
                        function: frame.function,
                        function_name,
                        pc: if Some(index) == top_bytecode {
                            self.cur_pc
                        } else {
                            frame.faulting_pc
                        },
                        locals_offset: Some(frame.locals_offset.raw()),
                        native: false,
                    },
                    Frame::Native(frame) => FrameView {
                        function: frame.function,
                        function_name,
                        pc: 0,
                        locals_offset: None,
                        native: true,
                    },
                }
            })
            .collect()
    }

    /// Every heap value the exported state refers to. The snapshot writer
    /// walks the object graph from these values (plus the values the engine
    /// holds outside the VM at the yield).
    ///
    /// A native continuation frame contributes its function and everything its
    /// continuation holds.
    #[must_use]
    pub fn snapshot_roots(&self) -> Vec<Value> {
        self.snapshot_root_paths()
            .into_iter()
            .map(|root| root.value)
            .collect()
    }

    /// [`Self::snapshot_roots`] with a path for each root that names the frame
    /// and, when debug information has it, the local variable.
    #[must_use]
    pub fn snapshot_root_paths(&self) -> Vec<SnapshotRoot> {
        let views = self.snapshot_frame_views();
        let mut roots = Vec::new();

        // Frame callables and the declarations their type arguments name.
        for (frame, view) in self.frames.iter().zip(&views) {
            let frame_label = frame_label(view);
            if !view.function.is_null() {
                roots.push(SnapshotRoot {
                    value: Value::object(view.function),
                    path: vec![frame_label.clone(), "callee".to_string()],
                });
            }
            if let Frame::Native(native) = frame {
                for ptr in native.continuation.gc_roots() {
                    if !ptr.is_null() {
                        roots.push(SnapshotRoot {
                            value: Value::object(ptr),
                            path: vec![
                                frame_label.clone(),
                                "a value held by the native call".to_string(),
                            ],
                        });
                    }
                }
            }
            if let Frame::Bytecode(frame) = frame {
                for (index, ty) in frame.type_args.iter().enumerate() {
                    ty.visit_heads(&mut |head| {
                        if head.is_resolved() {
                            roots.push(SnapshotRoot {
                                value: Value::object(head.ptr()),
                                path: vec![frame_label.clone(), format!("type argument {index}")],
                            });
                        }
                    });
                }
                let metadata = frame.type_metadata.iter().flat_map(|m| m.values.iter());
                for (index, value) in metadata.enumerate() {
                    let Some(value) = value else { continue };
                    value.ty.visit_heads(&mut |head| {
                        if head.is_resolved() {
                            roots.push(SnapshotRoot {
                                value: Value::object(head.ptr()),
                                path: vec![
                                    frame_label.clone(),
                                    format!("exact type argument {index}"),
                                ],
                            });
                        }
                    });
                }
            }
        }

        // Throw bookkeeping: thrown values and their recorded causes.
        let mut throw_root = |value: Value, what: &str| {
            if value.is_object() {
                roots.push(SnapshotRoot {
                    value,
                    path: vec![what.to_string()],
                });
            }
        };
        for (value, cause) in &self.thrown_value_causes {
            throw_root(*value, "a thrown value");
            throw_root(*cause, "the cause of a thrown value");
        }
        for (value, context) in self
            .thrown_value_contexts
            .iter()
            .chain(&self.preserved_throw_contexts)
        {
            throw_root(*value, "a thrown value");
            throw_root(context.cause, "the cause of a thrown value");
        }

        // Stack slots, attributed to the innermost frame that owns them.
        let bytecode_views: Vec<&FrameView> = views
            .iter()
            .filter(|view| view.locals_offset.is_some())
            .collect();
        for (index, value) in self.stack.0.iter().enumerate() {
            if !value.is_object() {
                continue;
            }
            let owner = bytecode_views
                .iter()
                .rev()
                .find(|view| view.locals_offset.is_some_and(|offset| offset <= index));
            let path = match owner {
                Some(view) => {
                    let offset = view.locals_offset.unwrap_or(0);
                    let function = self
                        .snapshot_resolve_function(view.function)
                        .map(|(_, function)| function);
                    let slot = function.and_then(|function| local_slot(function, offset, index));
                    let name = function
                        .zip(slot)
                        .and_then(|(function, slot)| local_name(function, slot));
                    let slot_label = match (name, slot) {
                        (Some(name), _) => format!("local `{name}`"),
                        (None, Some(slot)) => format!("local slot {slot}"),
                        (None, None) => format!("operand stack [{}]", index - offset),
                    };
                    vec![frame_label(view), slot_label]
                }
                None => vec![format!("stack slot {index}")],
            };
            roots.push(SnapshotRoot {
                value: *value,
                path,
            });
        }
        roots
    }

    /// Copy the pointer-bearing state out of this VM.
    ///
    /// The VM must be parked at a yield (`exec()` has returned). The values in
    /// the result point into this VM's heap and are only meaningful while the
    /// collector cannot move objects.
    ///
    /// # Errors
    ///
    /// Returns a description of the obstacle when the state cannot be
    /// represented: a native continuation frame whose continuation has no
    /// snapshot form is on the call stack.
    pub fn export_thread_state(&self) -> Result<VmThreadState, String> {
        let mut frames = Vec::with_capacity(self.frames.len());
        for frame in &self.frames {
            let frame = match frame {
                Frame::Bytecode(frame) => frame,
                Frame::Native(native) => {
                    let name = self
                        .snapshot_resolve_function(native.function)
                        .map_or_else(|| "<unknown>".to_string(), |(_, f)| f.name.clone());
                    let Some(state) = native.continuation.snapshot() else {
                        return Err(format!(
                            "a native continuation frame ({name}, {}) is on the call stack; \
                             this native continuation cannot be serialized",
                            native.continuation.snapshot_name()
                        ));
                    };
                    frames.push(FrameState {
                        native: Some(state),
                        function: native.function,
                        function_name: name,
                        instruction_ptr: 0,
                        faulting_pc: 0,
                        compact_pc: false,
                        locals_offset: 0,
                        type_args: Vec::new(),
                        type_metadata: None,
                        call_id: 0,
                        parent_call_id: 0,
                    });
                    continue;
                }
            };
            let Some((_, function)) = self.snapshot_resolve_function(frame.function) else {
                return Err("a call frame does not resolve to a function object".to_string());
            };
            frames.push(FrameState {
                native: None,
                function: frame.function,
                function_name: function.name.clone(),
                instruction_ptr: frame.instruction_ptr,
                faulting_pc: frame.faulting_pc,
                compact_pc: function.bytecode.compact.is_some(),
                locals_offset: frame.locals_offset.raw(),
                type_args: frame.type_args.clone(),
                type_metadata: frame.type_metadata.as_ref().map(|metadata| {
                    metadata
                        .values
                        .iter()
                        .map(|value| value.as_ref().map(|value| value.ty.clone()))
                        .collect()
                }),
                call_id: frame.call_id,
                parent_call_id: frame.parent_call_id,
            });
        }
        let contexts = |list: &[(Value, ThrowContext)]| {
            list.iter()
                .map(|(value, context)| {
                    (
                        *value,
                        ThrowContextState {
                            trace: context.trace.to_vec(),
                            cause: context.cause,
                        },
                    )
                })
                .collect()
        };
        Ok(VmThreadState {
            frames,
            stack: self.stack.0.clone(),
            cur_pc: self.cur_pc,
            call_id_counter: self.call_id_counter,
            current_call_id: self.current_call_id,
            id_overrides: self.id_overrides.clone(),
            thrown_value_causes: self.thrown_value_causes.clone(),
            thrown_value_contexts: contexts(&self.thrown_value_contexts),
            preserved_throw_contexts: contexts(&self.preserved_throw_contexts),
        })
    }

    /// Install `state` into this VM, which must be fresh (no frames) and built
    /// from the same program as the VM that exported the state. Every pointer
    /// in `state` must already point into this VM's heap.
    ///
    /// Process-scoped fields are left as the constructor set them. Restored
    /// frames have output capture disabled.
    ///
    /// # Errors
    ///
    /// Returns a description of the first inconsistency: the VM is not fresh,
    /// a frame's callable is not a bytecode function of the expected name, a
    /// program counter or a locals offset is out of bounds, a frame's next
    /// program counter is not the start of an instruction, or a frame's
    /// runtime-type metadata does not match its type arguments.
    pub fn import_thread_state(&mut self, state: VmThreadState) -> Result<(), String> {
        if !self.frames.is_empty() || !self.stack.0.is_empty() {
            return Err(
                "import_thread_state requires a VM with no frames and an empty stack".into(),
            );
        }

        // Validate before touching the VM.
        let mut previous_offset = 0usize;
        let mut continuations = Vec::new();
        for (index, frame) in state.frames.iter().enumerate() {
            let Some((_, function)) = self.snapshot_resolve_function(frame.function) else {
                return Err(format!(
                    "frame {index} ({}) does not resolve to a function object",
                    frame.function_name
                ));
            };
            if function.name != frame.function_name {
                return Err(format!(
                    "frame {index} names function {} but the program has {} at that position",
                    frame.function_name, function.name
                ));
            }
            if let Some(native) = &frame.native {
                if matches!(function.kind, bex_vm_types::FunctionKind::Bytecode) {
                    return Err(format!(
                        "frame {index} ({}) is a native continuation of a bytecode function",
                        function.name
                    ));
                }
                // A continuation calls or reads the objects it points at when
                // its callback returns, without a check of its own. The bytes
                // of a snapshot are not trusted, so every pointer must name
                // what the continuation expects: the callback of an array
                // combinator is callable, the class walk of `from_json` holds
                // its class, and the `to_string` and `to_json` walks hold the
                // values whose overrides are still to run.
                for (position, ptr) in native.ptrs.iter().enumerate() {
                    let fits = !ptr.is_null()
                        && if native.tag == "json.from_json_class" {
                            matches!(self.get_object(*ptr), Object::Class(_))
                        } else if native.tag.starts_with("array.") {
                            self.snapshot_resolve_function(*ptr).is_some()
                        } else {
                            true
                        };
                    if !fits {
                        return Err(format!(
                            "frame {index} ({}): pointer {position} of continuation `{}` is {}",
                            function.name,
                            native.tag,
                            if ptr.is_null() {
                                "null"
                            } else {
                                "not an object the continuation can use"
                            }
                        ));
                    }
                }
                let continuation = crate::package_baml::restore_continuation(native)
                    .map_err(|error| format!("frame {index} ({}): {error}", function.name))?;
                continuations.push(continuation);
                continue;
            }
            if !matches!(function.kind, bex_vm_types::FunctionKind::Bytecode) {
                return Err(format!(
                    "frame {index} ({}) is not a bytecode function",
                    function.name
                ));
            }
            if frame.compact_pc != function.bytecode.compact.is_some() {
                return Err(format!(
                    "frame {index} ({}) was exported with a different bytecode encoding \
                     (compact: {}) than this VM uses",
                    function.name, frame.compact_pc
                ));
            }
            // The dispatch loop reads the opcode at `instruction_ptr` without
            // a bounds check, so the next program counter must name an
            // instruction, not the end of the code.
            let len = code_len(function);
            if frame.instruction_ptr >= len || frame.faulting_pc >= len {
                return Err(format!(
                    "frame {index} ({}) has a program counter outside its code ({} / {} >= {len})",
                    function.name, frame.instruction_ptr, frame.faulting_pc
                ));
            }
            for pc in [frame.instruction_ptr, frame.faulting_pc] {
                if !is_instruction_boundary(function, pc) {
                    return Err(format!(
                        "frame {index} ({}) has program counter {pc}, which is not the start \
                         of an instruction",
                        function.name
                    ));
                }
            }
            // Local-variable instructions index the stack without bounds
            // checks, so the frame's whole locals region (parameters, then the
            // real locals) must lie inside the stack and below the next frame.
            let locals_end = frame
                .locals_offset
                .checked_add(function.arity)
                .and_then(|end| end.checked_add(function.real_local_count));
            let limit = state.frames[index + 1..]
                .iter()
                .find(|next| next.native.is_none())
                .map_or(state.stack.len(), |next| next.locals_offset);
            if frame.locals_offset < previous_offset
                || locals_end.is_none_or(|end| end > limit)
                || limit > state.stack.len()
            {
                return Err(format!(
                    "frame {index} ({}) has locals offset {} and {} local slots, which do not \
                     fit below {limit} (stack len {})",
                    function.name,
                    frame.locals_offset,
                    function.arity + function.real_local_count,
                    state.stack.len()
                ));
            }
            previous_offset = frame.locals_offset;
            if let Some(metadata) = &frame.type_metadata
                && metadata.len() != frame.type_args.len()
            {
                return Err(format!(
                    "frame {index} ({}) has {} exact type arguments for {} type arguments",
                    function.name,
                    metadata.len(),
                    frame.type_args.len()
                ));
            }
        }
        // `cur_pc` is a diagnostic register: the first dispatched instruction
        // overwrites it, and until then it is only used to look up source
        // lines and exception handlers. A value that is not an instruction of
        // the top frame's function is replaced, not refused.
        let mut cur_pc = state.cur_pc;
        if let Some(last) = state
            .frames
            .iter()
            .rev()
            .find(|frame| frame.native.is_none())
            && let Some((_, function)) = self.snapshot_resolve_function(last.function)
            && (cur_pc >= code_len(function) || !is_instruction_boundary(function, cur_pc))
        {
            cur_pc = last.instruction_ptr;
        }

        let mut continuations = continuations.into_iter();
        for frame in state.frames {
            if frame.native.is_some() {
                let Some(continuation) = continuations.next() else {
                    return Err("a native frame lost its continuation".to_string());
                };
                self.frames.push(Frame::Native(NativeFrame {
                    function: frame.function,
                    continuation,
                }));
                continue;
            }
            let function_id = self
                .snapshot_resolve_function(frame.function)
                .map_or(0, |(_, function)| function.function_id);
            self.frames.push(Frame::Bytecode(BytecodeFrame {
                function: frame.function,
                instruction_ptr: frame.instruction_ptr,
                locals_offset: bex_vm_types::StackIndex::from_raw(frame.locals_offset),
                type_args: frame.type_args,
                type_metadata: frame.type_metadata.map(|values| {
                    Box::new(FrameTypeMetadata {
                        values: values
                            .into_iter()
                            .map(|ty| ty.map(bex_vm_types::types::TypeValue::new))
                            .collect(),
                    })
                }),
                faulting_pc: frame.faulting_pc,
                call_id: frame.call_id,
                parent_call_id: frame.parent_call_id,
                capture_mask: VmCaptureMask::disabled(),
                function_id,
            }));
        }

        let contexts = |list: Vec<(Value, ThrowContextState)>| {
            list.into_iter()
                .map(|(value, context)| {
                    (
                        value,
                        ThrowContext {
                            trace: Arc::from(context.trace),
                            cause: context.cause,
                        },
                    )
                })
                .collect()
        };
        self.thrown_value_causes = state.thrown_value_causes;
        self.thrown_value_contexts = contexts(state.thrown_value_contexts);
        self.preserved_throw_contexts = contexts(state.preserved_throw_contexts);
        self.stack.0 = state.stack;
        self.cur_pc = cur_pc;
        self.call_id_counter = state.call_id_counter;
        self.current_call_id = state.current_call_id;
        self.id_overrides = state.id_overrides;
        self.pending_sysop_call_id = None;
        self.pending_sysop_function_id = None;
        self.pending_sysop_capture_mask = VmCaptureMask::disabled();
        Ok(())
    }
}

fn frame_label(view: &FrameView) -> String {
    let name = view.function_name.as_deref().unwrap_or("<unknown>");
    if view.native {
        format!("native frame {name}")
    } else {
        format!("frame {name}")
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use bex_vm_types::{
        ConstValue, FunctionKind, Object, ObjectIndex, RealizedTy, Value,
        bytecode::{Bytecode, Instruction},
        types::TypeValue,
    };

    use crate::{
        BexVm, StackFrame, VmExecState, VmThreadState,
        vm::{
            Frame, FrameTypeMetadata, ThrowContext,
            tests::{native_function_object, test_vm},
        },
    };

    /// A VM parked in a bytecode frame (at a `SendEvent` yield).
    fn parked_vm() -> BexVm {
        let Object::Function(mut function) = native_function_object() else {
            unreachable!()
        };
        function.kind = FunctionKind::Bytecode;
        function.bytecode = Bytecode {
            instructions: vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::SendEvent,
                Instruction::Jump(-3),
            ],
            constants: vec![
                ConstValue::Object(ObjectIndex::from_raw(1)),
                ConstValue::Int(42),
            ],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        let mut vm = test_vm(vec![
            Object::Function(function),
            Object::String("tick".into()),
        ]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.set_entry_point(entry, &[]);
        assert!(matches!(vm.exec().unwrap(), VmExecState::Event { .. }));
        vm
    }

    /// The pieces of VM state that only `vm.rs` could reach before: a frame's
    /// exact runtime-type metadata and the three throw bookkeeping vectors.
    #[test]
    fn type_metadata_and_throw_bookkeeping_are_exported_and_imported() {
        let mut source = parked_vm();
        let Some(Frame::Bytecode(frame)) = source.frames.last_mut() else {
            panic!("expected a bytecode frame");
        };
        frame.type_args = vec![RealizedTy::Int, RealizedTy::String];
        frame.type_metadata = Some(Box::new(FrameTypeMetadata {
            values: vec![Some(TypeValue::new(RealizedTy::Int)), None],
        }));
        let trace: Arc<[StackFrame]> = Arc::from(vec![StackFrame {
            function_name: "user.f".to_string(),
            file_path: "f.baml".to_string(),
            function_span: baml_type::Span::default(),
            error_line: 7,
        }]);
        source
            .thrown_value_causes
            .push((Value::int(1), Value::int(2)));
        source.thrown_value_contexts.push((
            Value::int(1),
            ThrowContext {
                trace: Arc::clone(&trace),
                cause: Value::int(2),
            },
        ));
        source.preserved_throw_contexts.push((
            Value::int(3),
            ThrowContext {
                trace,
                cause: Value::NULL,
            },
        ));

        let state = source.export_thread_state().expect("exports");
        let exported = &state.frames[0];
        assert_eq!(exported.type_metadata.as_ref().map(Vec::len), Some(2));
        assert_eq!(
            state.thrown_value_causes,
            vec![(Value::int(1), Value::int(2))]
        );
        assert_eq!(state.thrown_value_contexts[0].1.trace[0].error_line, 7);
        assert_eq!(state.preserved_throw_contexts[0].0, Value::int(3));

        // The compile-time function pointer differs per heap; a real restore
        // translates it. Here the second VM's own pointer stands in.
        let mut target = parked_vm();
        let function = match target.frames.pop() {
            Some(Frame::Bytecode(frame)) => frame.function,
            _ => panic!("expected a bytecode frame"),
        };
        target.frames.clear();
        target.stack.0.clear();
        let mut state = state;
        state.frames[0].function = function;
        target.import_thread_state(state).expect("imports");

        let Some(Frame::Bytecode(frame)) = target.frames.last() else {
            panic!("expected a bytecode frame");
        };
        let metadata = frame.type_metadata.as_ref().expect("metadata is restored");
        assert!(matches!(
            metadata.values.as_slice(),
            [
                Some(TypeValue {
                    ty: RealizedTy::Int
                }),
                None
            ]
        ));
        assert_eq!(
            target.thrown_value_causes,
            vec![(Value::int(1), Value::int(2))]
        );
        assert_eq!(target.thrown_value_contexts.len(), 1);
        assert_eq!(
            target.thrown_value_contexts[0].1.trace[0].function_name,
            "user.f"
        );
        assert_eq!(target.thrown_value_contexts[0].1.cause, Value::int(2));
        assert_eq!(target.preserved_throw_contexts.len(), 1);

        // A state whose metadata does not line up with its type arguments is
        // refused.
        let mut broken = target.export_thread_state().expect("exports");
        broken.frames[0].type_metadata = Some(vec![None]);
        let mut fresh = parked_vm();
        fresh.frames.clear();
        fresh.stack.0.clear();
        let error = fresh.import_thread_state(broken).unwrap_err();
        assert!(error.contains("exact type arguments"), "{error}");
    }
    /// A continuation without a snapshot form, standing in for the ones that
    /// stay process-bound (runtime compilation and test registration).
    struct ProcessBound;

    impl crate::package_baml::Continuation for ProcessBound {
        fn call(
            self: Box<Self>,
            _vm: &mut BexVm,
            value: Value,
        ) -> crate::package_baml::NativeCallResult {
            crate::package_baml::NativeCallResult::Done(value)
        }
        fn gc_roots(&self) -> Vec<bex_vm_types::HeapPtr> {
            Vec::new()
        }
        fn apply_forwarding(
            &mut self,
            _: &std::collections::HashMap<bex_vm_types::HeapPtr, bex_vm_types::HeapPtr>,
        ) {
        }
    }

    /// A native frame whose continuation has no snapshot form blocks the
    /// export, and the message names the function and the continuation.
    #[test]
    fn a_continuation_without_a_snapshot_form_blocks_the_export_with_its_name() {
        let mut vm = parked_vm();
        let function = vm.frames[0].function();
        vm.frames.push(Frame::Native(crate::vm::NativeFrame {
            function,
            continuation: Box::new(ProcessBound),
        }));
        let error = vm.export_thread_state().unwrap_err();
        assert!(error.contains("native continuation"), "{error}");
        assert!(error.contains("ProcessBound"), "{error}");
        assert!(error.contains("test_fn") || error.contains('('), "{error}");
    }

    /// Every continuation with a snapshot form restores to a continuation
    /// that snapshots to the same state, and a payload that does not fit is
    /// refused instead of being installed.
    #[test]
    fn continuation_states_round_trip_through_the_registry() {
        use crate::package_baml::{ContinuationState, restore_continuation};

        let state = |tag: &str, values: u32, ints: &[u64], strings: &[&str], types: usize| {
            ContinuationState {
                tag: tag.to_string(),
                ptrs: vec![bex_vm_types::HeapPtr::null()],
                values: (0..values).map(|n| Value::int(i64::from(n))).collect(),
                types: vec![RealizedTy::Int; types],
                ints: ints.to_vec(),
                strings: strings.iter().map(ToString::to_string).collect(),
            }
        };
        let good = [
            state("array.map", 4, &[3, 1, 1, 0], &[], 1),
            state("array.filter", 3, &[3, 0, 0, 0], &[], 1),
            state("array.flat_map", 5, &[2, 3, 1, 0], &[], 1),
            state("array.some", 2, &[2, 0, 1, 0], &[], 0),
            state("array.every", 2, &[2, 0, 0, 0], &[], 0),
            state("array.find", 2, &[2, 0, 1, 1], &[], 0),
            state("array.find_last", 2, &[2, 0, 0, 0], &[], 0),
            state("array.reduce", 3, &[3, 0, 2, 0], &[], 0),
            state("array.sort_by", 6, &[4, 1, 1, 0, 1, 2, 2, 3], &[], 0),
            state("json.from_json_class", 3, &[1, 2, 1, 1], &[], 3),
            state("json.from_json_list", 3, &[2, 1, 1], &[], 1),
            state("json.from_json_map", 3, &[2, 1, 1], &["a", "b", "a"], 1),
            state("ops.equals", 4, &[], &[], 0),
        ];
        for mut expected in good {
            if expected.tag == "ops.equals" {
                expected.ptrs = Vec::new();
            }
            if expected.tag.starts_with("json.from_json_list")
                || expected.tag.starts_with("json.from_json_map")
            {
                expected.ptrs = Vec::new();
            }
            let restored = restore_continuation(&expected)
                .unwrap_or_else(|error| panic!("{}: {error}", expected.tag));
            let again = restored.snapshot().expect("still has a snapshot form");
            assert_eq!(again.tag, expected.tag);
            assert_eq!(again.values, expected.values, "{}", expected.tag);
            assert_eq!(again.ints, expected.ints, "{}", expected.tag);
            assert_eq!(again.strings, expected.strings, "{}", expected.tag);
            assert_eq!(again.types.len(), expected.types.len(), "{}", expected.tag);
        }

        // Walks keep their results as text: JSON for `to_json`, strings for
        // `to_string` and the prompt assembly.
        let mut walk = state("json.to_json_walk", 1, &[], &["{\"a\":[1,null]}"], 0);
        walk.ptrs = vec![bex_vm_types::HeapPtr::null(); 2];
        let again = restore_continuation(&walk).unwrap().snapshot().unwrap();
        assert_eq!(again.strings, walk.strings);
        let mut walk = state("root.to_string_walk", 1, &[], &["done"], 0);
        walk.ptrs = vec![bex_vm_types::HeapPtr::null(); 2];
        assert!(restore_continuation(&walk).is_ok());
        let mut prompt = state("prompt.assembly", 3, &[1, 2], &[], 0);
        prompt.ptrs = vec![bex_vm_types::HeapPtr::null(); 1];
        assert!(restore_continuation(&prompt).is_ok());
        assert!(restore_continuation(&state("pass_through", 0, &[], &[], 0)).is_ok());
        assert!(restore_continuation(&state("json.from_json_identity", 0, &[], &[], 0)).is_ok());
        assert!(restore_continuation(&state("reflect.call_any", 0, &[], &[], 1)).is_ok());

        let bad = [
            // A cursor outside the array.
            state("array.map", 4, &[3, 1, 3, 0], &[], 1),
            // Lengths that do not add up to the values.
            state("array.filter", 3, &[5, 0, 0, 0], &[], 1),
            // Merge cursors outside the source.
            state("array.sort_by", 6, &[4, 1, 1, 0, 1, 2, 2, 9], &[], 0),
            // As many results as overrides: no override is outstanding.
            state("root.to_string_walk", 1, &[], &["done"], 0),
            state("json.from_json_map", 3, &[2, 1, 1], &["a"], 1),
            state("ops.equals", 3, &[], &[], 0),
            state("array.map", 4, &[3, 1, 1, 0], &[], 0),
            state("no.such.continuation", 0, &[], &[], 0),
            // Results that do not match the cursor: `map` has one result per
            // finished element, `filter` at most one.
            state("array.map", 5, &[3, 2, 1, 0], &[], 1),
            state("array.filter", 6, &[3, 3, 1, 0], &[], 1),
            state("json.from_json_list", 5, &[3, 2, 1], &[], 1),
            state("json.from_json_class", 3, &[0, 2, 1, 0], &[], 2),
        ];
        for broken in bad {
            assert!(
                restore_continuation(&broken).is_err(),
                "{} must be refused",
                broken.tag
            );
        }
    }

    /// A fresh VM of the `parked_vm` program, plus the state of a parked one
    /// with the function pointer of the fresh VM.
    fn fresh_vm_and_state() -> (BexVm, VmThreadState) {
        let source = parked_vm();
        let mut state = source.export_thread_state().expect("exports");
        let mut fresh = parked_vm();
        let function = match fresh.frames.pop() {
            Some(Frame::Bytecode(frame)) => frame.function,
            _ => panic!("expected a bytecode frame"),
        };
        fresh.frames.clear();
        fresh.stack.0.clear();
        state.frames[0].function = function;
        (fresh, state)
    }

    /// The interpreter reads code and local slots without bounds checks, so
    /// the import refuses a program counter at the end of the code, a
    /// call-site program counter inside an instruction, and a stack that is
    /// shorter than a frame's locals region.
    #[test]
    fn program_counters_and_frame_extents_are_checked() {
        let (mut vm, state) = fresh_vm_and_state();
        vm.import_thread_state(state.clone())
            .expect("the exported state imports");

        let code_len = {
            let function = vm
                .snapshot_function_of(state.frames[0].function)
                .expect("function");
            super::code_len(function)
        };

        let (mut vm, mut broken) = fresh_vm_and_state();
        broken.frames[0].instruction_ptr = code_len;
        let error = vm.import_thread_state(broken).unwrap_err();
        assert!(error.contains("outside its code"), "{error}");

        // `LoadConst` has an operand, so offset 1 is inside an instruction.
        let (mut vm, mut broken) = fresh_vm_and_state();
        broken.frames[0].faulting_pc = 1;
        let error = vm.import_thread_state(broken).unwrap_err();
        assert!(error.contains("not the start"), "{error}");

        let (mut vm, mut broken) = fresh_vm_and_state();
        broken.frames[0].locals_offset = broken.stack.len() + 1;
        let error = vm.import_thread_state(broken).unwrap_err();
        assert!(error.contains("do not fit"), "{error}");
    }

    /// `cur_pc` is a diagnostic register that the first dispatched instruction
    /// overwrites. A value outside the top function is replaced by the frame's
    /// next program counter, and the state is accepted.
    #[test]
    fn a_live_program_counter_outside_the_top_function_is_normalized() {
        let (mut vm, mut state) = fresh_vm_and_state();
        let next = state.frames[0].instruction_ptr;
        state.cur_pc = 10_000;
        vm.import_thread_state(state).expect("imports");
        assert_eq!(vm.cur_pc, next);
    }
}
