//! Ahead-of-time native code for BAML programs: optimized MIR in, a native
//! object file out, through Cranelift.
//!
//! What is compiled so far: `int`, `float`, `bool`, `null` and `string`
//! values, instances of classes without type parameters, arrays of any of
//! these, and optional references; arithmetic and comparisons, field and
//! index access, branches, int switches, and direct calls between functions
//! that stay inside that subset. [`coverage`] says, for a program, which
//! reachable functions fall outside it and why; [`compile`] refuses a
//! program with any.
//!
//! The compiled object is host-only and is linked by [`link`] against the
//! runtime in `baml_native_rt`, which owns the heap and its collector.

mod check;
mod codegen;
/// Object and mutator layout, shared with the runtime by including its file.
#[path = "../../baml_native_rt/src/layout.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    reason = "the runtime's file: it exports and uses every constant, compiled code only some"
)]
mod layout;
mod link;
mod loops;
mod program;
mod types;

use std::fmt;

use baml_compiler2_hir::loc::FunctionLoc;

pub use crate::link::{LinkError, link};

/// A MIR construct native code is not generated for yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocker {
    /// What kind of construct it is, in words that group across functions.
    pub construct: String,
    /// The instance: a local, a callee, an operator.
    pub detail: String,
}

impl Blocker {
    pub(crate) fn new(construct: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            f.write_str(&self.construct)
        } else {
            write!(f, "{} ({})", self.construct, self.detail)
        }
    }
}

/// A reachable function that cannot be compiled, and why.
#[derive(Clone, Debug)]
pub struct BlockedFunction {
    pub function: String,
    /// Each distinct construct that blocks it, in the order met; never empty.
    pub blockers: Vec<Blocker>,
}

/// Which of the functions a program's entry reaches can be compiled.
#[derive(Clone, Debug)]
pub struct Coverage {
    /// How many functions the entry reaches through direct calls, itself
    /// included.
    pub reachable: usize,
    /// The reachable functions outside the subset, the entry first if it is
    /// one, then in breadth-first call order.
    pub blocked: Vec<BlockedFunction>,
}

impl Coverage {
    pub fn is_supported(&self) -> bool {
        self.blocked.is_empty()
    }
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} of {} reachable functions are outside the native subset:",
            self.blocked.len(),
            self.reachable
        )?;
        for blocked in &self.blocked {
            write!(f, "\n  {}: {}", blocked.function, blocked.blockers[0])?;
        }
        Ok(())
    }
}

fn coverage_of(program: &program::Program<'_>) -> Coverage {
    Coverage {
        reachable: program.functions.len(),
        blocked: program
            .functions
            .iter()
            .filter(|function| !function.blockers.is_empty())
            .map(|function| BlockedFunction {
                function: function.name.clone(),
                blockers: function.blockers.clone(),
            })
            .collect(),
    }
}

/// Check every function `entry` reaches against the native subset.
pub fn coverage<'db>(db: &'db dyn baml_compiler2_mir::Db, entry: FunctionLoc<'db>) -> Coverage {
    coverage_of(&program::collect(db, entry))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompileOptions {
    /// Keep the Cranelift IR of every function, as text.
    pub emit_clif: bool,
    /// Keep the machine code of every function, as an assembly listing.
    pub emit_disassembly: bool,
}

/// A program compiled for the host.
#[derive(Debug)]
pub struct Compiled {
    /// A relocatable object file in the host's format.
    pub object: Vec<u8>,
    /// How many BAML functions it holds.
    pub functions: usize,
    pub clif: Option<String>,
    pub disassembly: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("cannot compile to native code: {0}")]
    Unsupported(Coverage),
    #[error("the entry function of a native program takes no arguments")]
    EntryTakesArguments,
    #[error("the entry function of a native program returns an int, float, bool, string or null")]
    EntryResultNotPrintable,
    /// A failure inside the backend: never the program's fault.
    #[error("native code generation failed: {0}")]
    Codegen(String),
}

/// Compile `entry` and everything it reaches into one object file.
pub fn compile<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    entry: FunctionLoc<'db>,
    options: CompileOptions,
) -> Result<Compiled, CompileError> {
    let program = program::collect(db, entry);
    let coverage = coverage_of(&program);
    if !coverage.is_supported() {
        return Err(CompileError::Unsupported(coverage));
    }
    let output = codegen::generate(db, &program, options)?;
    Ok(Compiled {
        object: output.object,
        functions: program.functions.len(),
        clif: output.clif,
        disassembly: output.disassembly,
    })
}
