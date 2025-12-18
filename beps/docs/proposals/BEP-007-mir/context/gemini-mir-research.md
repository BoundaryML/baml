# User
So I have a compiler with these phases: Lexer -> Parser (CST) -> AST (view into CST) -> HIR -> THIR -> Codegen (VM bytecode). Now I'm running into a problem. I want to desugar constructs like for x in array loops into something more codegen friendly, and also I have match statements that need some desugaring. I have added the for loop desugaring into while loops at the AST -> HIR layer transformation. But for match, I need the typechecker to know about all match arms to do exhaustivness analysis and stuff, I can't desugar into if statements. I was basically trying to mimic Rust, which in the playground has a "Show HIR" button and I can see desugared for loops. Match statements are not desugared there, only at the MIR level. But even at MIR Rust still does analysis of borrow checking and stuff. The problem with analyzing desugared code is that when finding errors you have to show the user his source code, not your compiler desugared code. So if I desugar at HIR layer my only option is to carry both the source code and the desugared code nodes, I use source for typechecking and errors, I use desugared for codegen. If there's error in desugared code I guess I can use synthetic spans that refer back to user's code so errors point at it, not at my desugared code. Another solution I thought about is to add another layer after THIR called DHIR (Desugared HIR) or something like that, where errors can no longer happen. But I wonder if I need at some point a CFG MIR where errors can be found, I'm gonna run into the same problem. How do other compilers deal with this? What would be the best approach?

# Model
This is a classic compiler architecture dilemma: **The trade-off between IR simplicity (desugaring early) and diagnostic fidelity (keeping original structure).**

