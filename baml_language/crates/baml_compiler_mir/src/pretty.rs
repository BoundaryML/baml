//! Human-readable MIR pretty printer.
//!
//! Outputs MIR in a format similar to Rust's MIR dumps:
//!
//! ```text
//! fn example(x: int) -> string {
//!     let _0: string;
//!     let _1: int;
//!
//!     bb0: {
//!         _2 = _1 > const 0;
//!         branch _2 -> bb1, bb2;
//!     }
//!
//!     bb1: {
//!         _0 = const "positive";
//!         goto -> bb3;
//!     }
//!     ...
//! }
//! ```

use std::fmt::{self, Write};

use crate::{
    AggregateKind, BasicBlock, Constant, Local, LocalDecl, MirFunction, Operand, Rvalue, Statement,
    StatementKind, Terminator,
};

/// Pretty print a MIR function.
pub fn display_function(func: &MirFunction) -> String {
    let mut output = String::new();
    let _ = write_function(&mut output, func);
    output
}

/// Write a MIR function to a formatter.
pub fn write_function(f: &mut impl Write, func: &MirFunction) -> fmt::Result {
    // Function header
    write!(f, "fn {}(", func.name)?;

    // Parameters (_1 through _arity)
    for i in 1..=func.arity {
        if i > 1 {
            write!(f, ", ")?;
        }
        // Guard against missing locals in error recovery cases
        if i < func.locals.len() {
            let local = &func.locals[i];
            write_local_decl_inline(f, Local(i), local)?;
        } else {
            write!(f, "_{i}: <missing>")?;
        }
    }

    // Return type from _0
    write!(f, ")")?;
    if !func.locals.is_empty() {
        let ret = &func.locals[0];
        write!(f, " -> {:?}", ret.ty)?;
    }
    writeln!(f, " {{")?;

    // Local declarations
    writeln!(f, "    // Locals:")?;
    for (i, local) in func.locals.iter().enumerate() {
        write!(f, "    let _{i}: {:?}", local.ty)?;
        if let Some(name) = &local.name {
            write!(f, " // {name}")?;
        }
        if i == 0 {
            write!(f, " // return")?;
        } else if i <= func.arity {
            write!(f, " // param")?;
        }
        writeln!(f)?;
    }
    writeln!(f)?;

    // Basic blocks
    for block in &func.blocks {
        write_block(f, block)?;
        writeln!(f)?;
    }

    writeln!(f, "}}")?;
    Ok(())
}

fn write_local_decl_inline(f: &mut impl Write, id: Local, decl: &LocalDecl) -> fmt::Result {
    if let Some(name) = &decl.name {
        write!(f, "{name}: {:?}", decl.ty)
    } else {
        write!(f, "{id}: {:?}", decl.ty)
    }
}

fn write_block(f: &mut impl Write, block: &BasicBlock) -> fmt::Result {
    writeln!(f, "    {}: {{", block.id)?;

    for stmt in &block.statements {
        write!(f, "        ")?;
        write_statement(f, stmt)?;
        writeln!(f)?;
    }

    if let Some(term) = &block.terminator {
        write!(f, "        ")?;
        write_terminator(f, term)?;
        writeln!(f)?;
    } else {
        writeln!(f, "        // unterminated")?;
    }

    writeln!(f, "    }}")?;
    Ok(())
}

fn write_statement(f: &mut impl Write, stmt: &Statement) -> fmt::Result {
    match &stmt.kind {
        StatementKind::Assign { destination, value } => {
            write!(f, "{destination} = ")?;
            write_rvalue(f, value)?;
            write!(f, ";")
        }
        StatementKind::Drop(place) => {
            write!(f, "drop({place});")
        }
        StatementKind::Unwatch(local) => {
            write!(f, "unwatch({local});")
        }
        StatementKind::NotifyBlock { name, level } => {
            write!(f, "notify_block({name}, level={level});")
        }
        StatementKind::WatchOptions { local, filter } => {
            write!(f, "{local}.$watch.options(")?;
            write_operand(f, filter)?;
            write!(f, ");")
        }
        StatementKind::WatchNotify(local) => {
            write!(f, "{local}.$watch.notify();")
        }
        StatementKind::VizEnter(idx) => {
            write!(f, "viz_enter({idx});")
        }
        StatementKind::VizExit(idx) => {
            write!(f, "viz_exit({idx});")
        }
        StatementKind::Nop => {
            write!(f, "nop;")
        }
        StatementKind::Assert(operand) => {
            write!(f, "assert(")?;
            write_operand(f, operand)?;
            write!(f, ");")
        }
    }
}

