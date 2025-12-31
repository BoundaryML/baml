---
id: BEP-007
title: "Mid-level Intermediate Representation (MIR)"
shepherds: Antonio Sarosi <sarosiantonio@gmail.com>
status: Draft
created: 2025-12-18
---

# BEP-007: Mid-level Intermediate Representation (MIR)

## Summary

This proposal introduces a Mid-level Intermediate Representation (MIR) between THIR and bytecode generation in the BAML compiler pipeline. MIR is a Control Flow Graph (CFG) based representation that simplifies the compilation of complex control flow constructs like `match` statements and error handling (`catch` expressions). By lowering high-level constructs into basic blocks connected by explicit jumps, MIR bridges the semantic gap between the tree-structured THIR and the linear bytecode, making codegen simpler, more maintainable, and easier to extend with new language features.

## Background: Control Flow Graphs and MIR

### What is a Control Flow Graph?

A Control Flow Graph (CFG) is a representation of code where:

1. **Basic Blocks**: Code is divided into sequences of straight-line instructions with no internal branches. Each basic block executes entirely or not at all.

2. **Terminators**: Each basic block ends with a "terminator" instruction that transfers control to another block (or returns).

3. **Edges**: Terminators create directed edges between blocks, forming a graph.

Unlike tree-based IRs (AST, HIR, THIR), CFGs "flatten" nested control flow into a graph of connected blocks:

```
Tree-based (nested):              CFG-based (flat):

if (cond) {                       bb0:
    a()                               cond
} else {                              branch cond -> bb1, bb2
    b()
}                                 bb1:
c()                                   call a
                                      goto -> bb3

                                  bb2:
                                      call b
                                      goto -> bb3

                                  bb3:
                                      call c
                                      return
```

### Why CFGs for Compilers?

1. **Uniform Control Flow**: All control flow (loops, conditionals, match) becomes explicit jumps between blocks
2. **Analysis-Friendly**: Dataflow analysis, liveness, and optimization passes traverse blocks uniformly
3. **Codegen-Friendly**: Linear bytecode emission becomes straightforward block-by-block traversal
4. **Pattern Compilation**: Match statements naturally compile to decision trees of blocks

### MIR in Industry

- **Rust**: Uses MIR for borrow checking and optimization after type checking
- **LLVM**: Uses CFG-based IR as its core representation
- **JVM/CLR**: Bytecode verifiers reconstruct CFGs for analysis
- **CPython**: Builds internal CFG before emitting bytecode

## Motivation

### The Problem: THIR → Bytecode Gap is Too Large

The current BAML compiler pipeline jumps directly from tree-structured THIR to linear bytecode:

```
Source → CST → HIR → THIR → Bytecode
                            ↑
                      (large semantic gap)
```

This creates significant complexity in `baml_codegen/src/compiler.rs`:

#### Complexity 1: Manual Jump Patching

The compiler must manually track and patch jump offsets because bytecode addresses aren't known until emission:

```rust
// Current approach: emit placeholder, patch later
fn compile_while_loop(&mut self, ...) {
    let loop_start = self.next_insn_index();

    compile_condition(self);

    // Emit jump with placeholder offset (0)
    let bail_jump = self.emit(Instruction::JumpIfFalse(0));
    self.emit(Instruction::Pop(1));

    // ... compile body ...

    // Jump back to start
    self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

    // NOW we know where to patch
    let pop_if_condition = self.emit(Instruction::Pop(1));
    self.patch_jump_to(bail_jump, pop_if_condition);

    // Patch all break statements
    for loc in break_locs {
        self.patch_jump(loc);
    }
}
```

This pattern requires:
- Tracking instruction indices during compilation
- Maintaining patch lists for forward jumps
- Manual offset calculation with error-prone arithmetic

#### Complexity 2: Break/Continue Tracking

Break and continue require complex bookkeeping to determine how many scopes to pop:

```rust
struct LoopInfo {
    scope_depth: usize,
    break_patch_list: Vec<usize>,
    continue_patch_list: Vec<usize>,
}

fn compile_break(&mut self) {
    let loop_info = self.current_loop.as_ref()
        .expect("break statement outside of loop");
    let pop_until = loop_info.scope_depth;

    // Must emit scope cleanup before jump
    self.emit_scope_drops(pop_until);

    let jump_loc = self.emit(Instruction::Jump(0));
    self.current_loop.as_mut().unwrap()
        .break_patch_list.push(jump_loc);
}
```

#### Complexity 3: Short-Circuit Boolean Operators

And/Or operators require inline control flow that breaks the tree-recursive pattern:

```rust
Expr::Binary { op: BinaryOp::And, lhs, rhs } => {
    self.compile_expr(*lhs, body);
    let skip_right = self.emit(Instruction::JumpIfFalse(0));
    self.emit(Instruction::Pop(1));
    self.compile_expr(*rhs, body);
    self.patch_jump(skip_right);
}
```

#### Complexity 4: Value-Producing Analysis

The compiler must recursively determine if expressions produce values for proper stack management:

