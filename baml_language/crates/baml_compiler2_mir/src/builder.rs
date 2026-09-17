//! MIR Builder API.
//!
//! Provides a fluent interface for constructing MIR functions. The builder
//! manages local allocation, basic block creation, and ensures well-formed MIR.
//!
//! # Example
//!
//! ```ignore
//! let mut builder = MirBuilder::new(Name::new("my_function"), 1);
//!
//! // Declare return place and parameter
//! let ret = builder.declare_local(Some("_return".into()), RuntimeTy::Int, None);
//! let param = builder.declare_local(Some("x".into()), RuntimeTy::Int, None);
//!
//! // Create blocks
//! let entry = builder.create_block();
//! let exit = builder.create_block();
//!
//! builder.set_current_block(entry);
//! builder.assign(Place::local(ret), Rvalue::Use(Operand::copy_local(param)));
//! builder.goto(exit);
//!
//! builder.set_current_block(exit);
//! builder.return_();
//!
//! let mir = builder.build();
//! ```

use baml_base::{Name, Span};
use baml_type::{RuntimeTy, TyTemplate};

use crate::{
    BasicBlock, BlockId, Constant, ItemRef, Landing, Local, LocalDecl, MirFunction,
    MirFunctionBody, MirFunctionKind, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
};

/// Builder for constructing MIR functions.
pub(crate) struct MirBuilder<'db> {
    name: Name,
    arity: usize,
    blocks: Vec<BasicBlock<'db>>,
    locals: Vec<LocalDecl>,
    current_block: Option<BlockId>,
    span: Option<Span>,
    /// Current source span for tagging statements/terminators.
    pub(crate) current_source_span: Option<Span>,
    /// The handler a throw or panic at the current lowering position unwinds
    /// to: the innermost enclosing `catch` handler or `defer` landing pad.
    /// Every block is stamped with it at creation (`BasicBlock::unwind`), and
    /// changing it moves lowering to a fresh block (`transition_unwind`), so
    /// a block is always a maximal run of code under one handler. Every
    /// append checks that the current block was created under the value in
    /// force, so code can never land in a block whose handler differs from
    /// the position it was lowered at.
    current_unwind: Option<BlockId>,
    /// The innermost handler whose body is being lowered
    /// (`BasicBlock::handling`), stamped the same way.
    current_handling: Option<BlockId>,
    /// How many `defer` bodies the current lowering position is inside; a
    /// block is stamped `shielded` when this is non-zero
    /// (`BasicBlock::shielded`), with the same fresh-block discipline as the
    /// handler.
    shield_depth: u32,
}

/// Where lowering was before it moved out of line to fill a handler block;
/// [`MirBuilder::end_out_of_line`] returns there.
#[must_use = "lowering returns by handing this back to `end_out_of_line`"]
pub(crate) struct OutOfLine {
    block: BlockId,
    unwind: Option<BlockId>,
    handling: Option<BlockId>,
}

/// An open shield — a `defer` body being lowered. [`MirBuilder::enter_shield`]
/// opens one; handing it back to [`MirBuilder::leave_shield`] closes it, so a
/// shield cannot be closed twice, and one left open is a token never handed
/// back.
#[must_use = "a shield is closed by handing this back to `leave_shield`"]
pub(crate) struct Shield {
    /// The depth entered; shields close innermost first.
    depth: u32,
}

// Some builder utilities are not yet used but will be needed as MIR 2 matures.
#[allow(dead_code)]
impl<'db> MirBuilder<'db> {
    /// Create a new MIR builder for a function.
    pub(crate) fn new(name: Name, arity: usize) -> Self {
        Self {
            name,
            arity,
            blocks: Vec::new(),
            locals: Vec::new(),
            current_block: None,
            span: None,
            current_source_span: None,
            current_unwind: None,
            current_handling: None,
            shield_depth: 0,
        }
    }

    /// Return the function name.
    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    /// Set the source span for the function.
    pub(crate) fn set_span(&mut self, span: Span) {
        self.span = Some(span);
    }

    // ========================================================================
    // Local Management
    // ========================================================================

