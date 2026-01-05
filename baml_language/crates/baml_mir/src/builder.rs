//! MIR Builder API.
//!
//! Provides a fluent interface for constructing MIR functions. The builder
//! manages local allocation, basic block creation, and ensures well-formed MIR.
//!
//! # Example
//!
//! ```ignore
//! let mut builder = MirBuilder::new("my_function", 1);
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

use baml_base::Name;
use baml_tir::Ty;
use text_size::TextRange;

use crate::{
    BasicBlock, BlockId, Constant, Local, LocalDecl, MirFunction, Operand, Place, Rvalue,
    Statement, StatementKind, Terminator, VizNode,
};

/// Builder for constructing MIR functions.
pub struct MirBuilder<'db> {
    name: String,
    arity: usize,
    blocks: Vec<BasicBlock<'db>>,
    locals: Vec<LocalDecl<'db>>,
    current_block: Option<BlockId>,
    span: Option<TextRange>,
    viz_nodes: Vec<VizNode>,
}

impl<'db> MirBuilder<'db> {
    /// Create a new MIR builder for a function.
    pub fn new(name: impl Into<String>, arity: usize) -> Self {
        Self {
            name: name.into(),
            arity,
            blocks: Vec::new(),
            locals: Vec::new(),
            current_block: None,
            span: None,
            viz_nodes: Vec::new(),
        }
    }