You have correctly identified the standard solution (synthetic spans) and the standard frustration (showing errors on code that "doesn't exist" in the user's file).

Here is how production compilers (like Rust, Swift, and Roslyn) typically solve this, and how you should fit it into your pipeline.

### 1. The Golden Rule: Span Propagation
Your intuition about "synthetic spans" is correct. This is not just a backup option; it is **the** industry standard way to handle this.

You do **not** need to carry both the source node and the desugared node in the AST/HIR. You carry the desugared node, but you carefully assign it the `Span` (file location) of the original source construct.

**Example: Desugaring a `for` loop**
Suppose the user writes:
```rust
// Span: 10..25
for x in arr { ... }
```

When you desugar this into a `while` loop in your AST->HIR phase:
1.  **The `while` node** inherits the span of the `for` keyword (or the whole loop).
2.  **The iterator variable** (hidden) inherits the span of `arr`.
3.  **The `.next()` call** (hidden) inherits the span of `arr` or `in`.

**If a Type Error occurs in the desugared code:**
If `arr` is not an iterator, the type checker (running on HIR/THIR) will complain that "type Array does not have method `.next()`". Because the method call node has the span of `arr`, the error highlights `arr` in the user's source code. The user sees: *"Error: `arr` is not iterable"*—which is perfectly intelligible, even though the user never wrote `.next()`.

### 2. The Desugaring Hierarchy

You should split your desugaring into two distinct buckets. You are currently trying to do everything at once, which is causing the friction with `match`.

#### A. Syntactic Sugar (Early Desugaring)
**Phase:** AST $\to$ HIR
**Strategy:** Desugar things that map purely structurally and don't require type information.
* **Constructs:** `for` loops, `while let`, `if let`, `unless`, self-assign operators (`+=`).
* **Reason:** These can be lowered to `loop`, `match`, and `assignment` immediately. This simplifies your HIR so your type checker has fewer node types to handle.
* **Diagnostics:** As mentioned above, rely on span propagation.

#### B. Semantic Constructs (Late Lowering)
**Phase:** THIR $\to$ MIR (or THIR $\to$ Bytecode)
**Strategy:** Keep these high-level during type checking, then lower them once you have types.
* **Constructs:** `match` expressions, closures (sometimes), `async/await`.
* **Reason:** As you noted, you need the "matrix" of patterns to check exhaustiveness. You cannot easily check exhaustiveness on a series of nested `if/else` statements.
* **Workflow:**
    1.  **HIR:** `match` exists as a tree node.
    2.  **THIR:** Type checker validates types of arms and performs **Exhaustiveness Checking** on the high-level `match` node.
    3.  **Lowering:** Once THIR is validated, you compile the `match` into a Decision Tree or a CFG (Control Flow Graph) of jumps/branches.

### 3. Do you need a "DHIR" or "MIR"?

You mentioned: *THIR $\to$ Codegen (VM Bytecode)*.

The jump from a Typed Tree (THIR) directly to VM Bytecode is often too large if you have complex control flow like `match`. This is where `MIR` (Mid-level IR) comes in. In your nomenclature, "DHIR" is essentially MIR.

I strongly recommend inserting a **CFG-based IR** (Control Flow Graph) after THIR but before Bytecode.

**The Pipeline:**
1.  **AST:** Raw parse tree.
2.  **HIR:** Desugared `for` loops. Everything is a tree. Names are resolved.
3.  **THIR:** Fully typed. `match` is still a high-level node.
    * *Action:* Perform exhaustiveness checks here.
    * *Action:* Report "missing match arm" errors here.
4.  **MIR (Your DHIR):** Lower `match` into jumps/gotos. Flatten nested expressions.
    * *Action:* Borrow checking (if you have it) usually happens here because it needs a Control Flow Graph to understand lifetimes across jumps.
    * *Note:* Even though the code looks nothing like the source, the nodes still hold the original Spans.
5.  **Codegen:** Emit bytecode.

### 4. How to show errors on Desugared MIR

You asked: *"If I desugar... and find errors, I have to show the user his source code."*

This is the hardest part of compiler UX. Here is how you handle it:

**Scenario:** You have lowered `match` into a complex web of GOTO statements in MIR. You run a borrow checker on that MIR and find a use-after-free error.

1.  **The Span:** The specific MIR instruction that causes the error (e.g., `move _1`) holds the span of the variable usage in the source code. You highlight that.
2.  **The Context:** If the error is specific to the desugaring logic (e.g., a variable created *by* the desugaring is invalid), you use a "Note".
    * *Primary Error:* "Borrow of moved value: `x`" (points to user code).
    * *Note:* "This error occurs in the desugaring of a `for` loop." (You can track "Desugaring Origin" in your span data structure).

### Summary of Recommendation

1.  **Desugar `for` loops in AST $\to$ HIR.** Ensure the generated nodes point to the source text spans. This keeps your type checker simple (it doesn't need to know what a `for` loop is).
2.  **Keep `match` in HIR and THIR.** Do your exhaustiveness checking on the high-level representation.
3.  **Create a Lowering Phase (THIR $\to$ MIR).** In this phase:
    * Convert the `match` into a Decision Tree or If-Else chain.
    * Convert expressions into a Control Flow Graph (linear basic blocks).
4.  **Codegen from MIR.**

**Next Step:**
Would you like to see a concrete example of how to implement the **Span propagation** for a `for` loop desugaring to ensure error messages point to the right place?

# User
Okay, I want to understand exactly how an MIR looks like. So, for Rust, I rememeber reading a couple things:

- This blog post: https://blog.rust-lang.org/2016/04/19/MIR/
- And the RFC: https://github.com/rust-lang/rfcs/blob/master/text/1211-mir.md

Also in the Rust playground I can see the MIR.

Given this program:

```rust
enum Status {
    Active(i32),
    Idle
}

fn print(s: &Status) {
    for _ in 0..10 {
        let s = match s {
            Status::Active(n) => String::from("active"),
            Status::Idle => String::from("idle")
        };
    }
}
```

It shows me this MIR:

```
// WARNING: This output format is intended for human consumers only
// and is subject to change without notice. Knock yourself out.
// HINT: See also -Z dump-mir for MIR at specific points during compilation.
fn print(_1: &Status) -> () {
    debug s => _1;
    let mut _0: ();
    let mut _2: std::ops::Range<i32>;
    let mut _3: std::ops::Range<i32>;
    let mut _4: std::ops::Range<i32>;
    let mut _5: std::option::Option<i32>;
    let mut _6: &mut std::ops::Range<i32>;
    let mut _7: isize;
    let mut _9: isize;
    scope 1 {
        debug iter => _4;
        let _8: std::string::String;
        let _10: &i32;
        scope 2 {
            debug s => _8;
        }
        scope 3 {
            debug n => _10;
        }
    }

    bb0: {
        _3 = std::ops::Range::<i32> { start: const 0_i32, end: const 10_i32 };
        _2 = <std::ops::Range<i32> as IntoIterator>::into_iter(move _3) -> [return: bb1, unwind continue];
    }

    bb1: {
        _4 = move _2;
        goto -> bb2;
    }

    bb2: {
        _6 = &mut _4;
        _5 = <std::ops::Range<i32> as Iterator>::next(copy _6) -> [return: bb3, unwind continue];
    }

    bb3: {
        _7 = discriminant(_5);
        switchInt(move _7) -> [0: bb6, 1: bb5, otherwise: bb4];
    }

    bb4: {
        unreachable;
    }

    bb5: {
        _9 = discriminant((*_1));
        switchInt(move _9) -> [0: bb8, 1: bb7, otherwise: bb4];
    }

    bb6: {
        return;
    }

    bb7: {
        _8 = <String as From<&str>>::from(const "idle") -> [return: bb9, unwind continue];
    }

    bb8: {
        _10 = &(((*_1) as Active).0: i32);
        _8 = <String as From<&str>>::from(const "active") -> [return: bb9, unwind continue];
    }

    bb9: {
        drop(_8) -> [return: bb2, unwind continue];
    }
}

alloc2 (size: 4, align: 1) {
    69 64 6c 65                                     │ idle
}

alloc1 (size: 6, align: 1) {
    61 63 74 69 76 65                               │ active
}

fn Status::Active(_1: i32) -> Status {
    let mut _0: Status;

    bb0: {
        _0 = Status::Active(move _1);
        return;
    }
}

// MIR FOR CTFE
fn Status::Active(_1: i32) -> Status {
    let mut _0: Status;

    bb0: {
        _0 = Status::Active(move _1);
        return;
    }
}
```

# Model
This is an excellent example of MIR. You’ve struck gold with this snippet because it perfectly demonstrates **control flow flattening**, which is exactly what you need to solve your `match` and `for` loop desugaring problems.

Here is the breakdown of exactly what you are looking at and how it solves the architectural problems you described.

### 1. The Anatomy of MIR

Unlike your AST or HIR (which are trees), MIR is a **Control Flow Graph (CFG)**. This means the concept of "nesting" (nested `if`s, nested loops) is completely gone.

* **Locals (`_0`, `_1`...):** All variables are declared at the top.
    * `_0`: Always the return value.
    * `_1`...`_n`: Arguments to the function.
    * `_n+1`...: Temporary variables generated by the compiler. Notice how many there are (`_2` to `_10`). MIR is very explicit about memory.
* **Basic Blocks (`bb0`, `bb1`...):** The code is sliced into chunks. A "Basic Block" is a sequence of straight-line code that **always** ends with a "Terminator" (a jump, a switch, or a return).
* **Terminators:** These are the traffic cops. `goto`, `switchInt`, `return`, `unreachable`.

### 2. How `match` is Desugared (The `switchInt`)

You mentioned you were stuck on how to desugar `match` while keeping semantics. Look at **`bb5`** in your output. This is the heart of your `match s { ... }`.

```rust
bb5: {
    // 1. Read the "tag" or variant index of the enum
    _9 = discriminant((*_1));

    // 2. Branch based on that integer
    switchInt(move _9) -> [0: bb8, 1: bb7, otherwise: bb4];
}
```

* **`discriminant`:** This is a special MIR instruction that extracts the integer value of the enum variant (e.g., `Active`=0, `Idle`=1).
* **`switchInt`:** This is the low-level version of `match`. It takes an integer and jumps to a specific block.
    * If `0` (Active) $\to$ Go to `bb8` (where the code for `Active` lives).
    * If `1` (Idle) $\to$ Go to `bb7` (where the code for `Idle` lives).
    * `otherwise` $\to$ Go to `bb4` (which contains `unreachable`, handling the "impossible" case).

**Why this helps you:**
By the time you get to MIR, you don't need to know about "patterns" or "exhaustiveness." You already checked that in THIR. Now you just need raw integer comparisons and jumps. This is incredibly friendly for Codegen.

### 3. How `for` is Desugared (The Loop)

You can see the `for` loop has been completely exploded into the `Iterator` protocol.

* **Setup (`bb0`):** Creates the range `0..10` (`_3`) and converts it into an iterator (`_2`).
* **Loop Header (`bb2`):** This is where the loop "starts".
* **The Check (`bb3`):** This is the desugared condition.
    ```rust
    bb3: {
        // Call .next() on the iterator
        _5 = <...>::next(copy _6) -> ...;

        // Check if result is Some (1) or None (0)
        _7 = discriminant(_5);
        switchInt(move _7) -> [0: bb6, 1: bb5, otherwise: bb4];
    }
    ```
    * If `0` (None) $\to$ Go to `bb6` (Exit loop/Return).
    * If `1` (Some) $\to$ Go to `bb5` (Execute loop body / The Match).

### 4. How to Apply This to Your Compiler

You are currently struggling because you are trying to do two incompatible things in one IR:
1.  **High-Level Analysis:** Type checking, exhaustiveness (Requires `match` to be a tree).
2.  **Low-Level Execution:** Bytecode generation (Requires `match` to be jumps).

**The Solution: Add the MIR Phase**

You should insert a phase between THIR and Codegen.

**Phase 1: THIR (Tree-based, Typed)**
* Keep `match` as a complex node: `Match { scrutinizer, arms: [...] }`.
* **Error Checking:** Run your exhaustiveness check here. If a case is missing, you have the tree structure to easily report *"Missing match arm for `Status::Idle`"*.

**Phase 2: MIR Construction (The "Lowering" step)**
* This is where you convert the Tree into the Graph.
* **Algorithm for `match`:**
    1.  Create a "start" block (e.g., `bb5`).
    2.  Emit `discriminant` instruction.
    3.  Create a block for each arm (`bb7`, `bb8`).
    4.  Emit a `switchInt` in the start block pointing to those arm blocks.
    5.  Recursively lower the code inside the arms.
    6.  Make all arms jump to a common "merge" block (end of match) or loop header.

**Phase 3: Codegen**
* Now Codegen is trivial! You just iterate over the list of Basic Blocks.
* `switchInt` becomes a `jump_table` or `cmp/jmp` instructions in your VM.
* `goto` becomes a `jmp`.
* You don't need to manage recursion or stack depth in your codegen logic because the MIR is already flat.

### 5. Handling Errors in Desugared Code

Revisiting your worry about errors:
> "If there's error in desugared code... errors point at it, not at my desugared code."

In MIR, every statement carries a `Span`.

* **Example:** In `bb3`, the call to `next()` is generated by the compiler.
* **The Span:** You attach the span of the `for` keyword (or the `in` keyword) from the original AST to this `next()` call instruction in the MIR.
* **The Error:** If `next()` fails (e.g., borrow checker says the iterator is moved), the error engine looks at the MIR instruction, grabs the span (which points to `for`), and prints:
    *"Error at line X: Borrow of moved value in for loop"*.

The user never sees `bb3` or `next()`. They just see their loop highlighted.

### Summary Recommendation

Do not try to desugar `match` inside your AST or HIR. It is too early.

1.  **Keep `match` structured** until THIR is finished.
2.  **Build a MIR** (Control Flow Graph) specifically for lowering `match` into `switches` and `loops` into `gotos`.
3.  **Codegen from MIR**. It will make your VM bytecode generation incredibly simple (almost 1-to-1 mapping).

# User
Interesting. I know CPython uses CFG as well, can you show me an example

# Model
CPython absolutely uses a CFG, though it exposes it a bit differently than Rust. In Python, the CFG generation happens deep inside the compiler (in `compile.c`) just before the final bytecode is emitted.

While Rust's MIR is a distinct intermediate language you can dump and read, Python's CFG is usually invisible unless you disassemble the final bytecode. However, the structure is exactly the same: **Basic Blocks** connected by **Jumps**.

Here is an example using Python's `dis` (disassembler) module, annotated to show you the hidden Control Flow Graph.

### The Python Source
Let's use a loop with a conditional, similar to your Rust example, to see how the control flow splits and merges.

```python
def generic_example(x):
    while x > 0:
        if x % 2 == 0:
            print("even")
        else:
            print("odd")
        x -= 1
```

### The "CFG" (Disassembled Bytecode)
If you run `import dis; dis.dis(generic_example)`, you get the linear bytecode. I have grouped the instructions below into **Basic Blocks (BB)** so you can see the graph structure that the compiler built.

**Note:** The `>>` arrows indicate jump targets (entry points to a basic block).

#### **BB0: Loop Setup & Header**
This is the entry point. It immediately checks the loop condition.
```text
  2           0 SETUP_LOOP              36 (to 38)   # Pushes a block for 'break' handling
        >>    2 LOAD_FAST                0 (x)
              4 LOAD_CONST               1 (0)
              6 COMPARE_OP               4 (>)
              8 POP_JUMP_IF_FALSE       36           # TERMINATOR: If False, Jump to BB4 (End)
```
* **Edges:**
    * True $\to$ **BB1** (Inside Loop)
    * False $\to$ **BB4** (Exit Loop)

#### **BB1: The `if` Condition**
We entered the loop. Now we check the `if`.
```text
  3          10 LOAD_FAST                0 (x)
             12 LOAD_CONST               2 (2)
             14 BINARY_MODULO
             16 LOAD_CONST               1 (0)
             18 COMPARE_OP               2 (==)
             20 POP_JUMP_IF_FALSE       30           # TERMINATOR: If False, Jump to BB3 (Else)
```
* **Edges:**
    * True $\to$ **BB2** (If Body)
    * False $\to$ **BB3** (Else Body)

#### **BB2: The `then` Branch**
The "True" path of the if statement.
```text
  4          22 LOAD_GLOBAL              0 (print)
             24 LOAD_CONST               3 ('even')
             26 CALL_FUNCTION            1
             28 POP_TOP
             29 JUMP_ABSOLUTE           32           # TERMINATOR: Jump to BB3-Merge
```
* **Edges:**
    * Always $\to$ **BB3-Merge** (Instruction 32)

#### **BB3: The `else` Branch & Merge**
Notice this block handles the `else` logic *and* the code that happens after the if/else (the decrement).
```text
  6     >>   30 LOAD_GLOBAL              0 (print)   # Target of "else" jump
             32 LOAD_CONST               4 ('odd')
             34 CALL_FUNCTION            1
             36 POP_TOP

  7     >>   32 LOAD_FAST                0 (x)       # Target of "then" jump (Merge point)
             34 LOAD_CONST               5 (1)
             36 INPLACE_SUBTRACT
             38 STORE_FAST               0 (x)
             40 JUMP_ABSOLUTE            2           # TERMINATOR: Jump back to BB0 (Loop Header)
```

#### **BB4: Exit**
Where `POP_JUMP_IF_FALSE` sends us when `x > 0` is finally false.
```text
        >>   36 POP_BLOCK                            # Clean up loop scope
        >>   38 LOAD_CONST               0 (None)
             40 RETURN_VALUE
```

### Key Takeaways for Your Compiler

1.  **Linearization:** CPython's compiler builds the graph in memory (objects pointing to other objects), but the final output is a flat array of bytes. The "jumps" are just integer offsets (e.g., `POP_JUMP_IF_FALSE 30` means "goto index 30").
2.  **Stack Depth Calculation:** One of the main reasons CPython builds a CFG is to calculate the maximum stack depth required for the frame. It traverses the graph to see how many items are pushed/popped in every possible path.
3.  **Basic Block structure:**
    * **Rust MIR:** Explicit `bb0`, `bb1` labels.
    * **CPython:** Implicit blocks defined by jump targets. Any instruction that is the *target* of a jump starts a new Basic Block. Any instruction that *jumps* ends a Basic Block.

### How this helps your "DHIR/MIR" design
You don't need to output fancy graphviz diagrams or text like Rust does. For your compiler, your "MIR" can simply be a list of `BasicBlock` objects:

```rust
struct BasicBlock {
    instructions: Vec<Instruction>,
    terminator: Terminator, // Jump, Branch, Return
}

struct MirFunction {
    blocks: Vec<BasicBlock>, // Just a flat list!
}
```

When you generate code (VM bytecode), you just:
1.  Iterate through the `blocks`.
2.  Keep a map of `BlockID -> BytecodeOffset`.
3.  Emit instructions.
4.  When you hit a Terminator (Jump), emit a placeholder.
5.  Once done, go back and patch the placeholders with the real `BytecodeOffset` you calculated.

This separates the complex logic of "how flow works" (MIR) from the boring logic of "counting bytes" (Codegen).

# User
Additional question: Can this be used for try-catch or that's handled elsewhere?

# Model
Yes, absolutely. In fact, MIR (or any CFG-based IR) is the **best** place to model exception handling.

Handling `try-catch` in a tree-based structure (AST/HIR) is deceptively simple: it looks like just another block. But in terms of control flow, exceptions are effectively "goto-from-anywhere." Modeling this in a CFG is standard practice.

There are two main ways compilers handle this in the CFG. You should choose the one that fits your VM architecture.

---

### 1. The "Unwind Edge" Approach (Rust, LLVM, C++)
In this model, every Basic Block that contains a function call (or an instruction that *might* throw) has **two** exit paths:
1.  **Normal Control Flow:** Where to go if the function returns successfully.
2.  **Unwind Control Flow:** Where to go if the function throws an exception.

If you look back at the Rust MIR output I showed you earlier, notice the `unwind continue` annotations?
```rust
_2 = <...>::into_iter(move _3) -> [return: bb1, unwind continue];
```
This instruction says: "If this call succeeds, goto `bb1`. If it panics/throws, continue bubbling the exception up the stack."

If this code were inside a `try` block (or `catch_unwind` in Rust), the `unwind` target would point to a specific **Cleanup Block** (your catch handler) instead of just "continue."

#### How it looks in your MIR
You would modify your `Terminator` or `Call` instruction to hold two targets:
```rust
enum Terminator {
    Goto(BlockId),
    Return,
    // A call that might crash/throw
    Call {
        func: String,
        args: Vec<Operand>,
        destination: BlockId, // Normal path
        cleanup: Option<BlockId>, // "Catch" path (if inside a try block)
    }
}
```

### 2. The Table-Based Approach (Java JVM, CPython, C# CLR)
This is likely the approach you want if you are building a VM.

Instead of drawing explicit arrows in the graph for every single instruction that might fail, you generate "ranges" of bytecode that are protected.

1.  **MIR Phase:** You designate specific Basic Blocks as "protected." You associate them with a "Handler Block."
2.  **Codegen Phase:** You generate an **Exception Table**.

**The Exception Table** usually looks like this:
| Start PC | End PC | Handler PC | Exception Type |
| :--- | :--- | :--- | :--- |
| 0x0010 | 0x0050 | 0x0090 | `ValueError` |
| 0x0010 | 0x0050 | 0x00A5 | `IOError` |

**How the VM uses it:**
When an exception is thrown at PC `0x0020`:
1.  The VM pauses execution.
2.  It scans the Exception Table.
3.  "Is `0x0020` inside `0x0010`...`0x0050`?" -> **Yes**.
4.  "Is the exception a `ValueError`?" -> **Yes**.
5.  Set PC to `0x0090` (The Handler).

### Example: Desugaring `try-catch` to MIR

Let's assume a simplified Python-like syntax:
```python
try:
    danger()
    print("Success")
catch Error:
    handle_error()
print("Done")
```

Here is how this looks as a **Control Flow Graph**:

**BB0 (Start of Try):**
* This block is marked as "Protected" (conceptually).
* Instruction: `Call danger()`.
* *Note:* If `danger()` throws, the VM/Runtime looks up the table and jumps to `BB2`.
* Instruction: `Goto BB1`.

**BB1 (Rest of Try - "Success"):**
* Instruction: `Print "Success"`.
* Instruction: `Goto BB3` (Skip the catch block).

**BB2 (The Catch Block):**
* *Entry:* This is where the VM jumps if an error happened in BB0.
* Instruction: `Call handle_error()`.
* Instruction: `Goto BB3`.

**BB3 (Join/Finally):**
* Instruction: `Print "Done"`.
* Instruction: `Return`.

### Summary: Where does this logic live?

It lives in **MIR**, but the specific implementation details affect **Codegen**.

1.  **In AST/HIR:** `TryCatch` is just a node with a `body` and a `handler`.
2.  **In MIR Construction (Lowering):**
    * You create a new Basic Block for the `catch` body.
    * You emit the code for the `try` body.
    * **Crucial Step:** You register the `try` blocks in a list of "Landing Pads" or "Exception Scopes."
3.  **In Codegen:**
    * You compute the exact Bytecode Offsets (Start/End) for the blocks inside the `try`.
    * You write those offsets into the **Exception Table** at the end of your bytecode file.

**Recommendation for your compiler:**
Use the **Table-Based Approach**. It keeps your MIR graph cleaner (you don't need double edges on every call) and it is the standard for virtually all bytecode interpreters (JVM, CLR, Python). The VM does the heavy lifting of checking the table when an exception actually occurs.