    /// Declare a new local variable or temporary.
    ///
    /// Returns the Local ID. Convention:
    /// - `_0` is the return place
    /// - `_1..=_n` are parameters (where n = arity)
    /// - `_n+1...` are user locals and temporaries
    pub(crate) fn declare_local(
        &mut self,
        name: Option<Name>,
        ty: RuntimeTy,
        span: Option<Span>,
    ) -> Local {
        let id = Local(self.locals.len());
        self.locals.push(LocalDecl {
            name,
            ty,
            span,
            scope_span: None,
            is_captured: false,
        });
        id
    }

    /// Declare a local a closure captures, and give it its cell.
    ///
    /// A non-parameter local is celled right here, in the current block: a
    /// cell is created where its binding is created, so a declaration that
    /// runs once per loop iteration hands each iteration's closures their own
    /// cell. Emit the initializing store after this, never before. A
    /// parameter (`_1..=_n`) is celled by the frame preamble instead, since the
    /// caller has already written its value into the slot.
    pub(crate) fn declare_captured_local(
        &mut self,
        name: Option<Name>,
        ty: RuntimeTy,
        span: Option<Span>,
    ) -> Local {
        let local = Local(self.locals.len());
        self.locals.push(LocalDecl {
            name,
            ty,
            span,
            scope_span: None,
            is_captured: true,
        });
        if local.0 > self.arity {
            self.push_statement(
                StatementKind::FreshCell {
                    local,
                    carry_value: false,
                },
                None,
            );
        }
        local
    }

    /// Allocate a temporary (unnamed local).
    pub(crate) fn temp(&mut self, ty: RuntimeTy) -> Local {
        self.declare_local(None, ty, None)
    }

    /// Get the number of locals declared so far.
    pub(crate) fn num_locals(&self) -> usize {
        self.locals.len()
    }

    /// Return the declared type of a local variable.
    ///
    /// Used by `bind_pattern` in `lower.rs` to propagate the scrutinee's type
    /// to catch binding locals when TIR has not populated the pattern type map.
    pub(crate) fn local_ty(&self, local: Local) -> RuntimeTy {
        self.locals[local.0].ty.clone()
    }

    /// The declaration of a local.
    pub(crate) fn local_decl(&self, local: Local) -> &LocalDecl {
        &self.locals[local.0]
    }

    /// Refine a local's declared type once TIR has a more specific one.
    pub(crate) fn set_local_ty(&mut self, local: Local, ty: RuntimeTy) {
        self.locals[local.0].ty = ty;
    }

    // ========================================================================
    // Block Management
    // ========================================================================