    /// Set the source span for the function.
    pub fn set_span(&mut self, span: TextRange) {
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
    pub fn declare_local(
        &mut self,
        name: Option<Name>,
        ty: Ty<'db>,
        span: Option<TextRange>,
        is_watched: bool,
    ) -> Local {
        let id = Local(self.locals.len());
        self.locals.push(LocalDecl {
            name,
            ty,
            span,
            is_watched,
        });
        id
    }

    /// Allocate a temporary (unnamed local).
    pub fn temp(&mut self, ty: Ty<'db>) -> Local {
        self.declare_local(None, ty, None, false)
    }

    /// Get the number of locals declared so far.
    pub fn num_locals(&self) -> usize {
        self.locals.len()
    }

    // ========================================================================
    // Block Management
    // ========================================================================

    /// Create a new basic block and return its ID.
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock::new(id));
        id
    }

    /// Set the current block for emitting statements and terminators.
    pub fn set_current_block(&mut self, block: BlockId) {
        self.current_block = Some(block);
    }

    /// Get the current block ID, panics if none is set.
    pub fn current_block(&self) -> BlockId {
        self.current_block.expect("no current block set")
    }

    /// Check if the current block has been terminated.
    pub fn is_current_terminated(&self) -> bool {
        self.current_block
            .map(|id| self.blocks[id.0].is_terminated())
            .unwrap_or(true)
    }

    /// Get a reference to a block.
    pub fn get_block(&self, id: BlockId) -> &BasicBlock<'db> {
        &self.blocks[id.0]
    }

    /// Get a mutable reference to a block.
    pub fn get_block_mut(&mut self, id: BlockId) -> &mut BasicBlock<'db> {
        &mut self.blocks[id.0]
    }

    // ========================================================================
    // Statement Emission
    // ========================================================================

    fn current_block_mut(&mut self) -> &mut BasicBlock<'db> {
        let id = self.current_block.expect("no current block set");
        &mut self.blocks[id.0]
    }

    /// Push a statement to the current block.
    pub fn push_statement(&mut self, kind: StatementKind<'db>, span: Option<TextRange>) {
        let block = self.current_block_mut();
        assert!(
            block.terminator.is_none(),
            "cannot add statement to terminated block"
        );
        block.statements.push(Statement { kind, span });
    }

    /// Emit an assignment: `dest = value`
    pub fn assign(&mut self, destination: Place, value: Rvalue<'db>) {
        self.push_statement(StatementKind::Assign { destination, value }, None);
    }

    /// Emit an assignment with span.
    pub fn assign_with_span(&mut self, destination: Place, value: Rvalue<'db>, span: TextRange) {
        self.push_statement(StatementKind::Assign { destination, value }, Some(span));
    }

    /// Emit a drop statement.
    pub fn drop(&mut self, place: Place) {
        self.push_statement(StatementKind::Drop(place), None);
    }

    /// Emit a nop statement.
    pub fn nop(&mut self) {
        self.push_statement(StatementKind::Nop, None);
    }

    /// Emit an unwatch statement for a watched local going out of scope.
    pub fn unwatch(&mut self, local: Local) {
        self.push_statement(StatementKind::Unwatch(local), None);
    }

    /// Emit a `watch_options` statement to update the filter for a watched local.
    pub fn watch_options(&mut self, local: Local, filter: Operand<'db>) {
        self.push_statement(StatementKind::WatchOptions { local, filter }, None);
    }

    /// Emit a `watch_notify` statement to manually trigger notification for a watched local.
    pub fn watch_notify(&mut self, local: Local) {
        self.push_statement(StatementKind::WatchNotify(local), None);
    }

    /// Emit an assert statement.
    pub fn assert(&mut self, condition: Operand<'db>) {
        self.push_statement(StatementKind::Assert(condition), None);
    }

    // ========================================================================
    // Terminator Emission
    // ========================================================================

    fn set_terminator(&mut self, terminator: Terminator<'db>) {
        let block = self.current_block_mut();
        assert!(block.terminator.is_none(), "block already has a terminator");
        block.terminator = Some(terminator);
    }

    /// Emit an unconditional goto.
    pub fn goto(&mut self, target: BlockId) {
        self.set_terminator(Terminator::Goto { target });
    }

    /// Emit a conditional branch.
    pub fn branch(&mut self, condition: Operand<'db>, then_block: BlockId, else_block: BlockId) {
        self.set_terminator(Terminator::Branch {
            condition,
            then_block,
            else_block,
        });
    }

    /// Emit a multi-way switch.
    pub fn switch(
        &mut self,
        discriminant: Operand<'db>,
        arms: Vec<(i64, BlockId)>,
        otherwise: BlockId,
    ) {
        self.set_terminator(Terminator::Switch {
            discriminant,
            arms,
            otherwise,
        });
    }

    /// Emit a return.
    pub fn return_(&mut self) {
        self.set_terminator(Terminator::Return);
    }

    /// Emit a function call.
    pub fn call(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
        self.set_terminator(Terminator::Call {
            callee,
            args,
            destination,
            target,
            unwind,
        });
    }

    /// Emit an unreachable terminator.
    pub fn unreachable(&mut self) {
        self.set_terminator(Terminator::Unreachable);
    }

    /// Emit a dispatch future (for LLM calls).
    pub fn dispatch_future(
        &mut self,
        callee: Operand<'db>,
        args: Vec<Operand<'db>>,
        future: Place,
        resume: BlockId,
    ) {
        self.set_terminator(Terminator::DispatchFuture {
            callee,
            args,
            future,
            resume,
        });
    }

    /// Emit an await.
    pub fn await_(
        &mut self,
        future: Place,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    ) {
        self.set_terminator(Terminator::Await {
            future,
            destination,
            target,
            unwind,
        });
    }

    // ========================================================================
    // Convenience Helpers
    // ========================================================================

    /// Assign a constant to a place.
    pub fn assign_const(&mut self, dest: Place, constant: Constant<'db>) {
        self.assign(dest, Rvalue::Use(Operand::Constant(constant)));
    }

    /// Assign an integer constant to a local.
    pub fn assign_int(&mut self, dest: Local, value: i64) {
        self.assign_const(Place::local(dest), Constant::Int(value));
    }

    /// Assign a boolean constant to a local.
    pub fn assign_bool(&mut self, dest: Local, value: bool) {
        self.assign_const(Place::local(dest), Constant::Bool(value));
    }

    /// Assign a string constant to a local.
    pub fn assign_string(&mut self, dest: Local, value: impl Into<String>) {
        self.assign_const(Place::local(dest), Constant::String(value.into()));
    }

    /// Copy one local to another.
    pub fn copy_local(&mut self, dest: Local, src: Local) {
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
    pub fn build(self) -> MirFunction<'db> {
        assert!(!self.blocks.is_empty(), "function has no blocks");

        // Verify all blocks are terminated
        for (i, block) in self.blocks.iter().enumerate() {
            assert!(block.terminator.is_some(), "block bb{i} is not terminated");
        }

        MirFunction {
            name: self.name,
            arity: self.arity,
            blocks: self.blocks,
            entry: BlockId(0),
            locals: self.locals,
            span: self.span,
            viz_nodes: self.viz_nodes,
        }
    }

    /// Build without checking termination (for incremental construction).
    pub fn build_unchecked(self) -> MirFunction<'db> {
        MirFunction {
            name: self.name,
            arity: self.arity,
            blocks: self.blocks,
            entry: BlockId(0),
            locals: self.locals,
            span: self.span,
            viz_nodes: self.viz_nodes,
        }
    }

    // ========================================================================
    // Visualization Helpers
    // ========================================================================

    /// Add a visualization node and return its index.
    pub fn add_viz_node(&mut self, node: VizNode) -> usize {
        let idx = self.viz_nodes.len();
        self.viz_nodes.push(node);
        idx
    }

    /// Emit a `VizEnter` statement for the given node index.
    pub fn viz_enter(&mut self, node_idx: usize) {
        self.push_statement(StatementKind::VizEnter(node_idx), None);
    }

    /// Emit a `VizExit` statement for the given node index.
    pub fn viz_exit(&mut self, node_idx: usize) {
        self.push_statement(StatementKind::VizExit(node_idx), None);
    }
}