```rust
fn expr_produces_value(expr_id: ExprId, body: &ExprBody) -> bool {
    match &body.exprs[expr_id] {
        Expr::If { then_branch, else_branch, .. } => {
            let Some(else_expr) = else_branch else {
                return false;  // If-without-else never produces
            };
            // Both branches must produce
            Self::expr_produces_value(*then_branch, body)
                && Self::expr_produces_value(*else_expr, body)
        }
        Expr::Block { tail_expr, .. } => {
            tail_expr.map(|tail| Self::expr_produces_value(tail, body))
                .unwrap_or(false)
        }
        _ => true,
    }
}
```

### Why Not Just Improve Current Codegen?

Adding new control flow constructs (match, catch) to the current architecture would multiply this complexity. Each new construct would need:
- Its own jump patching logic
- Integration with break/continue tracking
- Value-production analysis
- Scope cleanup coordination

With MIR, all these concerns are handled uniformly at the MIR level, and codegen becomes a simple block-by-block traversal.

## Proposed Design

### New Pipeline

```
Source → CST → HIR → THIR → MIR → Bytecode
                            ↑
                    (new layer)
```

MIR sits between THIR (where type checking and exhaustiveness analysis occur) and bytecode emission.

### MIR Data Structures

```rust
/// A function represented as a control flow graph.
pub struct MirFunction {
    /// Function name for debugging.
    pub name: String,
    /// Parameter count.
    pub arity: usize,
    /// All basic blocks in the function.
    pub blocks: Vec<BasicBlock>,
    /// Entry block index (always 0 by convention).
    pub entry: BlockId,
    /// Local variable declarations.
    pub locals: Vec<LocalDecl>,
    /// Span for error reporting.
    pub span: Span,
}

/// Unique identifier for a basic block within a function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

/// Unique identifier for a local variable/temporary.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Local(pub usize);

/// Declaration of a local variable or temporary.
pub struct LocalDecl {
    /// Variable name (empty for compiler temporaries).
    pub name: Option<String>,
    /// Type of this local.
    pub ty: Ty,
    /// Source span (for diagnostics).
    pub span: Option<Span>,
}

/// A basic block: a sequence of statements ending with a terminator.
pub struct BasicBlock {
    /// Unique identifier.
    pub id: BlockId,
    /// Statements executed in order.
    pub statements: Vec<Statement>,
    /// How this block exits (required).
    pub terminator: Terminator,
    /// Source span covering this block.
    pub span: Option<Span>,
}

/// A single MIR statement (does not transfer control).
pub struct Statement {
    pub kind: StatementKind,
    pub span: Option<Span>,
}

pub enum StatementKind {
    /// Assign a value to a local: `_1 = <rvalue>`
    Assign {
        destination: Place,
        value: Rvalue,
    },
    /// Drop a value (run destructor if any).
    Drop(Place),
    /// No-op (placeholder for removed statements).
    Nop,
}

/// A place in memory (lvalue).
pub enum Place {
    /// A local variable: `_1`
    Local(Local),
    /// Field access: `_1.field_idx`
    Field {
        base: Box<Place>,
        field: usize,
    },
    /// Array indexing: `_1[_2]`
    Index {
        base: Box<Place>,
        index: Local,
    },
}

/// A value computation (rvalue).
pub enum Rvalue {
    /// Use a place directly: `_1`
    Use(Operand),
    /// Binary operation: `_1 + _2`
    BinaryOp {
        op: BinOp,
        left: Operand,
        right: Operand,
    },
    /// Unary operation: `!_1`
    UnaryOp {
        op: UnaryOp,
        operand: Operand,
    },
    /// Create an array: `[_1, _2, _3]`
    Array(Vec<Operand>),
    /// Create a class instance: `ClassName { field0: _1, field1: _2 }`
    Aggregate {
        kind: AggregateKind,
        fields: Vec<Operand>,
    },
    /// Read discriminant of enum/union: `discriminant(_1)`
    Discriminant(Place),
    /// Get length of array: `len(_1)`
    Len(Place),
}

pub enum AggregateKind {
    Array,
    Class(String),
    EnumVariant { enum_name: String, variant: String },
}

/// An operand: either a place (read) or a constant.
pub enum Operand {
    /// Copy value from place.
    Copy(Place),
    /// Move value from place (consume it).
    Move(Place),
    /// A constant value.
    Constant(Constant),
}

pub enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

/// How a basic block transfers control.
pub enum Terminator {
    /// Unconditional jump to another block.
    Goto {
        target: BlockId,
    },
    /// Conditional branch based on boolean.
    Branch {
        condition: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// Multi-way branch based on integer discriminant.
    Switch {
        discriminant: Operand,
        /// Arms: (value, target block)
        arms: Vec<(i64, BlockId)>,
        /// Default target if no arm matches.
        otherwise: BlockId,
    },
    /// Return from function.
    Return,
    /// Call a function.
    Call {
        callee: Operand,
        args: Vec<Operand>,
        /// Where to store result.
        destination: Place,
        /// Block to jump to after call returns.
        target: BlockId,
        /// Block to jump to if call throws (for catch).
        unwind: Option<BlockId>,
    },
    /// Unreachable code (for exhaustive match).
    Unreachable,
    /// Dispatch an async operation (LLM call) without blocking.
    /// This is a suspend point - control returns to the embedder.
    DispatchFuture {
        callee: Operand,
        args: Vec<Operand>,
        /// Where to store the future handle.
        future: Place,
        /// Block to resume at after dispatch.
        resume: BlockId,
    },
    /// Await a future - suspend until result is ready.
    /// This is a suspend point - control returns to the embedder.
    Await {
        future: Place,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    },
}
```

