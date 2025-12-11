use std::collections::HashMap;

use baml_types::{BamlMap, BamlMedia};
use baml_viz_events::{parse_path_segment, PathSegment, VizExecDelta, VizExecEvent};

use crate::{
    bytecode::{BinOp, CmpOp, Instruction},
    errors::{ErrorLocation, InternalError, RuntimeError, VmError},
    indexable::{EvalStack, GlobalPool, ObjectIndex, ObjectPool, StackIndex},
    types::{
        Function, FunctionKind, FunctionType, Future, FutureKind, FutureType, Instance, Object,
        ObjectType, PendingFuture, Type, Value, Variant,
    },
    watch::{self, NodeId, RootState, Watch, WatchFilter},
    StackTrace, UnaryOp,
};

/// Max call stack size.
pub const MAX_FRAMES: usize = 256;

/// Call frame.
///
/// This is what gets pushed onto the call stack every time we call a function.
///
/// As with [`Value`], this struct should not own allocated objects (like
/// functions) but instead use references to index into [`Vm::objects`]. Should
/// be [`Copy`].
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// The running function.
    pub function: ObjectIndex,

    /// Instruction pointer (IP) or program counter (PC).
    ///
    /// Points to the next instruction that the VM will execute. It is of type
    /// [`isize`] because some jumps can create negative offsets (for loops)
    /// and it's easier to operate on an [`isize`] and cast it to [`usize`]
    /// only once (when we index into [`Bytecode::instructions`]). However,
    /// this number should never be negative, otherwise indexing into the
    /// instruction vec will throw [`InternalError::NegativeInstructionPtr`].
    pub instruction_ptr: isize,

    /// Local variables offset in the eval stack.
    pub locals_offset: StackIndex,
}

fn build_viz_exec_event(
    function: &Function,
    index: usize,
    delta: VizExecDelta,
) -> Result<VizExecEvent, VmError> {
    let Some(node) = function.viz_nodes.get(index) else {
        return Err(InternalError::ArrayIndexOutOfBounds {
            index,
            length: function.viz_nodes.len(),
        }
        .into());
    };

    let path_segment = parse_node_segment(&node.log_filter_key)
        .unwrap_or(PathSegment::FunctionRoot { ordinal: 0 });

    Ok(VizExecEvent {
        event: delta,
        node_id: node.node_id,
        node_type: node.node_type.clone(),
        path_segment,
        label: node.label.clone(),
        header_level: node.header_level,
    })
}

fn parse_node_segment(log_filter_key: &str) -> Option<PathSegment> {
    // log_filter_key is encoded as "<function>|<segment>|<segment>..."
    let segment = log_filter_key.rsplit('|').next().unwrap_or(log_filter_key);
    parse_path_segment(segment)
}

/// The beast.
///
/// This is a stack based virtual machine. Stack based machines work by pushing
/// and popping values from an "evaluation stack". Picture this example from
/// [Crafting Interpreters](https://craftinginterpreters.com/a-virtual-machine.html):
///
/// ```ignore
/// fn echo(n) {
///     print(n)
///     return n
/// }
///
/// print(echo(echo(1) + echo(2)) + echo(echo(4) + echo(5)))
/// ```
///
/// Output should be:
///
/// ```text
/// 1
/// 2
/// 3
/// 4
/// 5
/// 9
/// 12
/// ```
///
/// The code above would create an AST similar to this:
///
/// ```text
///                 +-------+
///                 | print |
///                 +-------+
///                     |
///                   +---+
///          +--------| + |--------+
///          |        +---+        |
///      +------+               +------+
///      | echo |               | echo |
///      +------+               +------+
///          |                     |
///        +---+                 +---+
///        | + |                 | + |
///        +---+                 +---+
///          |                     |
///     +---------+           +----------+
///     |         |           |          |
/// +------+   +------+   +------+   +------+
/// | echo |   | echo |   | echo |   | echo |
/// +------+   +------+   +------+   +------+
///     |         |           |          |
///   +---+     +---+       +---+      +---+
///   | 1 |     | 2 |       | 4 |      | 5 |
///   +---+     +---+       +---+      +---+
/// ```
///
/// If we "flatten" the AST considering the "lifetime" of each value, we get
/// this structure:
///
/// ```text
///                   +---+
/// constant 1 ...... | 1 |
/// echo(1) ......... |   |---+
/// constant 2 ...... |   | 2 |
/// echo(2) ......... |   |   |
///                   +---+---+
/// add 1+2 ......... | 3 |
/// echo(3) ......... |   |---+
/// constant 4 ...... |   | 4 |
/// echo(4) ......... |   |   |---+
/// constant 5 ...... |   |   | 5 |
/// echo(5) ......... |   |   |   |
///                   |   |---+---+
/// add 4+5 ......... |   | 9 |
/// echo(9) ......... |   |   |
///                   +---+---+
/// add 3+9 ......... |12 |
/// print(12) ....... |   |
///                   +---+
/// ```
///
/// Looks like a stack doesn't it? That's the evaluation stack. All values in
/// the program flow through that stack, eliminating the need for instructions
/// with registers. Instead of `ADD r2, r0, r1` we just have `ADD`, which pops
/// two values from the stack, produces the result and pushes it back on top.
/// Simple, right? The drawback is that we need to execute more instructions to
/// achieve the same result as a register based VM. If we want to add two
/// variables, a register VM would run a single instruction:
///
/// ```text
/// ADD r2, r0, r1  // Add the contents of r0 and r1 and store the result in r2
///                 // r2 = r0 + r1
/// ```
///
/// Meanwhile a stack VM would run 4 instructions:
///
/// ```text
/// LOAD_VAR 0   // Push the contents of variable 0 on top of the stack
/// LOAD_VAR 1   // Push the contents of variable 1 on top of the stack
/// ADD          // Pop two values, add and push the result on top of the stack
/// STORE_VAR 2  // Store the top of the stack in variable 2
/// ```
///
/// Basically it's slower because it needs more cycles to do the same thing.
/// Other than that, pretty much everything is better in a stack VM, especially
/// simplicity (we don't even need to figure out which registers to use and when
/// to use them).
pub struct Vm {
    /// Call stack.
    ///
    /// On each function call we create a new [`Frame`] and push it on this
    /// stack. On each return, we destroy the frame and pop it from the stack
    /// to resume the execution of the previous frame.
    pub frames: Vec<Frame>,