fn write_terminator(f: &mut impl Write, term: &Terminator) -> fmt::Result {
    match term {
        Terminator::Goto { target } => {
            write!(f, "goto -> {target};")
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            write!(f, "branch ")?;
            write_operand(f, condition)?;
            write!(f, " -> [{then_block}, {else_block}];")
        }
        Terminator::Switch {
            discriminant,
            arms,
            otherwise,
            exhaustive,
        } => {
            write!(f, "switch ")?;
            write_operand(f, discriminant)?;
            write!(f, " [")?;
            for (i, (val, target)) in arms.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{val}: {target}")?;
            }
            if *exhaustive {
                write!(f, ", otherwise: {otherwise}] (exhaustive);")
            } else {
                write!(f, ", otherwise: {otherwise}];")
            }
        }
        Terminator::Return => {
            write!(f, "return;")
        }
        Terminator::Call {
            callee,
            args,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = call ")?;
            write_operand(f, callee)?;
            write!(f, "(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
            }
            write!(f, ") -> [{target}")?;
            if let Some(u) = unwind {
                write!(f, ", unwind: {u}")?;
            }
            write!(f, "];")
        }
        Terminator::Unreachable => {
            write!(f, "unreachable;")
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            resume,
        } => {
            write!(f, "{future} = dispatch_future ")?;
            write_operand(f, callee)?;
            write!(f, "(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
            }
            write!(f, ") -> {resume};")
        }
        Terminator::Await {
            future,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = await {future} -> [{target}")?;
            if let Some(u) = unwind {
                write!(f, ", unwind: {u}")?;
            }
            write!(f, "];")
        }
    }
}

fn write_rvalue(f: &mut impl Write, rvalue: &Rvalue) -> fmt::Result {
    match rvalue {
        Rvalue::Use(operand) => write_operand(f, operand),
        Rvalue::BinaryOp { op, left, right } => {
            write_operand(f, left)?;
            write!(f, " {op} ")?;
            write_operand(f, right)
        }
        Rvalue::UnaryOp { op, operand } => {
            write!(f, "{op}")?;
            write_operand(f, operand)
        }
        Rvalue::Array(elements) => {
            write!(f, "[")?;
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, elem)?;
            }
            write!(f, "]")
        }
        Rvalue::Map(entries) => {
            write!(f, "{{ ")?;
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, key)?;
                write!(f, ": ")?;
                write_operand(f, value)?;
            }
            write!(f, " }}")
        }
        Rvalue::Aggregate { kind, fields } => {
            match kind {
                AggregateKind::Array => write!(f, "array")?,
                AggregateKind::Class(name) => write!(f, "{name}")?,
                AggregateKind::EnumVariant { enum_name, variant } => {
                    write!(f, "{enum_name}::{variant}")?;
                }
            }
            write!(f, " {{ ")?;
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, field)?;
            }
            write!(f, " }}")
        }
        Rvalue::Discriminant(place) => {
            write!(f, "discriminant({place})")
        }
        Rvalue::TypeTag(place) => {
            write!(f, "type_tag({place})")
        }
        Rvalue::Len(place) => {
            write!(f, "len({place})")
        }
        Rvalue::IsType { operand, ty } => {
            write!(f, "is_type(")?;
            write_operand(f, operand)?;
            write!(f, ", {ty:?})")
        }
    }
}

fn write_operand(f: &mut impl Write, operand: &Operand) -> fmt::Result {
    match operand {
        Operand::Copy(place) => write!(f, "copy {place}"),
        Operand::Move(place) => write!(f, "move {place}"),
        Operand::Constant(c) => write_constant(f, c),
    }
}

fn write_constant(f: &mut impl Write, constant: &Constant) -> fmt::Result {
    match constant {
        Constant::Int(n) => write!(f, "const {n}_i64"),
        Constant::Float(n) => write!(f, "const {n}_f64"),
        Constant::String(s) => write!(f, "const {s:?}"),
        Constant::Bool(b) => write!(f, "const {b}"),
        Constant::Null => write!(f, "const null"),
        Constant::Function(name) => write!(f, "const fn {name}"),
        Constant::EnumVariant { enum_name, variant } => write!(f, "const {enum_name}.{variant}"),
        Constant::Ty(ty) => write!(f, "const type {ty:?}"),
    }
}

// ============================================================================
// Display implementations
// ============================================================================

impl fmt::Display for MirFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use a String buffer since fmt::Formatter doesn't implement Write
        let mut buf = String::new();
        write_function(&mut buf, self).map_err(|_| fmt::Error)?;
        f.write_str(&buf)
    }
}