### MIR Builder API

```rust
/// Builder for constructing MIR functions.
pub struct MirBuilder {
    /// Function being built.
    function: MirFunction,
    /// Current block being populated.
    current_block: BlockId,
}

impl MirBuilder {
    fn new_function(name: String, arity: usize) -> Self {
        // Creates function with entry block (bb0)
    }

    fn new_local(&mut self, name: Option<String>, ty: Ty) -> Local {
        // Allocate a new local variable/temporary
    }

    fn new_block(&mut self) -> BlockId {
        // Create a new basic block
    }

    fn switch_to_block(&mut self, block: BlockId) {
        // Set current block for statement emission
    }

    fn push_assign(&mut self, dest: Place, value: Rvalue) {
        // Add assignment statement to current block
    }

    fn terminate_goto(&mut self, target: BlockId) {
        // Set terminator and seal current block
    }

    fn terminate_branch(&mut self, cond: Operand, then_bb: BlockId, else_bb: BlockId) {
        // Set conditional branch terminator
    }

    fn terminate_switch(&mut self, discr: Operand, arms: Vec<(i64, BlockId)>, otherwise: BlockId) {
        // Set switch terminator
    }

    fn terminate_call(&mut self, callee: Operand, args: Vec<Operand>, dest: Place, target: BlockId, unwind: Option<BlockId>) {
        // Set call terminator
    }

    fn finish(self) -> MirFunction {
        // Finalize and return the MIR function
    }
}
```

### Salsa Integration