    /// Evaluation stack.
    ///
    /// This stack only stores values.
    pub stack: EvalStack,

    /// Object pool.
    ///
    /// For now, since we don't have a garbage collector yet, this is basically
    /// an arena of objects. **Every object** is allocated here and will be
    /// destroyed when the lifetime of the [`Vm`] ends. Do not allocate objects
    /// elsewhere since that will make adding a garbage collector harder.
    /// Only allocate objects here and use indices to reference them, don't
    /// bother with Rust references because they will introduce lifetime issues.
    pub objects: ObjectPool,

    /// Global variables.
    ///
    /// This stores the functions and globally declared variables.
    pub globals: GlobalPool,

    /// Offset of the first runtime allocated object.
    ///
    /// This is used to track the index of the first runtime allocated object.
    /// When the embedder calls [`Vm::collect_garbage`] it will drop all values
    /// after this offset.
    pub runtime_allocs_offset: ObjectIndex,

    /// Environment variables available during execution.
    pub env_vars: HashMap<String, String>,

    /// Emit dependency graph.
    pub watch: Watch,

    /// Tracks which local variables are watched (have @watch).
    pub watched_vars: HashMap<StackIndex, (String, String)>,

    pub interrupt_frame: Option<usize>,
}

/// VM execution state.
///
/// The virtual machine cannot deal with futures, so when when it stumbles upon
/// future creation instructions, it returns control flow to the embedder,
/// expecting the embedder to schedule the future and yield back the control
/// flow to the VM.
///
/// Similarly, when the VM encounters an await point, it returns control flow to
/// the embedder, expecting the embedder to await the future and fulfil it with
/// the final result before yielding back control flow to the VM.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq)]
pub enum VmExecState {
    /// VM cannot proceed. It is awaiting a pending future to complete.
    Await(ObjectIndex),

    /// VM notifies caller about a future that needs to be scheduled.
    ///
    /// Bytecode execution continues when control flow is handled back to the
    /// VM.
    ScheduleFuture(ObjectIndex),

    /// VM has completed the execution of all available bytecode.
    Complete(Value),

    /// Notify about watched variables.
    Notify(WatchNotification),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq)]
pub enum WatchNotification {
    Variables(Vec<watch::NodeId>),
    Viz {
        function_name: String,
        event: VizExecEvent,
    },
}

#[derive(Clone, Debug)]
pub struct BamlVmProgram {
    pub objects: ObjectPool,
    pub globals: GlobalPool,
    pub resolved_function_names: HashMap<String, (ObjectIndex, FunctionKind)>,
    pub resolved_class_names: HashMap<String, ObjectIndex>,
    pub resolved_enums_names: HashMap<String, ObjectIndex>,
}

impl Vm {
    pub fn new(
        BamlVmProgram {
            objects, globals, ..
        }: BamlVmProgram,
        env_vars: HashMap<String, String>,
    ) -> Self {
        Self {
            frames: Vec::new(),
            stack: EvalStack::new(),
            runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
            objects,
            globals,
            env_vars,
            watch: Watch::new(),
            watched_vars: HashMap::new(),
            interrupt_frame: None,
        }
    }

    /// Bootstraps the VM preparing the given function to run.
    pub fn set_entry_point(&mut self, function: ObjectIndex, args: &[Value]) {
        debug_assert!(
            matches!(self.objects[function], Object::Function(_)),
            "expect function as entry point, got {:?}",
            self.objects[function]
        );

        // TODO: Run collect_garbage in codegen after each function call.
        if self.objects.len() != self.runtime_allocs_offset.0 {
            eprintln!("WARNING: garbage collection did not run before setting a new entry point");
        }

        self.stack.push(Value::Object(function));
        self.stack.extend(args.iter().copied());

        self.frames.push(Frame {
            function,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        });
    }

    /// Restores the VM state and prepares it for the next execution.
    ///
    /// This is used to clear the stack and frames after execution.
    pub fn finalize(&mut self) {
        // If the VM returns correctly with VmExecState::Complete, the eval
        // stack and call stack should be empty.
        self.stack.clear();
        self.frames.clear();
        self.collect_garbage();
    }

