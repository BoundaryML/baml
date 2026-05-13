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
//! let ret = builder.declare_local(Some("_return".into()), Ty::Int, None);
//! let param = builder.declare_local(Some("x".into()), Ty::Int, None);
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
use baml_type::Ty;

use crate::{
    BasicBlock, BlockId, CatchRegion, Constant, ItemRef, Local, LocalDecl, MirFunction,
    MirFunctionBody, MirFunctionKind, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    VizNode,
};

/// Builder for constructing MIR functions.
pub(crate) struct MirBuilder {
    name: Name,
    arity: usize,
    blocks: Vec<BasicBlock>,
    locals: Vec<LocalDecl>,
    current_block: Option<BlockId>,
    span: Option<Span>,
    viz_nodes: Vec<VizNode>,
    /// Current source span for tagging statements/terminators.
    pub(crate) current_source_span: Option<Span>,
    /// Catch regions recorded during lowering for exception table construction.
    pub(crate) catch_regions: Vec<CatchRegion>,
}

// Some builder utilities are not yet used but will be needed as MIR 2 matures.
#[allow(dead_code)]
impl MirBuilder {
    /// Create a new MIR builder for a function.
    pub(crate) fn new(name: Name, arity: usize) -> Self {
        Self {
            name,
            arity,
            blocks: Vec::new(),
            locals: Vec::new(),
            current_block: None,
            span: None,
            viz_nodes: Vec::new(),
            current_source_span: None,
            catch_regions: Vec::new(),
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
        ty: Ty,
        span: Option<Span>,
        is_watched: bool,
    ) -> Local {
        let id = Local(self.locals.len());
        self.locals.push(LocalDecl {
            name,
            ty,
            span,
            scope_span: None,
            is_watched,
            is_captured: false,
        });
        id
    }

    /// Allocate a temporary (unnamed local).
    pub(crate) fn temp(&mut self, ty: Ty) -> Local {
        self.declare_local(None, ty, None, false)
    }

    /// Get the number of locals declared so far.
    pub(crate) fn num_locals(&self) -> usize {
        self.locals.len()
    }

    /// Return the declared type of a local variable.
    ///
    /// Used by `bind_pattern` in `lower.rs` to propagate the scrutinee's type
    /// to catch binding locals when TIR has not populated the pattern type map.
    pub(crate) fn local_ty(&self, local: Local) -> Ty {
        self.locals[local.0].ty.clone()
    }

    /// Get a mutable reference to a local declaration.
    ///
    /// Used by Phase 4 to set `is_captured = true` after lowering the function body
    /// but before calling `build()`.
    pub(crate) fn local_decl_mut(&mut self, local: Local) -> &mut LocalDecl {
        &mut self.locals[local.0]
    }

    // ========================================================================
    // Block Management
    // ========================================================================

    /// Create a new basic block and return its ID.
    pub(crate) fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock::new(id));
        id
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
    pub(crate) fn get_block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }

    /// Get a mutable reference to a block.
    pub(crate) fn get_block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        &mut self.blocks[id.0]
    }

    // ========================================================================
    // Statement Emission
    // ========================================================================

    fn current_block_mut(&mut self) -> &mut BasicBlock {
        let id = self.current_block.expect("no current block set");
        &mut self.blocks[id.0]
    }

    /// Push a statement to the current block.
    pub(crate) fn push_statement(&mut self, kind: StatementKind, span: Option<Span>) {
        let span = span.or(self.current_source_span);
        let block = self.current_block_mut();
        assert!(
            block.terminator.is_none(),
            "cannot add statement to terminated block"
        );
        block.statements.push(Statement { kind, span });
    }

    /// Emit an assignment: `dest = value`
    pub(crate) fn assign(&mut self, destination: Place, value: Rvalue) {
        self.push_statement(StatementKind::Assign { destination, value }, None);
    }

    /// Emit an assignment with span.
    pub(crate) fn assign_with_span(&mut self, destination: Place, value: Rvalue, span: Span) {
        self.push_statement(StatementKind::Assign { destination, value }, Some(span));
    }

    /// Emit a drop statement.
    pub(crate) fn drop(&mut self, place: Place) {
        self.push_statement(StatementKind::Drop(place), None);
    }

    /// Emit a fresh-cell statement for a loop variable.
    pub(crate) fn fresh_cell(&mut self, local: Local) {
        self.push_statement(StatementKind::FreshCell(local), None);
    }

    /// Emit a nop statement.
    pub(crate) fn nop(&mut self) {
        self.push_statement(StatementKind::Nop, None);
    }

    /// Emit an unwatch statement for a watched local going out of scope.
    pub(crate) fn unwatch(&mut self, local: Local) {
        self.push_statement(StatementKind::Unwatch(local), None);
    }

    /// Emit a `watch_options` statement to update the filter for a watched local.
    pub(crate) fn watch_options(&mut self, local: Local, filter: Operand) {
        self.push_statement(StatementKind::WatchOptions { local, filter }, None);
    }

    /// Emit a `watch_notify` statement to manually trigger notification for a watched local.
    pub(crate) fn watch_notify(&mut self, local: Local) {
        self.push_statement(StatementKind::WatchNotify(local), None);
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

    fn set_terminator(&mut self, terminator: Terminator) {
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
    pub(crate) fn branch(&mut self, condition: Operand, then_block: BlockId, else_block: BlockId) {
        self.set_terminator(Terminator::Branch {
            condition,
            then_block,
            else_block,
        });
    }

    /// Emit a short-circuit `&&` / `||` terminator.
    pub(crate) fn short_circuit(
        &mut self,
        operand: Operand,
        is_and: bool,
        destination: Place,
        eval_rhs: BlockId,
        join: BlockId,
    ) {
        self.set_terminator(Terminator::ShortCircuit {
            operand,
            is_and,
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
        discriminant: Operand,
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
        callee: Operand,
        args: Vec<Operand>,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
        self.call_with_type_args(callee, args, 0, destination, target, unwind);
    }

    /// Emit a function call with an explicit type-argument count.
    ///
    /// The first `ntypeargs` entries of `args` must be `Object::Type` values
    /// produced by `Rvalue::LoadType`.  Regular value args follow after them.
    pub(crate) fn call_with_type_args(
        &mut self,
        callee: Operand,
        args: Vec<Operand>,
        ntypeargs: usize,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "Call destination must be a local place"
        );
        self.set_terminator(Terminator::Call {
            callee,
            args,
            ntypeargs,
            destination,
            target,
            unwind,
        });
    }

    /// Emit an unreachable terminator.
    pub(crate) fn unreachable(&mut self) {
        self.set_terminator(Terminator::Unreachable);
    }

    /// Emit a throw terminator (unwind with error value).
    pub(crate) fn throw(&mut self, value: Operand) {
        self.set_terminator(Terminator::Throw { value });
    }

    /// Emit a throw-if-panic terminator: if the value is a panic instance,
    /// throw it; otherwise continue to `otherwise`.
    pub(crate) fn throw_if_panic(&mut self, value: Operand, otherwise: BlockId) {
        self.set_terminator(Terminator::ThrowIfPanic { value, otherwise });
    }

    /// BEP-034 phase D′: emit a sys-op call. The sys-op is invoked
    /// inline in the engine (single VM↔engine round trip) and its
    /// return value is bound directly into `destination`.
    pub(crate) fn sys_op(
        &mut self,
        callee: Operand,
        args: Vec<Operand>,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
        debug_assert!(
            matches!(destination, Place::Local(_)),
            "SysOp destination must be a local place"
        );
        self.set_terminator(Terminator::SysOp {
            callee,
            args,
            destination,
            target,
            unwind,
        });
    }

    /// Emit an await.
    pub(crate) fn await_(
        &mut self,
        future: Place,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
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
            unwind,
        });
    }

    /// BEP-034: emit a spawn terminator. Pops a closure operand plus an
    /// optional name operand and binds the resulting `Future<T, E>`
    /// handle into `future`.
    pub(crate) fn spawn(
        &mut self,
        closure: Operand,
        name: Operand,
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
            future,
            resume,
        });
    }

    // ========================================================================
    // Convenience Helpers
    // ========================================================================

    /// Assign a constant to a place.
    pub(crate) fn assign_const(&mut self, dest: Place, constant: Constant) {
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
    pub(crate) fn build(self) -> MirFunction {
        assert!(!self.blocks.is_empty(), "function has no blocks");

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
                catch_regions: self.catch_regions.clone(),
                viz_nodes: self.viz_nodes,
            }),
            lambdas: vec![],
        }
    }

    /// Consume the builder and produce just the `MirFunctionBody`.
    ///
    /// Used when building a let-binding initializer — the caller holds the
    /// `arity` and `item_ref` context externally.
    pub(crate) fn build_body(self) -> MirFunctionBody {
        assert!(!self.blocks.is_empty(), "let body has no blocks");
        for (i, block) in self.blocks.iter().enumerate() {
            assert!(block.terminator.is_some(), "block bb{i} is not terminated");
        }
        MirFunctionBody {
            blocks: self.blocks,
            entry: BlockId(0),
            locals: self.locals,
            catch_regions: self.catch_regions,
            viz_nodes: self.viz_nodes,
        }
    }

    /// Build without checking termination (for incremental construction).
    ///
    /// The `item_ref` field is set to a placeholder — `lower_function` overwrites
    /// it with the real fully-qualified reference after calling `build_unchecked()`.
    pub(crate) fn build_unchecked(self) -> MirFunction {
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
                catch_regions: self.catch_regions.clone(),
                viz_nodes: self.viz_nodes,
            }),
            lambdas: vec![],
        }
    }

    // ========================================================================
    // Visualization Helpers
    // ========================================================================

    /// Add a visualization node and return its index.
    pub(crate) fn add_viz_node(&mut self, node: VizNode) -> usize {
        let idx = self.viz_nodes.len();
        self.viz_nodes.push(node);
        idx
    }
}