The BAML compiler uses [Salsa](https://salsa-rs.github.io/salsa/) for incremental compilation. Each compiler phase (HIR, THIR) defines a database trait with tracked queries. MIR follows this pattern.

#### Database Trait

```rust
/// Database trait for MIR queries.
/// Extends THIR's database to access type information during lowering.
#[salsa::db]
pub trait Db: baml_thir::Db {}
```

The root database in `baml_db` implements all traits:

```rust
#[salsa::db]
impl baml_mir::Db for RootDatabase {}
```

#### Tracked Queries

MIR defines tracked functions that Salsa memoizes and invalidates incrementally:

```rust
/// Tracked: Lower a function from THIR to MIR.
/// This query depends on:
/// - `function_signature` (for parameter types)
/// - `function_body` (for the expression tree)
/// - `infer_function` (for type information)
///
/// It only re-executes when these dependencies change.
#[salsa::tracked]
pub fn lower_function<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> MirFunctionResult<'db> {
    let signature = function_signature(db, function);
    let body = function_body(db, function);
    let inference = infer_function(db, function);

    let mir = MirBuilder::lower(db, &signature, &body, &inference);

    MirFunctionResult::new(db, mir)
}

/// Tracked: Get MIR for all functions in the project.
#[salsa::tracked]
pub fn project_mir<'db>(
    db: &'db dyn Db,
    project: Project,
) -> ProjectMir<'db> {
    let items = project_items(db, project);
    let functions: Vec<_> = items
        .items(db)
        .iter()
        .filter_map(|item| match item {
            ItemId::Function(f) => Some(lower_function(db, *f)),
            _ => None,
        })
        .collect();

    ProjectMir::new(db, functions)
}
```

#### Tracked Result Structs

Following the Salsa 2022 pattern, results containing collections are wrapped in tracked structs:

```rust
/// Tracked struct holding the MIR for a single function.
#[salsa::tracked]
pub struct MirFunctionResult<'db> {
    #[tracked]
    pub mir: MirFunction,
}

/// Tracked struct holding MIR for all project functions.
#[salsa::tracked]
pub struct ProjectMir<'db> {
    #[tracked]
    #[returns(ref)]
    pub functions: Vec<MirFunctionResult<'db>>,
}
```

#### Incrementality Benefits

With Salsa integration, MIR lowering is incremental:

1. **Body-only changes**: If only a function body changes, only that function's MIR is re-lowered. Other functions' MIR is cached.

2. **Signature changes**: If a function signature changes, dependent MIR (callers) may need re-lowering, but unrelated functions are unaffected.

3. **Type changes**: If a class definition changes, only functions using that class re-lower.

```
Edit function foo() body
    → invalidates: function_body(foo)
    → re-executes: lower_function(foo)
    → cached: lower_function(bar), lower_function(baz), ...
```

#### Interned IDs for MIR Entities

For stable cross-function references (e.g., call targets), we use interned IDs:

```rust
/// Interned reference to a MIR function.
/// Stable across incremental updates.
#[salsa::interned]
pub struct MirFunctionId<'db> {
    pub loc: FunctionLoc<'db>,
}
```

## MIR and Loops

### While Loop Lowering

THIR while loop:
```baml
while (condition) {
    body
}
```

Lowers to MIR:
```
bb0 (loop_header):
    _1 = <condition>
    branch _1 -> bb1, bb2

bb1 (loop_body):
    <body statements>
    goto -> bb0

bb2 (loop_exit):
    <continuation>
```

### Break and Continue

Break and continue become simple gotos with no special handling:

```
bb1 (loop_body):
    <statements before break/continue>
    goto -> bb2          // break: goes to loop_exit
    // OR
    goto -> bb0          // continue: goes to loop_header
```

The MIR builder tracks the current loop's header and exit blocks:

```rust
struct LoopContext {
    header: BlockId,    // continue target
    exit: BlockId,      // break target
}
```

### For-In Loop Lowering

For-in loops are already desugared to while loops in HIR. MIR receives:

```baml
// Desugared from: for (x in arr) { body }
{
    let _iter = arr;
    let _len = _iter.length();
    let _i = 0;
    while (_i < _len) {
        let x = _iter[_i];
        body
        _i += 1;
    }
}
```

Which lowers to straightforward MIR blocks following the while pattern.

## MIR and Match Statements

### Proposed Match Syntax

```baml
match (x) {
    c: ClassName => "type binding",
    i: int => "primitive type binding",
    5 => "literal values match",
    u: ClassName | string => "union match",
    Status.Active => "enum variant match",
    other => "catch all binding"
}
```

### Match Pattern Types

```rust
/// A pattern in a match arm.
pub enum MatchPattern {
    /// Bind to a variable with type constraint: `c: ClassName`
    TypeBinding {
        binding: String,
        ty: Ty,
    },
    /// Match a literal value: `5`, `"hello"`, `true`
    Literal(Constant),
    /// Match an enum variant: `Status.Active`
    EnumVariant {
        enum_name: String,
        variant: String,
    },
    /// Match one of several types: `u: ClassName | string`
    Union {
        binding: String,
        types: Vec<Ty>,
    },
    /// Wildcard/catch-all: `_` or `other`
    Wildcard {
        binding: Option<String>,
    },
}

/// A match arm in THIR (before lowering).
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<ExprId>,  // Optional `if` guard
    pub body: ExprId,
}
```

### Match Lowering Algorithm

```rust
fn lower_match(scrutinee: ExprId, arms: &[MatchArm]) -> BlockId {
    // 1. Evaluate scrutinee into a temporary
    // 2. For type-based matching, emit discriminant check
    // 3. Build decision tree of Switch/Branch terminators
    // 4. Each arm body becomes its own block
    // 5. All arm blocks goto a common join block
}
```

### Match Lowering Example

Source:
```baml
match (x) {
    n: int => n + 1,
    s: string => s.length(),
    other => 0
}
```

MIR:
```
bb0 (entry):
    _1 = x
    _2 = discriminant(_1)
    switch _2 -> [INT: bb1, STRING: bb2, otherwise: bb3]

bb1 (int_arm):
    _3 = copy _1 as int
    _4 = _3 + const 1
    goto -> bb4

bb2 (string_arm):
    _5 = copy _1 as string
    _6 = call string.length(_5) -> bb4

bb3 (wildcard_arm):
    _7 = const 0
    goto -> bb4

bb4 (join):
    _result = phi(_4, _6, _7)  // Or explicit assignment before goto
    <continuation>
```

### Enum Variant Matching

Source:
```baml
match (status) {
    Status.Active => "running",
    Status.Idle => "stopped"
}
```

MIR:
```
bb0:
    _1 = status
    _2 = discriminant(_1)
    switch _2 -> [0: bb1, 1: bb2, otherwise: bb3]

bb1 (Active):
    _3 = const "running"
    goto -> bb4

bb2 (Idle):
    _3 = const "stopped"
    goto -> bb4

bb3:
    unreachable    // Exhaustiveness checked in THIR

bb4 (join):
    <use _3>
```

## MIR and Error Handling

### Proposed Catch Syntax

```baml
let x = call_fn_that_can_throw() catch {
    p: ParseError => "default value on parse error",
    n: NetworkError => "default value on net error",
    p: Panic => "catch panic",
};
```

### Two Approaches to Exception Handling in CFGs

There are two main ways compilers model exception handling in Control Flow Graphs:

#### Approach 1: Unwind Edges (Rust, LLVM, C++)

In this model, every instruction that might throw has **two exit paths** encoded directly in the CFG:

1. **Normal path**: Where to go if the operation succeeds
2. **Unwind path**: Where to go if the operation throws

```rust
// Every potentially-throwing call has explicit edges
Terminator::Call {
    callee: Operand,
    destination: Place,
    target: BlockId,           // Success -> bb1
    unwind: Option<BlockId>,   // Failure -> bb_catch
}
```

**Pros:**
- Exception flow is explicit in the CFG
- Enables precise analysis of exception paths
- No runtime table lookups

**Cons:**
- Every call site needs two edges, even if not in a try block
- CFG becomes more complex with many edges
- More difficult to add new throwing operations

#### Approach 2: Exception Tables (JVM, CLR, CPython)

In this model, the CFG remains simple (calls just continue to the next block), but a separate **Exception Table** maps bytecode ranges to handlers:

```rust
struct ExceptionTable {
    entries: Vec<ExceptionTableEntry>,
}

struct ExceptionTableEntry {
    start_pc: usize,        // Protected range start
    end_pc: usize,          // Protected range end
    handler_pc: usize,      // Handler address
    exception_type: Type,   // What to catch
}
```

When an exception occurs at PC `0x0020`:
1. VM scans the exception table
2. Finds entry where `start_pc <= 0x0020 < end_pc`
3. Jumps to `handler_pc` if exception type matches

**Pros:**
- Simpler CFG (no unwind edges on every call)
- Standard approach for bytecode VMs
- Easy to add new exception types

**Cons:**
- Runtime table lookup on exception
- Exception flow not visible in CFG
- Slightly more complex VM implementation

### Chosen Approach: Hybrid (Unwind Edges in MIR, Table in Bytecode)

We use a **hybrid approach**:

1. **In MIR**: Use unwind edges on Call terminators. This makes exception flow explicit during lowering and analysis. Only calls inside `catch` blocks have `unwind: Some(block)`.

2. **During Codegen**: Convert unwind edges to exception table entries. The VM uses table-based dispatch at runtime.

This gives us the best of both worlds:
- **Clear MIR semantics**: Exception paths are explicit in the CFG for analysis
- **Efficient runtime**: Table-based lookup avoids edge overhead in bytecode
- **Simple calls**: Calls outside `catch` blocks have `unwind: None` and no table entry

### Catch Lowering

The `catch` construct uses the Call terminator's `unwind` field:

```rust
Terminator::Call {
    callee: Operand,
    args: Vec<Operand>,
    destination: Place,
    target: BlockId,      // Normal return path
    unwind: Option<BlockId>,  // Exception path
}
```

### Catch Lowering Example

Source:
```baml
let result = risky_call() catch {
    e: ParseError => "parse failed",
    e: NetworkError => "network failed",
};
```

MIR:
```
bb0:
    call risky_call() -> [return: bb1, unwind: bb2]

bb1 (success):
    _result = <call result>
    goto -> bb5

bb2 (unwind_entry):
    _exception = <caught exception>
    _exc_type = discriminant(_exception)
    switch _exc_type -> [PARSE_ERROR: bb3, NETWORK_ERROR: bb4, otherwise: bb6]

bb3 (parse_error_handler):
    _result = const "parse failed"
    goto -> bb5

bb4 (network_error_handler):
    _result = const "network failed"
    goto -> bb5

bb5 (join):
    <use _result>

bb6 (rethrow):
    // Re-raise unhandled exception
    rethrow _exception
```

### Exception Table Generation

During codegen, MIR blocks with `unwind` targets generate exception table entries:

```rust
struct ExceptionTableEntry {
    /// Bytecode range covered by this handler.
    start_pc: usize,
    end_pc: usize,
    /// Handler bytecode address.
    handler_pc: usize,
    /// Exception type to catch (or ALL).
    exception_type: ExceptionType,
}
```

## MIR and Async/Await

### Current LLM Function Model

In BAML, the only truly "async" functions are LLM functions:

```baml
function ClassifyText(text: string) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Classify the following text as positive, negative, or neutral:
        {{ text }}

        Return just the classification.
    "#
}
```

These are called from regular expression functions like any other function:

```baml
function process_text(text: string) -> string {
    let classification = ClassifyText(text);
    classification
}
```

### How LLM Calls Work Today

Under the hood, LLM function calls compile differently from regular calls:

1. **`DISPATCH_FUTURE`** instead of `CALL`: When the VM executes this instruction, it returns control to the embedder (the runtime driving the VM). The embedder schedules the LLM request in the background.

2. **Immediate `AWAIT`**: Currently, every `DISPATCH_FUTURE` is followed immediately by an `AWAIT` instruction. When the VM sees `AWAIT`, it returns control to the embedder saying "await future ID x". The embedder awaits the future, then calls `vm.fulfill_future(id, result)` to provide the result.

```
Current bytecode for: let x = ClassifyText("hello");

LOAD_CONST "hello"
DISPATCH_FUTURE ClassifyText    ; Returns to embedder, schedules LLM call
AWAIT                           ; Returns to embedder, waits for result
STORE_VAR x
```

This is a **cooperative coroutine model**: the VM yields to the embedder at specific points, and the embedder drives execution forward.

### MIR Representation of LLM Calls

In MIR, we model LLM calls with a dedicated `DispatchFuture` terminator that makes the suspend point explicit:

```rust
/// Dispatch an async operation (LLM call) and suspend.
Terminator::DispatchFuture {
    /// The LLM function to call.
    callee: Operand,
    /// Arguments to the function.
    args: Vec<Operand>,
    /// Future handle stored here.
    future: Place,
    /// Block to resume at after dispatch.
    resume: BlockId,
}

/// Await a future and suspend until result is ready.
Terminator::Await {
    /// The future to await.
    future: Place,
    /// Where to store the result.
    destination: Place,
    /// Block to continue at after result is ready.
    target: BlockId,
    /// Block to jump to if the future fails (for catch).
    unwind: Option<BlockId>,
}
```

### Current Behavior: Implicit Await

Today, without an `await` keyword, every LLM call is immediately awaited:

Source:
```baml
function process(text: string) -> string {
    let result = ClassifyText(text);
    result
}
```

MIR (current implicit await):
```
fn process(_1: string) -> string {
    let _0: string;
    let _2: Future<string>;    // Future handle

    bb0: {
        dispatch_future ClassifyText(_1) -> _2 resume bb1;
    }

    bb1: {
        await _2 -> _0 target bb2;
    }

    bb2: {
        return;
    }
}
```

### Future: Explicit Await Syntax

When we add the `await` keyword, users can dispatch multiple LLM calls before awaiting:

```baml
function parallel_classify(texts: string[]) -> string[] {
    // Dispatch all futures (non-blocking)
    let future1 = ClassifyText(texts[0]);
    let future2 = ClassifyText(texts[1]);
    let future3 = ClassifyText(texts[2]);

    // Now await them (could be in any order)
    let result1 = await future1;
    let result2 = await future2;
    let result3 = await future3;

    [result1, result2, result3]
}
```

MIR (explicit await):
```
fn parallel_classify(_1: string[]) -> string[] {
    let _0: string[];
    let _2: Future<string>;   // future1
    let _3: Future<string>;   // future2
    let _4: Future<string>;   // future3
    let _5: string;           // result1
    let _6: string;           // result2
    let _7: string;           // result3

    bb0: {
        _8 = _1[const 0];
        dispatch_future ClassifyText(_8) -> _2 resume bb1;
    }

    bb1: {
        _9 = _1[const 1];
        dispatch_future ClassifyText(_9) -> _3 resume bb2;
    }

    bb2: {
        _10 = _1[const 2];
        dispatch_future ClassifyText(_10) -> _4 resume bb3;
    }

    bb3: {
        // All futures dispatched, now await them
        await _2 -> _5 target bb4;
    }

    bb4: {
        await _3 -> _6 target bb5;
    }

    bb5: {
        await _4 -> _7 target bb6;
    }

    bb6: {
        _0 = [_5, _6, _7];
        return;
    }
}
```

### Await with Error Handling

Combining `await` with `catch` allows handling LLM failures:

```baml
let result = await future catch {
    e: LlmError => "fallback value",
};
```

MIR:
```
bb3: {
    await _2 -> _result target bb4 unwind bb5;
}

bb4 (success):
    goto -> bb6;

bb5 (error_handler):
    _result = const "fallback value";
    goto -> bb6;

bb6 (join):
    <use _result>
```

### Codegen: MIR to Bytecode

The MIR terminators map directly to existing VM instructions:

| MIR Terminator | Bytecode |
|----------------|----------|
| `dispatch_future f(args) -> _fut resume bb` | `DISPATCH_FUTURE f` |
| `await _fut -> _dest target bb` | `AWAIT` |

The key insight is that MIR makes the **suspend points explicit** as block boundaries. Each `dispatch_future` and `await` ends a basic block because control returns to the embedder.

## Human-Readable MIR Format

### Format Specification

```
fn function_name(param0: Type, param1: Type) -> ReturnType {
    // Local declarations
    let _0: ReturnType;              // Return value
    let _1: ParamType = param0;      // Parameter
    let _2: TempType;                // Temporary

    bb0: {
        _2 = const 42;
        _3 = _1 + _2;
        branch _3 > 0 -> bb1, bb2;
    }

    bb1: {
        _0 = call some_function(_3) -> [ok: bb3, err: bb4];
    }

    bb2: {
        _0 = const 0;
        goto -> bb3;
    }

    bb3: {
        return;
    }

    bb4: {
        unreachable;
    }
}
```

### Statement Syntax

```
_dest = <rvalue>;                    // Assignment
drop(_place);                        // Drop/cleanup
nop;                                 // No-op
```

### Rvalue Syntax

```
const <literal>                      // Constant: const 42, const "hello"
copy _local                          // Copy from local
move _local                          // Move from local
_1 + _2                              // Binary op
!_1                                  // Unary op
[_1, _2, _3]                         // Array literal
ClassName { _1, _2 }                 // Class instantiation
discriminant(_1)                     // Get enum discriminant
len(_1)                              // Get array length
```

### Terminator Syntax

```
goto -> bb1;                                           // Unconditional
branch <cond> -> bb1, bb2;                            // Conditional
switch <discr> -> [0: bb1, 1: bb2, otherwise: bb3];   // Multi-way
call func(args) -> [ok: bb1, err: bb2];               // Call with unwind
call func(args) -> bb1;                               // Call without unwind
return;                                               // Return
unreachable;                                          // Should never execute
dispatch_future func(args) -> _fut resume bb1;        // Dispatch LLM call (suspend)
await _fut -> _dest target bb1;                       // Await future (suspend)
await _fut -> _dest target bb1 unwind bb2;            // Await with error handling
```

### Full Example

Source:
```baml
function classify(x: int | string) -> string {
    match (x) {
        n: int => {
            if (n > 0) {
                return "positive";
            }
            return "non-positive";
        }
        s: string => s,
    }
}
```

MIR:
```
fn classify(_1: int | string) -> string {
    let _0: string;
    let _2: int;                     // discriminant temp
    let _3: int;                     // bound 'n'
    let _4: bool;                    // comparison result
    let _5: string;                  // bound 's'

    bb0: {
        _2 = discriminant(_1);
        switch _2 -> [INT: bb1, STRING: bb5, otherwise: bb6];
    }

    bb1: {
        _3 = copy _1 as int;
        _4 = _3 > const 0;
        branch _4 -> bb2, bb3;
    }

    bb2: {
        _0 = const "positive";
        goto -> bb7;
    }

    bb3: {
        _0 = const "non-positive";
        goto -> bb7;
    }

    bb5: {
        _5 = copy _1 as string;
        _0 = copy _5;
        goto -> bb7;
    }

    bb6: {
        unreachable;
    }

    bb7: {
        return;
    }
}
```

## MIR to Bytecode: Simplified Codegen

### Algorithm Overview

```rust
fn codegen_mir_function(mir: &MirFunction) -> Bytecode {
    // Phase 1: Allocate locals to stack slots
    // Phase 2: Emit blocks in order, recording block start addresses
    // Phase 3: Patch jump targets with actual addresses
}
```

### Why This is Simpler

With MIR, codegen becomes mechanical:

1. **No Recursive Tree Walking**: Just iterate through blocks
2. **No Jump Patching During Emit**: Block structure is already determined
3. **No Break/Continue Tracking**: Already resolved to block targets
4. **No Value-Production Analysis**: MIR is explicit about all assignments

### Codegen Example

MIR:
```
bb0: {
    _1 = const 10;
    branch _1 > 0 -> bb1, bb2;
}

bb1: {
    _2 = const "positive";
    goto -> bb3;
}

bb2: {
    _2 = const "negative";
    goto -> bb3;
}

bb3: {
    return;
}
```

Bytecode (with block addresses):
```
; bb0 starts at 0
0: LOAD_CONST 10
1: STORE_VAR 1
2: LOAD_VAR 1
3: LOAD_CONST 0
4: CMP_GT
5: JUMP_IF_FALSE +4      ; -> bb2 at 10

; bb1 starts at 6
6: LOAD_CONST "positive"
7: STORE_VAR 2
8: JUMP +4               ; -> bb3 at 13

; bb2 starts at 10 (patched into instruction 5)
10: LOAD_CONST "negative"
11: STORE_VAR 2
12: JUMP +1              ; -> bb3 at 13

; bb3 starts at 13
13: RETURN
```

### Simplified Codegen Implementation

```rust
fn emit_block(block: &BasicBlock, block_addrs: &HashMap<BlockId, usize>) -> Vec<Instruction> {
    let mut insns = vec![];

    for stmt in &block.statements {
        insns.extend(emit_statement(stmt));
    }

    match &block.terminator {
        Terminator::Goto { target } => {
            let addr = block_addrs[target];
            insns.push(Instruction::Jump(addr));
        }
        Terminator::Branch { condition, then_block, else_block } => {
            insns.extend(emit_operand(condition));
            insns.push(Instruction::JumpIfFalse(block_addrs[else_block]));
            insns.push(Instruction::Jump(block_addrs[then_block]));
        }
        Terminator::Switch { discriminant, arms, otherwise } => {
            // Emit as series of comparisons or jump table
        }
        Terminator::Return => {
            insns.push(Instruction::Return);
        }
        // ... other terminators
    }

    insns
}
```

## Comparison: Before and After MIR

### Before (Direct THIR → Bytecode)

```rust
// Complex recursive compilation with inline jump management
fn compile_expr(&mut self, expr_id: ExprId, body: &ExprBody) {
    match expr {
        Expr::If { condition, then_branch, else_branch } => {
            self.compile_expr(*condition, body);
            let skip_if = self.emit(Instruction::JumpIfFalse(0));  // Placeholder!
            self.emit(Instruction::Pop(1));
            self.compile_expr(*then_branch, body);
            let skip_else = self.emit(Instruction::Jump(0));      // Placeholder!
            self.patch_jump(skip_if);                              // Patch first
            self.emit(Instruction::Pop(1));
            if let Some(else_expr) = else_branch {
                self.compile_expr(*else_expr, body);
            }
            self.patch_jump(skip_else);                            // Patch second
        }
        // ... more complex patterns for loops, calls, etc.
    }
}
```

### After (THIR → MIR → Bytecode)

```rust
// Step 1: Clean lowering to MIR (no bytecode concerns)
fn lower_if(&mut self, condition: ExprId, then_expr: ExprId, else_expr: Option<ExprId>) {
    let cond_local = self.lower_expr(condition);

    let then_block = self.builder.new_block();
    let else_block = self.builder.new_block();
    let join_block = self.builder.new_block();

    self.builder.terminate_branch(cond_local, then_block, else_block);

    self.builder.switch_to_block(then_block);
    let then_val = self.lower_expr(then_expr);
    self.builder.push_assign(result, then_val);
    self.builder.terminate_goto(join_block);

    self.builder.switch_to_block(else_block);
    if let Some(else_e) = else_expr {
        let else_val = self.lower_expr(else_e);
        self.builder.push_assign(result, else_val);
    }
    self.builder.terminate_goto(join_block);

    self.builder.switch_to_block(join_block);
}

// Step 2: Simple codegen from MIR (no control flow logic)
fn emit_mir_function(mir: &MirFunction) -> Bytecode {
    for block in &mir.blocks {
        emit_block(block);
    }
    patch_all_jumps();
}
```

## Implementation Plan

### Phase 1: Core MIR Infrastructure
1. Define MIR data structures (`mir.rs`)
2. Implement MIR builder API
3. Add human-readable MIR printer

### Phase 2: Basic Lowering
1. Lower expressions (literals, binary ops, variables)
2. Lower statements (let, assign, return)
3. Lower if/else expressions
4. Lower while loops with break/continue

### Phase 3: Match Statements
1. Add match syntax to parser/HIR
2. Implement exhaustiveness checking in THIR
3. Lower match to MIR switch blocks

### Phase 4: Error Handling
1. Add catch syntax to parser/HIR
2. Implement catch lowering with unwind blocks
3. Generate exception tables during codegen

### Phase 5: Codegen Migration
1. Implement MIR → Bytecode emission
2. Migrate from direct THIR codegen
3. Remove old direct codegen code

## Backwards Compatibility

This is an internal compiler change. No user-visible syntax or semantics change (except the new match and catch features). Existing BAML code compiles identically.

## Alternatives Considered

### Alternative 1: Extend Current Codegen

We could continue adding complexity to the current direct THIR → bytecode approach. This was rejected because:
- Each new control flow construct requires duplicating jump patching logic
- The code becomes increasingly difficult to maintain and reason about
- Match statement compilation especially benefits from CFG representation

### Alternative 2: SSA Form MIR

We could use Static Single Assignment form where each variable is assigned exactly once. This was rejected because:
- SSA adds complexity (phi nodes, dominance frontiers)
- BAML doesn't need the optimizations SSA enables
- Simple CFG is sufficient for our needs

**Example optimization we don't need:** Constant propagation and folding. With SSA, a compiler can prove that if `_1 = 5` and `_2 = _1 + 3`, then `_2` is always `8`, eliminating the addition at compile time:

```
// Before optimization (SSA)     // After constant folding
_1 = 5                           _3 = 16
_2 = _1 + 3
_3 = _2 * 2
```

SSA requires additional bookkeeping (phi nodes at control flow joins, dominance frontier computation) that adds significant implementation complexity. While these optimizations would save VM cycles, the performance gains don't justify that complexity at this stage. Simple CFG suffices for our current needs. If performance requirements change in the future, we can add SSA—MIR is completely transparent to users, so this would never be a breaking change.

**Refactoring cost to add SSA later:** Converting non-SSA MIR to SSA is a moderate refactor, not trivial but well-understood:

1. **Dominance analysis** - Build a dominator tree. O(n) with Lengauer-Tarjan.
2. **Dominance frontier computation** - Find where phi nodes are needed. O(n) with optimized algorithms.
3. **Phi node insertion** - At join points where a variable has different reaching definitions, insert `_x = phi(_x.1, _x.2)`.
4. **Variable renaming** - Walk the dominator tree, renaming each assignment to a fresh version.

This would require ~500-1000 lines of new code for the core algorithms (Cytron et al. 1991), a `Phi` variant in our MIR data structures, and updates to consumers (pretty printer, codegen). Crucially, our MIR is already block-structured—the hard prerequisite—so the path would simply be: THIR → MIR (current) → SSA pass → optimizations → codegen.

### Alternative 3: Stack-Based IR

We could use a stack-based intermediate representation similar to the final bytecode. This was rejected because:
- Stack-based IRs don't simplify control flow compilation
- We'd still need the same jump patching complexity
- CFG provides clearer separation of concerns

**What is a stack-based IR?** Instead of named locals (`_1 = _2 + _3`), operations push and pop from an implicit stack:

```
// Source: let x = a + b * c

// Register-based (MIR)          // Stack-based
_1 = b                           LOAD b
_2 = c                           LOAD c
_3 = _1 * _2                     MUL
_4 = a                           LOAD a
_5 = _4 + _3                     ADD
                                 STORE x
```

**Why jump patching remains:** Stack-based IRs address *data flow* (how values move), not *control flow* (how execution jumps). Forward jumps still need placeholders:

```
// Source: if (cond) { a() } else { b() }

// Stack-based IR (still has the same problem!)
0: LOAD cond
1: JUMP_IF_FALSE ???    // Don't know target yet!
2: CALL a
3: JUMP ???             // Don't know target yet!
4: CALL b               // Only now can we patch instruction 1
5: ...                  // Only now can we patch instruction 3
```

The jump patching complexity comes from *linear instruction layout*, not from how we represent values. CFG solves this by making control flow *structural* (named block targets) rather than *positional* (byte offsets).