    /// Returns a reference to the pending future.
    ///
    /// Returns [`InternalError::TypeError`] if the future is not pending, or not a future.
    pub fn pending_future(&self, future: ObjectIndex) -> Result<&PendingFuture, InternalError> {
        match &self.objects[future] {
            Object::Future(Future::Pending(future)) => Ok(future),
            other => Err(InternalError::TypeError {
                expected: FutureType::Pending.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    pub fn fulfil_future(
        &mut self,
        future_index: ObjectIndex,
        value: Value,
    ) -> Result<(), InternalError> {
        let Object::Future(future) = &mut self.objects[future_index] else {
            return Err(InternalError::TypeError {
                expected: FutureType::Any.into(),
                got: ObjectType::of(&self.objects[future_index]).into(),
            });
        };

        *future = Future::Ready(value);

        // At any given moment, the VM can only await a single future, because
        // we can only call the AWAIT instruction on a future on top of the
        // stack. If that future being await is fulfilled, we need to replace
        // the future on the stack with the ready value so that the next
        // instruction that the VM runs can use the value, not the future
        // object.
        if let Some(Value::Object(index)) = self.stack.last() {
            if *index == future_index {
                self.stack.pop();
                self.stack.push(value);
            }
        }

        Ok(())
    }

    /// Keeps only compile time necessary objects.
    ///
    /// Everything allocated while the program run is dropped.
    pub fn collect_garbage(&mut self) {
        self.objects.drain(self.runtime_allocs_offset..);
    }

    /// Allocates an array on the heap and returns it to the caller.
    pub fn alloc_array(&mut self, values: Vec<Value>) -> Value {
        Value::Object(self.objects.insert(Object::Array(values)))
    }

    pub fn alloc_map(&mut self, values: BamlMap<String, Value>) -> Value {
        Value::Object(self.objects.insert(Object::Map(values)))
    }

    pub fn alloc_string(&mut self, s: String) -> Value {
        Value::Object(self.objects.insert(Object::String(s)))
    }

    /// TODO: Seems to low level for an embedder, provide an API that takes
    /// class name and mapping of field name => value instead.
    pub fn alloc_instance(&mut self, class: ObjectIndex, fields: Vec<Value>) -> Value {
        Value::Object(
            self.objects
                .insert(Object::Instance(Instance { class, fields })),
        )
    }

    // TODO: Same problem as above. Ideally takes (&str, &str) instead.
    pub fn alloc_variant(&mut self, enm: ObjectIndex, index: usize) -> Value {
        Value::Object(self.objects.insert(Object::Variant(Variant { enm, index })))
    }

    pub fn alloc_media(&mut self, media: BamlMedia) -> Value {
        Value::Object(self.objects.insert(Object::Media(media)))
    }

    /// Builds a stack trace for the given error.
    ///
    /// The error is assumed to have happened wherever the instruction pointer
    /// was left at.
    ///
    /// TODO: Not a clean API for the caller, VM should ideally return some kind
    /// of error struct that contains the error and trace and this would not
    /// be needed. That requires some refactoring though.
    pub fn stack_trace(&self, error: VmError) -> StackTrace {
        let trace = self
            .frames
            .iter()
            .map(|frame| {
                let function = self.objects[frame.function].as_function()?;

                // VM increments instruction pointer as soon as it reads the
                // instruction. So in reality the error ocurred on the previous
                // instruction. The saturating sub is just in case the code has
                // a bug somewhere.
                let last_executed_instruction = frame.instruction_ptr.saturating_sub(1);

                Ok(ErrorLocation {
                    function_name: function.name.to_owned(),
                    function_span: function.span.to_owned(),
                    error_line: function.bytecode.source_lines[last_executed_instruction as usize],
                })
            })
            .collect::<Result<Vec<_>, VmError>>()
            .map_err(|e| {
                RuntimeError::Other(format!(
                    "internal error: Vm::stack_trace() failed to build stack trace: {e}\n\noriginal error: {error}"
                ))
            })
            .unwrap_or_default();

        StackTrace { error, trace }
    }

    /// Stops the execution of the current bytecode in favor of the given
    /// function
    ///
    /// When the new control flow ends (given functions pops from the stack)
    /// then the previosly running bytecode resumes execution.
    fn interrupt(
        &mut self,
        function_index: ObjectIndex,
        args: &[Value],
    ) -> Result<VmExecState, VmError> {
        if !matches!(&self.objects[function_index], Object::Function(_)) {
            return Err(RuntimeError::Other("Invalid interrupt function".to_string()).into());
        }

        // Index of the frame that starts the interrupt code.
        self.interrupt_frame = Some(self.frames.len());

        let locals_offset = self.stack.len();

        // Params.
        self.stack.push(Value::Object(function_index));
        self.stack.extend(args.iter().copied());

        // Push the new frame.
        self.frames.push(Frame {
            function: function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(locals_offset),
        });

        // Execute the interrupt code and return the result.
        self.exec()
    }

    // Runs filters and returns remaining notifications for the watched node.
    fn process_notifications(
        &mut self,
        watched_node: NodeId,
    ) -> Result<Vec<watch::NodeId>, VmError> {
        // Make a copy of all the roots that reach the watched node.
        let mut notifications = self.watch.copy_roots_reaching(watched_node);

        // Some notifications might be removed with filters,  we'll return this.
        let mut filtered_notifications = vec![];

        // Sort them by variables first. At the moment this is not really used
        // because we only have variables, at this point it's unlikely we will
        // implement notifications on objects (references), so we might be able
        // to get rid of this.
        notifications.sort_by(|a, b| match (a, b) {
            (NodeId::LocalVar(a), NodeId::LocalVar(b)) => a.cmp(b),
            (NodeId::LocalVar(_), NodeId::HeapObject(_)) => std::cmp::Ordering::Less,
            (NodeId::HeapObject(_), NodeId::LocalVar(_)) => std::cmp::Ordering::Greater,
            (NodeId::HeapObject(a), NodeId::HeapObject(b)) => a.cmp(b),
        });

        for notification in notifications {
            // The call to copy_roots_reaching() should always return valid
            // roots, so this should really be unreachable.
            let Some(state) = self.watch.root_state(notification) else {
                continue;
            };

            match state.filter {
                // Manual notify means skip this notification. If paused also skip
                WatchFilter::Manual | WatchFilter::Paused => continue,

                // Default filter is a basic diff. If the value has actually
                // changed, then notify.
                WatchFilter::Default => {
                    let Some(last_assigned) = state.last_assigned else {
                        filtered_notifications.push(notification);
                        continue;
                    };

                    match crate::native::deep_equals(self, &[last_assigned, state.value]) {
                        Ok(Value::Bool(b)) => {
                            if !b {
                                filtered_notifications.push(notification);
                            }
                        }

                        other => {
                            return Err(RuntimeError::Other(format!(
                                "Invalid deep equals result during watch: {other:?}"
                            ))
                            .into());
                        }
                    }
                }

                // Run user function to decide if we should notify.
                WatchFilter::Function(filter_func) => {
                    let mut result = self.interrupt(filter_func, &[state.value]);
                    loop {
                        match result {
                            Ok(VmExecState::Complete(Value::Bool(notify))) => {
                                if notify {
                                    filtered_notifications.push(notification);
                                }
                                break;
                            }
                            Ok(VmExecState::Notify(_)) => {
                                // Ignore viz/variable notifications produced by the filter and keep running it.
                                result = self.exec();
                            }
                            Ok(other) => {
                                return Err(RuntimeError::Other(format!(
                                    "Invalid filter function return: {other:?}"
                                ))
                                .into());
                            }
                            Err(err) => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(filtered_notifications)
    }

    /// When a watched node changes, we need to update the graph topology
    /// and copy the previous values of the affected roots.
    fn update_watched_node(
        &mut self,
        watched_node: NodeId,
        path: watch::Path,
        old_value: Value,
        new_value: Value,
    ) -> Result<(), VmError> {
        if let Value::Object(old) = old_value {
            self.watch
                .unlink_edge(watched_node, path.clone(), NodeId::HeapObject(old));
        }

        if let Value::Object(new) = new_value {
            self.watch
                .link_edge(watched_node, path, NodeId::HeapObject(new), &self.objects);
        }

        // Copy previous values.
        let mut old_roots_copies = vec![];

        for root in self.watch.copy_roots_reaching(watched_node) {
            if let Some(state) = self.watch.root_state(root) {
                let deep_copy = crate::native::deep_copy_object(self, &[state.value])?;
                old_roots_copies.push(deep_copy);
            }
        }

        for (root, old_value) in self
            .watch
            .copy_roots_reaching(watched_node)
            .iter()
            .zip(old_roots_copies)
        {
            if let Some(state) = self.watch.root_state_mut(*root) {
                state.last_assigned = Some(old_value);
                // current value has not really changes, top level object is the same.
            }
        }

        Ok(())
    }

    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    pub fn exec(&mut self) -> Result<VmExecState, VmError> {
        // Grab the last frame from the call stack.
        //
        // Note that [`Frame`] is [`Copy`], so in case the borrow checker
        // complains too much and you can't circumvent it then you can make a
        // local copy of the frame, modify it as needed, and then when we're
        // done with this frame store it back in the vector to persist changes.
        // It's a similar trick to what we've implemented in the cycle detection
        // algorithm. Take a look at the `strong_connect` function in the
        // `tarjan.rs` file.
        let Some(mut frame) = self.frames.last_mut() else {
            // This should actually return "Void" or () like Rust.
            return Ok(VmExecState::Complete(Value::Null));
        };

        // Grab a reference to the function object. We do this before the loop
        // because there's no need to run this on every single iteration. Read
        // the implementations of `Instruction::Call` and `Instruction::Return`
        // below.
        //
        // We do run into some issues/boilerplate, take a look at the impl of
        // `Instruction::AllocArray`. We can write a macro or something.
        let mut function = self.objects[frame.function].as_function()?;

        loop {
            // Current instruction pointer.
            let instruction_ptr = frame.instruction_ptr;

            // Move the frame's IP to the next instruction. We'll deal with
            // jump offsets later.
            frame.instruction_ptr += 1;

            // NOTE: `core::intrinsics::unlikely` is only available on nightly.
            // This branch is a big annoyance for small functions (like pushing the frame)
            // and gets smaller the bigger the function due to branch (mis)prediction.
            if instruction_ptr < 0 {
                return Err(InternalError::NegativeInstructionPtr(instruction_ptr).into());
            }

            // Runtime debugging information.
            // #[cfg(debug_assertions)]
            // {
            //     let stack = self
            //         .stack
            //         .iter()
            //         .map(|v| crate::debug::display_value(v, &self.objects))
            //         .collect::<Vec<_>>()
            //         .join(", ");
            //
            //     eprintln!("[{stack}]");
            //
            //     let (instruction, metadata) = crate::debug::display_instruction(
            //         instruction_ptr,
            //         function,
            //         &self.stack,
            //         &self.objects,
            //         &self.globals,
            //     );
            //
            //     eprintln!("{instruction} {metadata}");
            // }

            match function.bytecode.instructions[instruction_ptr as usize] {
                Instruction::VizEnter(_) | Instruction::VizExit(_) => {
                    let instruction = &function.bytecode.instructions[instruction_ptr as usize];
                    let (index, delta) = match instruction {
                        Instruction::VizEnter(index) => (*index, VizExecDelta::Enter),
                        Instruction::VizExit(index) => (*index, VizExecDelta::Exit),
                        _ => unreachable!("matched on viz instruction"),
                    };

                    let event = build_viz_exec_event(function, index, delta)?;

                    return Ok(VmExecState::Notify(WatchNotification::Viz {
                        function_name: function.name.clone(),
                        event,
                    }));
                }
                Instruction::LoadConst(index) => {
                    let value = &function.bytecode.constants[index];
                    self.stack.push(*value);
                }

                Instruction::LoadVar(index) => {
                    let value = self.stack[frame.locals_offset + index];
                    self.stack.push(value);
                }

                Instruction::StoreVar(index) => {
                    // Absolute index of the local variable.
                    let local_var_index = frame.locals_offset + index;

                    // New value.
                    let value = self.stack.ensure_pop()?;

                    // Old value being replaced.
                    let old_value = std::mem::replace(&mut self.stack[local_var_index], value);

                    // Check if this binding is emittable.
                    if self.watched_vars.contains_key(&local_var_index) {
                        // Node ID of the local variable in the emit graph.
                        let watched_node = NodeId::LocalVar(local_var_index);

                        // If we had a previous binding to an object, unlink it
                        // so it doesn't emit anymore.
                        if let Value::Object(old_node) = old_value {
                            self.watch.unlink_edge(
                                watched_node,
                                watch::Path::Binding,
                                NodeId::HeapObject(old_node),
                            );
                        }

                        // If we have a new binding, link it so it emits.
                        if let Value::Object(new_node) = value {
                            self.watch.link_edge(
                                watched_node,
                                watch::Path::Binding,
                                NodeId::HeapObject(new_node),
                                &self.objects,
                            );
                        }

                        let old_value_deep_copy =
                            crate::native::deep_copy_object(self, &[old_value])?;

                        if let Some(state) = self.watch.root_state_mut(watched_node) {
                            state.last_assigned = Some(old_value_deep_copy);
                            state.value = value;
                        }

                        let notifications = self.process_notifications(watched_node)?;

                        // borrow checker.
                        frame = self.frames.last_mut().expect("last_mut() must exist");
                        function = self.objects[frame.function].as_function()?;

                        if !notifications.is_empty() {
                            return Ok(VmExecState::Notify(WatchNotification::Variables(
                                notifications,
                            )));
                        }
                    }
                }

                Instruction::LoadGlobal(index) => {
                    let value = &self.globals[index];
                    self.stack.push(*value);
                }

                Instruction::StoreGlobal(index) => {
                    // Consume the value. Read impl of Instruction::StoreVar.
                    let value = self.stack.ensure_pop()?;

                    self.globals[index] = value;
                }

                Instruction::LoadField(index) => {
                    let top = self.stack.ensure_pop()?;

                    let reference = self.objects.as_object(&top, ObjectType::Instance)?;

                    let Object::Instance(instance) = &self.objects[reference] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(&self.objects[reference]).into(),
                        }
                        .into());
                    };

                    // Push the value on top of the stack.
                    self.stack.push(instance.fields[index]);
                }

                Instruction::StoreField(index) => {
                    // Consume the new value to be set from the stack.
                    let new_value = self.stack.ensure_pop()?;

                    // Consume the instance value from the stack.
                    let instance_index = self
                        .objects
                        .as_object(&self.stack.ensure_pop()?, ObjectType::Instance)?;

                    // Read old value (and typecheck).
                    let old_value = match &self.objects[instance_index] {
                        Object::Instance(instance) => instance.fields[index],

                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology.
                    let watched_node = NodeId::HeapObject(instance_index);

                    self.update_watched_node(
                        watched_node,
                        watch::Path::InstanceField(index),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Instance(instance) = &mut self.objects[instance_index] {
                        instance.fields[index] = new_value;
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // Borrow checker.
                    frame = self.frames.last_mut().expect("last_mut() must exist");
                    function = self.objects[frame.function].as_function()?;

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
                    }
                }

                Instruction::Pop(n) => {
                    let drain_start = self.stack.len() - n;
                    let drain_range = StackIndex::from_raw(drain_start)..;

                    // Check if any of the popped variables are emittable and
                    // unregister them
                    for i in drain_start..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going
                            // out of scope.
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable.
                            if let Value::Object(obj) = self.stack[index] {
                                self.watch.unlink_edge(
                                    var_node,
                                    watch::Path::Binding,
                                    NodeId::HeapObject(obj),
                                );
                            }
                        }
                    }

                    self.stack.drain(drain_range);
                }

                Instruction::Copy(offset) => {
                    let index = self.stack.ensure_slot_from_top(offset)?;
                    let value = self.stack[index];
                    self.stack.push(value);
                }

                Instruction::PopReplace(n) => {
                    let value = self.stack.ensure_pop()?;

                    // Pop the last `n` locals from the stack.
                    let drain_start = self.stack.len() - n;

                    // Clean up any emittable variables in the range being
                    // popped.
                    for i in drain_start..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going out of scope
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable
                            if let Value::Object(obj) = self.stack[index] {
                                self.watch.unlink_edge(
                                    var_node,
                                    watch::Path::Binding,
                                    NodeId::HeapObject(obj),
                                );
                            }
                        }
                    }

                    let drain_range = StackIndex::from_raw(drain_start)..;
                    self.stack.drain(drain_range);

                    // Push the value back on top of the stack.
                    self.stack.push(value);
                }

                Instruction::Jump(offset) => {
                    // Reassign the frame's IP to the new instruction.
                    // Remember that offset can be negative here, so even though
                    // we're adding it can still jump backwards.
                    frame.instruction_ptr = instruction_ptr + offset;
                }

                Instruction::JumpIfFalse(offset) => {
                    match &self.stack[self.stack.ensure_stack_top()?] {
                        // Reassign only if the top of the stack is false.
                        Value::Bool(value) => {
                            if !value {
                                frame.instruction_ptr = instruction_ptr + offset;
                            }
                        }

                        // Type error, we don't have "falsey" values in the language
                        // so we should always check booleans.
                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Bool,
                                got: self.objects.type_of(other),
                            }));
                        }
                    }
                }

                Instruction::BinOp(op) => {
                    let right = self.stack.ensure_pop()?;
                    let left = self.stack.ensure_pop()?;

                    let result = match (left, right) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(match op {
                            BinOp::Div if right == 0 => {
                                return Err(RuntimeError::DivisionByZero {
                                    left: Value::Int(left),
                                    right: Value::Int(right),
                                }
                                .into());
                            }

                            BinOp::Add => left + right,
                            BinOp::Sub => left - right,
                            BinOp::Mul => left * right,
                            BinOp::Div => left / right,
                            BinOp::Mod => left % right,

                            BinOp::BitAnd => left & right,
                            BinOp::BitOr => left | right,
                            BinOp::BitXor => left ^ right,
                            BinOp::Shl => left << right,
                            BinOp::Shr => left >> right,
                        }),

                        (Value::Float(left), Value::Float(right)) => {
                            Value::Float(match op {
                                BinOp::Div if right == 0.0 => {
                                    return Err(RuntimeError::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    }
                                    .into());
                                }

                                BinOp::Add => left + right,
                                BinOp::Sub => left - right,
                                BinOp::Mul => left * right,
                                BinOp::Div => left / right,
                                BinOp::Mod => left % right,

                                // Bitwise ops not applicable to floats.
                                BinOp::BitAnd
                                | BinOp::BitOr
                                | BinOp::BitXor
                                | BinOp::Shl
                                | BinOp::Shr => {
                                    return Err(VmError::from(InternalError::CannotApplyBinOp {
                                        left: Type::Float,
                                        right: Type::Float,
                                        op,
                                    }));
                                }
                            })
                        }

                        (Value::Object(_), Value::Object(_)) if op == BinOp::Add => {
                            let left = self.objects.as_string(&left)?;
                            let right = self.objects.as_string(&right)?;

                            let mut concat = left.clone();
                            concat.push_str(right);

                            let concat_str_object =
                                Value::Object(self.objects.insert(Object::String(concat)));

                            // Borrow check.
                            function = self.objects[frame.function].as_function()?;

                            concat_str_object
                        }

                        _ => {
                            return Err(VmError::from(InternalError::CannotApplyBinOp {
                                left: self.objects.type_of(&left),
                                right: self.objects.type_of(&right),
                                op,
                            }));
                        }
                    };

                    self.stack.push(result);
                }

                Instruction::CmpOp(op) => {
                    let right = self.stack.ensure_pop()?;
                    let left = self.stack.ensure_pop()?;

                    let result = match (left, right) {
                        (Value::Int(left), Value::Int(right)) => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,

                            CmpOp::InstanceOf => {
                                return Err(InternalError::CannotApplyCmpOp {
                                    left: Type::Int,
                                    right: Type::Int,
                                    op,
                                }
                                .into());
                            }
                        }),

                        (Value::Float(left), Value::Float(right)) => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,

                            CmpOp::InstanceOf => {
                                return Err(InternalError::CannotApplyCmpOp {
                                    left: Type::Float,
                                    right: Type::Float,
                                    op,
                                }
                                .into());
                            }
                        }),

                        (Value::Object(left_index), Value::Object(right_index))
                            if matches!(self.objects[left_index], Object::String(_))
                                && matches!(self.objects[right_index], Object::String(_)) =>
                        {
                            let left = self.objects.as_string(&left)?;
                            let right = self.objects.as_string(&right)?;

                            Value::Bool(match op {
                                CmpOp::Eq => left == right,
                                CmpOp::NotEq => left != right,
                                CmpOp::Lt => left < right,
                                CmpOp::LtEq => left <= right,
                                CmpOp::Gt => left > right,
                                CmpOp::GtEq => left >= right,
                                CmpOp::InstanceOf => {
                                    return Err(InternalError::CannotApplyCmpOp {
                                        left: Type::Object(ObjectType::String),
                                        right: Type::Object(ObjectType::String),
                                        op,
                                    }
                                    .into());
                                }
                            })
                        }

                        _ => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,

                            CmpOp::InstanceOf => {
                                let left = self.objects.as_object(&left, ObjectType::Instance)?;

                                let Object::Instance(instance) = &self.objects[left] else {
                                    return Err(InternalError::TypeError {
                                        expected: ObjectType::Instance.into(),
                                        got: ObjectType::of(&self.objects[left]).into(),
                                    }
                                    .into());
                                };

                                let right = self.objects.as_object(&right, ObjectType::Class)?;

                                instance.class == right
                            }

                            _ => {
                                return Err(VmError::from(InternalError::CannotApplyCmpOp {
                                    left: self.objects.type_of(&left),
                                    right: self.objects.type_of(&right),
                                    op,
                                }));
                            }
                        }),
                    };

                    self.stack.push(result);
                }

                Instruction::UnaryOp(op) => {
                    let value = self.stack.ensure_pop()?;

                    let result = match (op, value) {
                        (UnaryOp::Not, Value::Bool(value)) => Value::Bool(!value),
                        (UnaryOp::Neg, Value::Int(value)) => Value::Int(-value),
                        (UnaryOp::Neg, Value::Float(value)) => Value::Float(-value),
                        _ => {
                            return Err(VmError::from(InternalError::CannotApplyUnaryOp {
                                op,
                                value: self.objects.type_of(&value),
                            }));
                        }
                    };

                    self.stack.push(result);
                }

                Instruction::AllocArray(size) => {
                    // Pop all the elements from the stack and create an array.
                    let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                    let array = self.stack.drain(drain_range).collect();

                    // Allocate it on the heap.
                    self.objects.push(Object::Array(array));

                    // Push the array object on top of the stack.
                    self.stack
                        .push(Value::Object(ObjectIndex::from_raw(self.objects.len() - 1)));

                    // objects.push() above might've reallocated the vector so
                    // borrow checker complains. Restore the reference.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::LoadArrayElement => {
                    // Stack should contain [array, index]
                    // Pop the index first, then the array
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;

                    let array_obj_index =
                        self.objects.as_object(&array_value, ObjectType::Array)?;

                    let Object::Array(array) = &self.objects[array_obj_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(&self.objects[array_obj_index]).into(),
                        }));
                    };

                    // Get the index
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        _ => {
                            return Err(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.objects.type_of(&index_value),
                            }
                            .into());
                        }
                    };

                    // Check bounds
                    if index >= array.len() {
                        return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                            index,
                            length: array.len(),
                        }));
                    }

                    // Push the element onto the stack
                    self.stack.push(array[index]);
                }

                Instruction::LoadMapElement => {
                    // LoadMapElement Instruction
                    //
                    // Stack before: [map, key]
                    // Stack after: [value]
                    //
                    // Interpretation steps:
                    // 1. Pop key from stack (top element)
                    // 2. Pop map reference from stack (bottom element)
                    // 3. Validate that the popped map reference is indeed a map object
                    // 4. Get the key as a string from the objects pool (maps use string keys)
                    //    - Validate key_value is an object reference to a String
                    //    - Get the string reference from the objects pool
                    // 5. Look up the value at map[key]
                    // 6. Handle the case where key doesn't exist in the map
                    //    - Return a runtime error NoSuchKeyInMap if key not found
                    // 7. Push the found value onto the stack

                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;

                    let map_index = self.objects.as_object(&map_value, ObjectType::Map)?;

                    let Object::Map(map) = &self.objects[map_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(&self.objects[map_index]).into(),
                        }));
                    };

                    // Get the string key from the objects pool
                    let key_index = self.objects.as_object(&key_value, ObjectType::String)?;
                    let key = self.objects[key_index].as_string()?;

                    // Look up the value in the map
                    let value = map.get(key).copied().ok_or(RuntimeError::NoSuchKeyInMap)?;

                    // Push the value onto the stack
                    self.stack.push(value);
                }

                Instruction::StoreArrayElement => {
                    // Instruction args.
                    let new_value = self.stack.ensure_pop()?;
                    let index_value = self.stack.ensure_pop()?;
                    let array_object_index = self
                        .objects
                        .as_object(&self.stack.ensure_pop()?, ObjectType::Array)?;

                    // Verify index.
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.objects.type_of(&other),
                            }));
                        }
                    };

                    // Read old value (and typecheck).
                    let old_value = match &self.objects[array_object_index] {
                        Object::Array(array) => {
                            // Check bounds.
                            if index >= array.len() {
                                return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                                    index,
                                    length: array.len(),
                                }));
                            }

                            array[index]
                        }

                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology
                    let watched_node = NodeId::HeapObject(array_object_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::ArrayIndex(index),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Array(array) = &mut self.objects[array_object_index] {
                        array[index] = new_value;
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // borrow checker.
                    frame = self.frames.last_mut().expect("last_mut() must exist");
                    function = self.objects[frame.function].as_function()?;

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
                    }
                }

                Instruction::StoreMapElement => {
                    // Instruction args.
                    let new_value = self.stack.ensure_pop()?;
                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;

                    // Get the string key from the objects pool.
                    let key_index = self.objects.as_object(&key_value, ObjectType::String)?;
                    let key = self.objects[key_index].as_string()?.clone();

                    let map_index = self.objects.as_object(&map_value, ObjectType::Map)?;

                    // Read old value (and typecheck).
                    //
                    // If the map didn't contain any value we'll use null so
                    // there's not watch graph edge to update.
                    let old_value = match &self.objects[map_index] {
                        Object::Map(map) => map.get(&key).copied().unwrap_or(Value::Null),

                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology
                    let watched_node = NodeId::HeapObject(map_index);

                    self.update_watched_node(
                        watched_node,
                        watch::Path::MapKey(key.clone()),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Map(map) = &mut self.objects[map_index] {
                        map.insert(key, new_value);
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // borrow checker.
                    frame = self.frames.last_mut().expect("last_mut() must exist");
                    function = self.objects[frame.function].as_function()?;

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
                    }
                }

                Instruction::AllocInstance(index) => {
                    let Object::Class(class) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Class.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Allocate the fields.
                    let mut fields = Vec::with_capacity(class.field_names.len());
                    fields.resize(class.field_names.len(), Value::Null);

                    // Allocate an instance of the class.
                    self.objects.push(Object::Instance(Instance {
                        class: index,
                        fields,
                    }));

                    // Push the instance object on top of the stack.
                    self.stack
                        .push(Value::Object(ObjectIndex::from_raw(self.objects.len() - 1)));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }

                // TODO: Contains a lot of typechecking, we know at compile time
                // that all this stuff is right. Should do something about it.
                Instruction::AllocVariant(enum_index) => {
                    let Object::Enum(enm) = &self.objects[enum_index] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Enum.into(),
                            got: ObjectType::of(&self.objects[enum_index]).into(),
                        }
                        .into());
                    };

                    let variant = self.stack.ensure_pop()?;

                    let Value::Int(variant_index) = variant else {
                        return Err(InternalError::TypeError {
                            expected: Type::Int,
                            got: self.objects.type_of(&variant),
                        }
                        .into());
                    };

                    if variant_index < 0 {
                        return Err(InternalError::ArrayIndexIsNegative(variant_index).into());
                    }

                    if variant_index as usize >= enm.variant_names.len() {
                        return Err(InternalError::ArrayIndexOutOfBounds {
                            index: variant_index as usize,
                            length: enm.variant_names.len(),
                        }
                        .into());
                    }

                    let object_index = self.objects.insert(Object::Variant(Variant {
                        enm: enum_index,
                        index: variant_index as usize,
                    }));

                    // Push the variant object on top of the stack.
                    self.stack.push(Value::Object(object_index));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::DispatchFuture(arg_count) => {
                    let args_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    let expected_type = FunctionType::Llm;

                    let index = self
                        .objects
                        .as_object(&self.stack[args_offset], expected_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callable_future) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: expected_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != callable_future.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: callable_future.arity,
                            got: arg_count,
                        }));
                    }

                    // Not a future.
                    if !matches!(
                        callable_future.kind,
                        FunctionKind::Llm | FunctionKind::Future
                    ) {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: FunctionType::Llm.into(), // TODO: Fix this
                            got: FunctionType::from(callable_future.kind).into(),
                        }));
                    }

                    // Collect the function call args and cleanup the call.
                    let future_args = self.stack.drain(args_offset..).skip(1).collect();

                    // Create the pending future.
                    let pending_future = PendingFuture {
                        function: callable_future.name.clone(),
                        args: future_args,
                        kind: match callable_future.kind {
                            FunctionKind::Llm => FutureKind::Llm,
                            FunctionKind::Future => FutureKind::Net,
                            _ => unreachable!(),
                        },
                    };

                    // Allocate the future.
                    let object_index = self
                        .objects
                        .insert(Object::Future(Future::Pending(pending_future)));

                    // Now leave the future on top of the stack.
                    self.stack.push(Value::Object(object_index));

                    // Yield control flow back to the embedder.
                    return Ok(VmExecState::ScheduleFuture(object_index));
                }

                Instruction::Await => {
                    let value = self.stack.ensure_stack_top()?;

                    let wanted_type = FutureType::Any;

                    let index = self
                        .objects
                        .as_object(&self.stack[value], wanted_type.into())?;

                    let Object::Future(awaiting) = &self.objects[index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: wanted_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }));
                    };

                    match awaiting {
                        // Can't do nothing, handle control flow back to embedder.
                        Future::Pending(_) => {
                            return Ok(VmExecState::Await(index));
                        }

                        // Replace the future on the eval stack with the ready
                        // value.
                        Future::Ready(value) => {
                            self.stack.pop();
                            self.stack.push(*value);
                        }
                    }
                }

                Instruction::Watch(index) => {
                    // Stack contains: [channel, filter]

                    // Consume filter.
                    let filter = match self.stack.ensure_pop()? {
                        Value::Null => WatchFilter::Default,
                        Value::Object(object_index) => match &self.objects[object_index] {
                            Object::Function(_) => WatchFilter::Function(object_index),
                            Object::String(mode) if mode == "manual" => WatchFilter::Manual,
                            Object::String(mode) if mode == "never" => WatchFilter::Paused,
                            _ => {
                                return Err(RuntimeError::Other("Invalid filter".to_string()).into())
                            }
                        },
                        _ => return Err(RuntimeError::Other("Invalid filter".to_string()).into()),
                    };

                    // Consume channel.
                    let channel = self
                        .objects
                        .as_string(&self.stack.ensure_pop()?)?
                        .to_owned();

                    let local_var_index = StackIndex::from_raw(frame.locals_offset.raw() + index);
                    let value = self.stack[local_var_index];

                    // The variable index should be the same as where the value is stored
                    let var_node = NodeId::LocalVar(local_var_index);

                    // Register this variable as an emittable root.
                    self.watch.register_root(
                        var_node,
                        RootState {
                            channel,
                            value,
                            filter,
                            last_notified: None,
                            last_assigned: None,
                        },
                    );

                    let watched_var_name = &function.locals_in_scope
                        [function.bytecode.scopes[instruction_ptr as usize]][index];
                    // Track this so we can unregister on scope exit
                    self.watched_vars.insert(
                        local_var_index,
                        (watched_var_name.to_string(), function.name.clone()),
                    );

                    // If it's an object, build the entire dependency graph
                    if let Value::Object(object_index) = value {
                        // Build the graph.

                        // Link the root emittable variable to the object
                        self.watch.link_edge(
                            var_node,
                            watch::Path::Binding,
                            NodeId::HeapObject(object_index),
                            &self.objects,
                        );
                    }
                }

                Instruction::Notify(index) => {
                    let local_var_index = StackIndex::from_raw(frame.locals_offset.raw() + index);
                    let var_node = NodeId::LocalVar(local_var_index);

                    let notifications = self.watch.copy_roots_reaching(var_node);

                    if notifications.len() != 1 && notifications.first() != Some(&var_node) {
                        return Err(RuntimeError::Other("Invalid manual notify".to_string()).into());
                    }

                    return Ok(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    )));
                }

                Instruction::Call(arg_count) => {
                    // Function calls are pushed onto the stack like this:
                    //
                    // [callee, arg1, arg2, ..., argN]
                    //
                    // The callee is a ref to the function object, and after
                    // that we have all the function arguments. `arg_count` is
                    // the number of arguments pushed after the callee.
                    //
                    // That's how we compute the relative offset of the callee
                    // and it's local args in the stack.
                    let locals_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    // Get the function object from the stack.
                    let local = &self.stack[locals_offset];

                    let function_type = FunctionType::Callable;

                    let index = self.objects.as_object(local, function_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callee) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: function_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != callee.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: callee.arity,
                            got: arg_count,
                        }));
                    }

                    // Check if we've reached the max call stack size.
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
                    }

                    match callee.kind {
                        FunctionKind::Native(func) => {
                            // NOTE: (perf) could use drain(..) instead, or even maintain the arguments
                            // reference in the stack, using `swap` to insert the result.
                            let args =
                                self.stack[StackIndex::from_raw(locals_offset.0 + 1)..].to_owned();

                            // Run Rust native function.
                            let result = func(self, &args)?;

                            // Drop function call and place result on top.
                            self.stack.drain(locals_offset..);
                            self.stack.push(result);

                            // Rust borrow check workaround because we're passing VM as
                            // mut and technically the frame pointer could be
                            // invalidated. Frame is Copy so we can maintain a
                            // local owned copy to avoid this but then we'd need
                            // to presist changes when moving to a new frame.
                            //
                            // We use `ObjectIndex` constructor directly because we know it's a
                            // valid reference (we are executing instructions inside of it).
                            frame = self.frames.last_mut().expect("last_mut() was pushed above");
                            function = self.objects[frame.function].as_function()?;
                        }

                        FunctionKind::Exec => {
                            // Otherwise push the new frame.
                            self.frames.push(Frame {
                                function: index,
                                instruction_ptr: 0,
                                locals_offset,
                            });

                            // Point to next frame.
                            frame = self.frames.last_mut().expect("last_mut() was pushed above");

                            // Grab function ref. We do this to avoid running this
                            // code at the beginning of each iteration since it's
                            // totaly unnecessary. The function only changes when the
                            // frame changes.
                            function = self.objects[frame.function].as_function()?;
                        }

                        FunctionKind::Llm | FunctionKind::Future => {
                            return Err(InternalError::TypeError {
                                expected: FunctionType::Callable.into(),
                                got: FunctionType::from(callee.kind).into(),
                            }
                            .into());
                        }
                    }
                }

                Instruction::Return => {
                    // Pop the result from the eval stack.
                    let result = self.stack.ensure_pop()?;

                    // Clean up any emittable variables in the function's scope
                    for i in frame.locals_offset.0..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going out of scope
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable
                            if i < self.stack.len() {
                                if let Value::Object(obj) = self.stack[index] {
                                    self.watch.unlink_edge(
                                        var_node,
                                        watch::Path::Binding,
                                        NodeId::HeapObject(obj),
                                    );
                                }
                            }
                        }
                    }

                    // Restore the eval stack to the state before the function
                    // was called and leave the result on top.
                    self.stack.drain(frame.locals_offset..);
                    self.stack.push(result);

                    // Pop from the call stack.
                    self.frames.pop();

                    // Return from interrupt.
                    if Some(self.frames.len()) == self.interrupt_frame {
                        self.interrupt_frame = None;
                        return self
                            .stack
                            .ensure_pop()
                            .map(VmExecState::Complete)
                            .map_err(Into::into);
                    }

                    // If there are no more frames, we're done.
                    let Some(previous_frame) = self.frames.last_mut() else {
                        return self
                            .stack
                            .ensure_pop()
                            .map(VmExecState::Complete)
                            .map_err(Into::into);
                    };

                    // Resume previous frame execution.
                    frame = previous_frame;

                    // Point to the previous frame's function. Read the
                    // implementation of `Instruction::Call` above this one for
                    // more information about this piece.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::Assert => {
                    let value = self.stack.pop().ok_or(RuntimeError::AssertionError)?;

                    let Value::Bool(condition_result) = value else {
                        return Err(InternalError::TypeError {
                            expected: Type::Bool,
                            got: self.objects.type_of(&value),
                        }
                        .into());
                    };

                    if !condition_result {
                        return Err(RuntimeError::AssertionError.into());
                    }
                }

                Instruction::AllocMap(n) => {
                    let map = if n > 0 {
                        let end_of_values = self.stack.ensure_slot_from_top(2 * n - 1)?;
                        let end_of_keys = self.stack.ensure_slot_from_top(n - 1)?;
                        let idx_of_last_key = self.stack.ensure_slot_from_top(n - 1)?;

                        // We can safely copy the objects that act as values so there's no problem
                        // with not draining them.
                        let values = self.stack[end_of_values..end_of_keys].iter().copied();

                        // We cannot copy key references since we aren't interning yet, so we
                        // must clone the strings.
                        // Here we'll also double-check that the keys are strings. This adds `n`
                        // branches which is not ideal for performance. Might want to consider this
                        // in map accesses.
                        let keys = self.stack[idx_of_last_key..].iter().map(|k| {
                            let obj_index = self.objects.as_object(k, ObjectType::String)?;

                            self.objects[obj_index].as_string().cloned()
                        });

                        let pairs = values
                            .zip(keys)
                            .map(|(val, key_res)| key_res.map(|k| (k, val)));

                        let map = pairs.collect::<Result<BamlMap<_, _>, _>>()?;

                        // drain & drop the drain so that vec is empty.
                        self.stack.drain(end_of_values..);

                        map
                    } else {
                        // nothing to pop.
                        BamlMap::new()
                    };

                    let obj_index = self.objects.insert(Object::Map(map));

                    self.stack.push(Value::Object(obj_index));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }
            }
        }
    }
}