    /// Create a new basic block and return its ID. It unwinds to the handler
    /// in force here and belongs to the handler body being lowered.
    pub(crate) fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        let mut block = BasicBlock::new(id);
        block.unwind = self.current_unwind;
        block.handling = self.current_handling;
        block.shielded = self.shield_depth > 0;
        self.blocks.push(block);
        id
    }

    /// Number of blocks created so far. Block IDs are dense `0..num_blocks()`,
    /// so a range captured around a lowering step names exactly the blocks that
    /// step created (used to record a catch handler body, BEP-042).
    pub(crate) fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// Set the current block for emitting statements and terminators.
    pub(crate) fn set_current_block(&mut self, block: BlockId) {
        self.current_block = Some(block);
    }

    /// Get the current block ID, panics if none is set.
    pub(crate) fn current_block(&self) -> BlockId {
        self.current_block.expect("no current block set")
    }

    /// Check if the current block has been terminated.
    pub(crate) fn is_current_terminated(&self) -> bool {
        self.current_block
            .map(|id| self.blocks[id.0].is_terminated())
            .unwrap_or(true)
    }

    /// Get a reference to a block.
    pub(crate) fn get_block(&self, id: BlockId) -> &BasicBlock<'db> {
        &self.blocks[id.0]
    }

    /// Get a mutable reference to a block.
    pub(crate) fn get_block_mut(&mut self, id: BlockId) -> &mut BasicBlock<'db> {
        &mut self.blocks[id.0]
    }

    // ========================================================================
    // Handlers
    // ========================================================================

    /// The handler the current lowering position unwinds to.
    pub(crate) fn unwind(&self) -> Option<BlockId> {
        self.current_unwind
    }

    /// Create a handler block: the VM lands in it with `landing`, and it is
    /// the first block of its own handler body. It unwinds to the handler in
    /// force where it is created — never to itself.
    pub(crate) fn create_handler_block(&mut self, landing: Landing) -> BlockId {
        let id = self.create_block();
        let block = &mut self.blocks[id.0];
        block.landing = Some(landing);
        block.handling = Some(id);
        id
    }

    /// What the VM lands in `handler` with.
    pub(crate) fn landing(&self, handler: BlockId) -> Landing {
        self.blocks[handler.0]
            .landing
            .unwrap_or_else(|| unreachable!("{handler:?} is not a handler block"))
    }

    /// Lower what follows with `unwind` as its handler. Live code needs a
    /// block created under the new handler, so the current block (created
    /// under the old one) is left with a `goto` to a fresh block, which
    /// becomes current. A current block that is still empty simply takes the
    /// new handler: everything lowered into it will run under it. A
    /// terminated current block needs nothing, the caller moves away from
    /// it. A `catch` opens with its handler over the try body and closes with
    /// the outer value; a `defer` opens with its pad over the rest of the
    /// block; an inline replay retreats to the value its defer was armed
    /// under and comes back.
    pub(crate) fn transition_unwind(&mut self, unwind: Option<BlockId>) {
        if self.current_unwind == unwind {
            return;
        }
        if let Some(handler) = unwind {
            assert!(
                self.blocks[handler.0].landing.is_some(),
                "{handler:?} is not a handler block: nothing can unwind to it"
            );
        }
        if self.is_current_terminated() {
            self.current_unwind = unwind;
            return;
        }
        let current = self.current_block();
        if self.blocks[current.0].statements.is_empty() {
            self.blocks[current.0].unwind = unwind;
            self.current_unwind = unwind;
            return;
        }
        // The goto belongs to the block it leaves, under the old handler.
        let fresh = BlockId(self.blocks.len());
        self.set_terminator(Terminator::Goto { target: fresh });
        self.current_unwind = unwind;
        let created = self.create_block();
        debug_assert_eq!(created, fresh);
        self.current_block = Some(fresh);
    }

    /// Lower what follows inside a `defer` body: shielded from cancellation.
    /// Nests; handing the returned [`Shield`] to [`Self::leave_shield`]
    /// closes it. Live code that follows needs a block stamped shielded, so
    /// the current block is left with a `goto` to a fresh one (an empty
    /// block is restamped).
    pub(crate) fn enter_shield(&mut self) -> Shield {
        let depth = self.shield_depth + 1;
        self.transition_shield(depth);
        Shield { depth }
    }

    /// Close `shield`, the innermost one open.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the token is consumed so a shield cannot be closed twice"
    )]
    pub(crate) fn leave_shield(&mut self, shield: Shield) {
        debug_assert_eq!(
            shield.depth, self.shield_depth,
            "shields close innermost first"
        );
        self.transition_shield(shield.depth - 1);
    }

    /// Make `depth` the shield depth. Nothing more if the current block is
    /// terminated or already carries the new shield state; an empty current
    /// block is restamped; otherwise it is left with a `goto` — emitted under
    /// the old state, the block's own — to a fresh block created under the new.
    fn transition_shield(&mut self, depth: u32) {
        let shielded = depth > 0;
        if self.is_current_terminated() {
            self.shield_depth = depth;
            return;
        }
        let current = self.current_block();
        if self.blocks[current.0].shielded == shielded {
            self.shield_depth = depth;
            return;
        }
        if self.blocks[current.0].statements.is_empty() {
            self.blocks[current.0].shielded = shielded;
            self.shield_depth = depth;
            return;
        }
        let fresh = BlockId(self.blocks.len());
        self.set_terminator(Terminator::Goto { target: fresh });
        self.shield_depth = depth;
        let created = self.create_block();
        debug_assert_eq!(created, fresh);
        self.current_block = Some(fresh);
    }

    /// Fill `block` (a handler created earlier) with `unwind` as its handler
    /// and `handling` as the handler body it is part of, leaving the current
    /// block where it is; the returned token brings lowering back to it.
    pub(crate) fn begin_out_of_line(
        &mut self,
        block: BlockId,
        unwind: Option<BlockId>,
        handling: Option<BlockId>,
    ) -> OutOfLine {
        let out_of_line = OutOfLine {
            block: self.current_block(),
            unwind: self.current_unwind,
            handling: self.current_handling,
        };
        self.current_block = Some(block);
        self.current_unwind = unwind;
        self.current_handling = handling;
        out_of_line
    }

    /// Return from [`Self::begin_out_of_line`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the token is consumed so lowering cannot return to the same place twice"
    )]
    pub(crate) fn end_out_of_line(&mut self, out_of_line: OutOfLine) {
        self.current_block = Some(out_of_line.block);
        self.current_unwind = out_of_line.unwind;
        self.current_handling = out_of_line.handling;
    }

    // ========================================================================
    // Statement Emission
    // ========================================================================

    fn current_block_mut(&mut self) -> &mut BasicBlock<'db> {
        self.check_current_protection();
        let id = self.current_block.expect("no current block set");
        &mut self.blocks[id.0]
    }

    /// The current block must have been created under the handler in force:
    /// a block's handler is fixed at creation, so lowering code into it from
    /// a position with a different handler would give that code the wrong
    /// one.
    fn check_current_protection(&self) {
        let id = self.current_block.expect("no current block set");
        let block = &self.blocks[id.0];
        assert!(
            block.unwind == self.current_unwind
                && block.handling == self.current_handling
                && block.shielded == (self.shield_depth > 0),
            "{id:?} was created unwinding to {:?} (handling {:?}, shielded {}) but code is being \
             lowered into it unwinding to {:?} (handling {:?}, shielded {})",
            block.unwind,
            block.handling,
            block.shielded,
            self.current_unwind,
            self.current_handling,
            self.shield_depth > 0,
        );
    }

    /// Push a statement to the current block.
    pub(crate) fn push_statement(&mut self, kind: StatementKind<'db>, span: Option<Span>) {
        let span = span.or(self.current_source_span);
        let block = self.current_block_mut();
        assert!(
            block.terminator.is_none(),
            "cannot add statement to terminated block"
        );
        block.statements.push(Statement { kind, span });
    }

    /// Emit an assignment: `dest = value`
    pub(crate) fn assign(&mut self, destination: Place, value: Rvalue<'db>) {
        self.push_statement(StatementKind::Assign { destination, value }, None);
    }

    /// Emit an assignment with span.
    pub(crate) fn assign_with_span(&mut self, destination: Place, value: Rvalue<'db>, span: Span) {
        self.push_statement(StatementKind::Assign { destination, value }, Some(span));
    }

    /// Emit an open-world interface-field store.
    pub(crate) fn virtual_field_store(
        &mut self,
        iface: baml_type::TyTemplateInterface,
        receiver: Operand<'db>,
        field_index: u32,
        field: baml_base::Name,
        value: Operand<'db>,
    ) {
        self.push_statement(
            StatementKind::VirtualFieldStore {
                iface,
                receiver,
                field_index,
                field,
                value,
            },
            None,
        );
    }

    /// Emit a drop statement.
    pub(crate) fn drop(&mut self, place: Place) {
        self.push_statement(StatementKind::Drop(place), None);
    }

    /// Give a captured local a new cell holding its current value, so the
    /// closures that captured the old cell stop sharing it with what follows.
    pub(crate) fn recell_with_current_value(&mut self, local: Local) {
        debug_assert!(
            self.locals[local.0].is_captured,
            "recell of {local}, which no closure captures"
        );
        self.push_statement(
            StatementKind::FreshCell {
                local,
                carry_value: true,
            },
            None,
        );
    }

    /// Emit a nop statement.
    pub(crate) fn nop(&mut self) {
        self.push_statement(StatementKind::Nop, None);
    }

    /// Set debug scope span for a local variable.
    pub(crate) fn set_local_scope_span(&mut self, local: Local, scope_span: Option<Span>) {
        if let Some(local_decl) = self.locals.get_mut(local.0) {
            local_decl.scope_span = scope_span;
        }
    }

    // ========================================================================
    // Terminator Emission
    // ========================================================================

    fn set_terminator(&mut self, terminator: Terminator<'db>) {
        let terminator_span = self.current_source_span;
        let block = self.current_block_mut();
        assert!(block.terminator.is_none(), "block already has a terminator");
        block.terminator = Some(terminator);
        block.terminator_span = terminator_span;
    }

    /// Emit an unconditional goto.
    pub(crate) fn goto(&mut self, target: BlockId) {
        self.set_terminator(Terminator::Goto { target });
    }

    /// Emit a conditional branch.
    pub(crate) fn branch(
        &mut self,
        condition: Operand<'db>,
        then_block: BlockId,
        else_block: BlockId,
    ) {
        self.set_terminator(Terminator::Branch {
            condition,
            then_block,
            else_block,
        });
    }

    pub(crate) fn narrow_bind(
        &mut self,
        source: Operand<'db>,
        ty_template: TyTemplate,
        destination: Local,
        then_block: BlockId,
        else_block: BlockId,
    ) {
        self.set_terminator(Terminator::NarrowBind {
            source,
            ty_template,
            destination,
            then_block,
            else_block,
        });
    }

    /// Emit a short-circuit `&&` / `||` terminator.
    pub(crate) fn short_circuit(
        &mut self,
        operand: Operand<'db>,
        kind: crate::ShortCircuitKind,
        destination: Place,
        eval_rhs: BlockId,
        join: BlockId,
    ) {
        self.set_terminator(Terminator::ShortCircuit {
            operand,
            kind,
            destination,
            eval_rhs,
            join,
        });
    }

    /// Emit a multi-way switch.
    ///
    /// If `exhaustive` is true, the switch covers all possible discriminant values,
    /// allowing the last arm's comparison to be skipped during codegen.
    pub(crate) fn switch(
        &mut self,
        discriminant: Operand<'db>,
        arms: Vec<(i64, BlockId)>,
        otherwise: BlockId,
        exhaustive: bool,
        arm_names: Vec<(i64, String)>,
    ) {
        self.set_terminator(Terminator::Switch {
            discriminant,
            arms,
            otherwise,
            exhaustive,
            arm_names,
        });
    }

    /// Emit a return.
    pub(crate) fn return_(&mut self) {
        self.set_terminator(Terminator::Return);
    }

    /// Emit a function call.
    pub(crate) fn call(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        destination: Place,
        target: BlockId,
    ) {
        self.call_with_type_args(callee, args, 0, destination, target);
    }

    /// Emit a function call with an explicit type-argument count.
    ///
    /// The first `ntypeargs` entries of `args` must be `Object::Type` values
    /// produced by `Rvalue::LoadType`.  Regular value args follow after them.
    pub(crate) fn call_with_type_args(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        ntypeargs: usize,
        destination: Place,
        target: BlockId,
    ) {
        self.call_with_type_args_and_runtime_id(callee, args, ntypeargs, None, destination, target);
    }

    /// Emit a function call with an optional hidden runtime-id operand.
    pub(crate) fn call_with_type_args_and_runtime_id(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        ntypeargs: usize,
        runtime_id: Option<Operand<'db>>,
        destination: Place,
        target: BlockId,
    ) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "Call destination must be a local place"
        );
        self.set_terminator(Terminator::Call {
            argument_layout: None,
            callee,
            args,
            ntypeargs,
            runtime_id,
            destination,
            target,
            unwind: self.current_unwind,
        });
    }

    /// Attach the checked argument layout to the call terminator just emitted.
    pub(crate) fn set_call_layout(&mut self, layout: Option<baml_type::CallLayout>) {
        match &mut self.current_block_mut().terminator {
            Some(
                Terminator::Call {
                    argument_layout, ..
                }
                | Terminator::VirtualCall {
                    argument_layout, ..
                },
            ) => *argument_layout = layout,
            _ => unreachable!("call layout requires a call terminator"),
        }
    }

    /// Emit an open-world virtual interface-method call. The implementation is
    /// resolved at runtime from the receiver's concrete type (the first `args`
    /// entry) against `iface`. Used for statically-undetermined receivers
    /// (bounded type-var / interface-existential / `Self` in a default body).
    pub(crate) fn virtual_call(
        &mut self,
        iface: baml_type::TyTemplateInterface,
        method: String,
        args: Vec<Operand<'db>>,
        ntypeargs: usize,
        destination: Place,
        target: BlockId,
    ) {
        self.virtual_call_with_runtime_id(
            iface,
            method,
            args,
            ntypeargs,
            None,
            destination,
            target,
        );
    }

    /// Emit an open-world virtual interface-method call with an optional hidden
    /// runtime-id operand.
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn virtual_call_with_runtime_id(
        &mut self,
        iface: baml_type::TyTemplateInterface,
        method: String,
        args: Vec<Operand<'db>>,
        ntypeargs: usize,
        runtime_id: Option<Operand<'db>>,
        destination: Place,
        target: BlockId,
    ) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "VirtualCall destination must be a local place"
        );
        debug_assert!(
            args.len() > ntypeargs,
            "VirtualCall must carry at least the receiver value argument"
        );
        self.set_terminator(Terminator::VirtualCall {
            argument_layout: None,
            iface,
            method,
            args,
            ntypeargs,
            runtime_id,
            destination,
            target,
            unwind: self.current_unwind,
        });
    }

    /// Emit an unreachable terminator.
    pub(crate) fn unreachable(&mut self) {
        self.set_terminator(Terminator::Unreachable);
    }

    /// Emit a throw terminator (unwind with error value).
    pub(crate) fn throw(&mut self, value: Operand<'db>) {
        self.set_terminator(Terminator::Throw { value });
    }

    /// Emit a rethrow terminator for a caught error value.
    pub(crate) fn rethrow(&mut self, value: Operand<'db>) {
        self.set_terminator(Terminator::Rethrow { value });
    }

    /// Emit a throw-if-panic terminator: if the value is a panic instance,
    /// throw it; otherwise continue to `otherwise`.
    pub(crate) fn throw_if_panic(&mut self, value: Operand<'db>, otherwise: BlockId) {
        self.set_terminator(Terminator::ThrowIfPanic { value, otherwise });
    }

    /// BEP-034 phase D′: emit a sys-op call. The sys-op is invoked
    /// inline in the engine (single VM↔engine round trip) and its
    /// return value is bound directly into `destination`.
    pub(crate) fn sys_op(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        destination: Place,
        target: BlockId,
    ) {
        self.sys_op_with_runtime_id(callee, args, None, destination, target);
    }

    /// BEP-034 phase D′ sys-op call with an optional hidden runtime-id operand.
    pub(crate) fn sys_op_with_runtime_id(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        runtime_id: Option<Operand<'db>>,
        destination: Place,
        target: BlockId,
    ) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "SysOp destination must be a local place"
        );
        self.set_terminator(Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            target,
            unwind: self.current_unwind,
        });
    }

    /// Emit an await.
    pub(crate) fn await_(&mut self, future: Place, destination: Place, target: BlockId) {
        debug_assert!(
            matches!(future, Place::Local(_)),
            "Await future place must be local"
        );
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "Await destination must be a local place"
        );
        self.set_terminator(Terminator::Await {
            future,
            destination,
            target,
            unwind: self.current_unwind,
        });
    }

    /// BEP-034: emit an `await_any` terminator — suspend until the first of
    /// the `futures` array settles and bind its `int` index into `destination`.
    pub(crate) fn await_any(&mut self, futures: Operand<'db>, destination: Place, target: BlockId) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "AwaitAny destination must be a local place"
        );
        self.set_terminator(Terminator::AwaitAny {
            futures,
            destination,
            target,
            unwind: self.current_unwind,
        });
    }

    /// BEP-034: emit a spawn terminator. Pops a closure operand plus an
    /// optional name operand and binds the resulting `Future<T, E>`
    /// handle into `future`.
    pub(crate) fn spawn(
        &mut self,
        closure: Operand<'db>,
        name: Operand<'db>,
        config: Option<Box<Operand<'db>>>,
        future_ty: Box<crate::ir::SpawnFutureTy>,
        future: Place,
        resume: BlockId,
    ) {
        debug_assert!(
            matches!(future, Place::Local(_)),
            "Spawn future handle place must be local"
        );
        self.set_terminator(Terminator::Spawn {
            closure,
            name,
            config,
            future_ty,
            future,
            resume,
        });
    }

    // ========================================================================
    // Convenience Helpers
    // ========================================================================

    /// Assign a constant to a place.
    pub(crate) fn assign_const(&mut self, dest: Place, constant: Constant<'db>) {
        self.assign(dest, Rvalue::Use(Operand::Constant(constant)));
    }

    /// Assign an integer constant to a local.
    pub(crate) fn assign_int(&mut self, dest: Local, value: i64) {
        self.assign_const(Place::local(dest), Constant::Int(value));
    }

    /// Assign a boolean constant to a local.
    pub(crate) fn assign_bool(&mut self, dest: Local, value: bool) {
        self.assign_const(Place::local(dest), Constant::Bool(value));
    }

    /// Assign a string constant to a local.
    pub(crate) fn assign_string(&mut self, dest: Local, value: impl Into<String>) {
        self.assign_const(Place::local(dest), Constant::String(value.into()));
    }

    /// Copy one local to another.
    pub(crate) fn copy_local(&mut self, dest: Local, src: Local) {
        self.assign(Place::local(dest), Rvalue::Use(Operand::copy_local(src)));
    }

    // ========================================================================
    // Build
    // ========================================================================

    /// Consume the builder and produce the MIR function.
    ///
    /// Panics if:
    /// - No blocks were created
    /// - Any block is unterminated
    ///
    /// The `item_ref` field is set to a placeholder — `lower_function` overwrites
    /// it with the real fully-qualified reference after calling `build()`.
    pub(crate) fn build(self) -> MirFunction<'db> {
        assert!(!self.blocks.is_empty(), "function has no blocks");
        self.assert_regions_closed();

        for (i, block) in self.blocks.iter().enumerate() {
            assert!(block.terminator.is_some(), "block bb{i} is not terminated");
        }

        MirFunction {
            arity: self.arity,
            span: self.span,
            item_ref: ItemRef::Free {
                package: baml_base::Name::new(""),
                namespace: vec![],
                name: self.name,
            },
            kind: MirFunctionKind::Bytecode(MirFunctionBody {
                blocks: self.blocks,
                entry: BlockId(0),
                locals: self.locals,
            }),
            lambdas: vec![],
            signature: None,
        }
    }

    /// Consume the builder and produce just the `MirFunctionBody`.
    ///
    /// Used when building a let-binding initializer — the caller holds the
    /// `arity` and `item_ref` context externally.
    pub(crate) fn build_body(self) -> MirFunctionBody<'db> {
        assert!(!self.blocks.is_empty(), "let body has no blocks");
        self.assert_regions_closed();
        for (i, block) in self.blocks.iter().enumerate() {
            assert!(block.terminator.is_some(), "block bb{i} is not terminated");
        }
        MirFunctionBody {
            blocks: self.blocks,
            entry: BlockId(0),
            locals: self.locals,
        }
    }

    fn assert_regions_closed(&self) {
        assert!(
            self.current_unwind.is_none()
                && self.current_handling.is_none()
                && self.shield_depth == 0,
            "a handler ({:?}, handling {:?}) or a shield (depth {}) is still in force at build",
            self.current_unwind,
            self.current_handling,
            self.shield_depth,
        );
    }

    /// Build without checking termination (for incremental construction).
    ///
    /// The `item_ref` field is set to a placeholder — `lower_function` overwrites
    /// it with the real fully-qualified reference after calling `build_unchecked()`.
    pub(crate) fn build_unchecked(self) -> MirFunction<'db> {
        MirFunction {
            arity: self.arity,
            span: self.span,
            item_ref: ItemRef::Free {
                package: baml_base::Name::new(""),
                namespace: vec![],
                name: self.name,
            },
            kind: MirFunctionKind::Bytecode(MirFunctionBody {
                blocks: self.blocks,
                entry: BlockId(0),
                locals: self.locals,
            }),
            lambdas: vec![],
            signature: None,
        }
    }
}
