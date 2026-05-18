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
    AggregateKind, BasicBlock, BuiltinKind, Constant, IntrinsicOp, Local, LocalDecl, LogLevel,
    MirFunction, MirFunctionBody, MirFunctionKind, Operand, Rvalue, Statement, StatementKind,
    Terminator,
};

/// Pretty print a MIR function.
pub fn display_function(func: &MirFunction) -> String {
    let mut output = String::new();
    let _ = write_function(&mut output, func);
    output
}

/// Write a MIR function to a formatter.
pub fn write_function(f: &mut impl Write, func: &MirFunction) -> fmt::Result {
    match &func.kind {
        MirFunctionKind::Builtin(kind) => {
            let kind_str = match kind {
                BuiltinKind::Io => "io",
                BuiltinKind::Vm => "vm",
                BuiltinKind::Intrinsic => "intrinsic",
            };
            writeln!(f, "fn {} = builtin({kind_str})", func.item_ref)
        }
        MirFunctionKind::Bytecode(body) => write_bytecode_function(f, func, body),
    }
}

/// Write the bytecode body of a MIR function.
fn write_bytecode_function(
    f: &mut impl Write,
    func: &MirFunction,
    body: &MirFunctionBody,
) -> fmt::Result {
    // Function header
    write!(f, "fn {}(", func.item_ref)?;

    // Parameters (_1 through _arity)
    for i in 1..=func.arity {
        if i > 1 {
            write!(f, ", ")?;
        }
        // Guard against missing locals in error recovery cases
        if i < body.locals.len() {
            let local = &body.locals[i];
            write_local_decl_inline(f, Local(i), local)?;
        } else {
            write!(f, "_{i}: <missing>")?;
        }
    }

    // Return type from _0
    write!(f, ")")?;
    if !body.locals.is_empty() {
        let ret = &body.locals[0];
        write!(f, " -> {}", ret.ty)?;
    }
    writeln!(f, " {{")?;

    // Local declarations
    writeln!(f, "    // Locals:")?;
    for (i, local) in body.locals.iter().enumerate() {
        write!(f, "    let _{i}: {}", local.ty)?;
        if let Some(name) = &local.name {
            write!(f, " // {name}")?;
        }
        if i == 0 {
            write!(f, " // return")?;
        } else if i <= func.arity {
            write!(f, " // param")?;
        }
        if local.is_captured {
            write!(f, " [captured]")?;
        }
        writeln!(f)?;
    }
    writeln!(f)?;

    // Basic blocks
    for (i, block) in body.blocks.iter().enumerate() {
        write_block(f, block)?;
        if i + 1 < body.blocks.len() {
            writeln!(f)?;
        }
    }

    writeln!(f, "}}")?;

    // Recursively display child lambda functions, labeled by index.
    for (idx, lambda) in func.lambdas.iter().enumerate() {
        writeln!(f)?;
        writeln!(f, "// lambda[{idx}]")?;
        write_function(f, lambda)?;
    }

    Ok(())
}

fn write_local_decl_inline(f: &mut impl Write, id: Local, decl: &LocalDecl) -> fmt::Result {
    if let Some(name) = &decl.name {
        write!(f, "{name}: {}", decl.ty)
    } else {
        write!(f, "{id}: {}", decl.ty)
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
        StatementKind::FreshCell(local) => {
            write!(f, "fresh_cell({local});")
        }
        StatementKind::Intrinsic { op, args } => {
            let op_str = match op {
                IntrinsicOp::Log(LogLevel::Info) => "log_info",
                IntrinsicOp::Log(LogLevel::Debug) => "log_debug",
                IntrinsicOp::Log(LogLevel::Warn) => "log_warn",
                IntrinsicOp::Log(LogLevel::Error) => "log_error",
                IntrinsicOp::SendEvent => "send_event",
            };
            write!(f, "intrinsic {op_str}(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
            }
            write!(f, ");")
        }
        StatementKind::Nop => {
            write!(f, "nop;")
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
            arm_names,
        } => {
            // Build name lookup for symbolic display
            let name_map: std::collections::HashMap<i64, &str> =
                arm_names.iter().map(|(v, n)| (*v, n.as_str())).collect();

            write!(f, "switch ")?;
            write_operand(f, discriminant)?;
            write!(f, " [")?;
            for (i, (val, target)) in arms.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                if let Some(name) = name_map.get(val) {
                    write!(f, "{name}: {target}")?;
                } else {
                    write!(f, "{val}: {target}")?;
                }
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
            ntypeargs,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = call ")?;
            write_operand(f, callee)?;
            if *ntypeargs > 0 {
                write!(f, "<")?;
                for (i, arg) in args.iter().take(*ntypeargs).enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write_operand(f, arg)?;
                }
                write!(f, ">")?;
            }
            write!(f, "(")?;
            for (i, arg) in args.iter().skip(*ntypeargs).enumerate() {
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
        Terminator::SysOp {
            callee,
            args,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = sys_op ")?;
            write_operand(f, callee)?;
            write!(f, "(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
            }
            write!(f, ") -> {target}")?;
            if let Some(u) = unwind {
                write!(f, " unwind {u}")?;
            }
            write!(f, ";")
        }
        Terminator::Spawn {
            closure,
            name,
            future,
            resume,
        } => {
            write!(f, "{future} = spawn ")?;
            write_operand(f, closure)?;
            write!(f, " name=")?;
            write_operand(f, name)?;
            write!(f, " -> {resume};")
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
        Terminator::Throw { value } => {
            write!(f, "throw ")?;
            write_operand(f, value)?;
            write!(f, ";")
        }
        Terminator::ThrowIfPanic { value, otherwise } => {
            write!(f, "throw_if_panic ")?;
            write_operand(f, value)?;
            write!(f, " -> {otherwise};")
        }
        Terminator::ShortCircuit {
            operand,
            is_and,
            destination,
            eval_rhs,
            join,
        } => {
            let op = if *is_and { "&&" } else { "||" };
            write!(f, "{destination} = short_circuit({op}) ")?;
            write_operand(f, operand)?;
            write!(f, " -> [eval: {eval_rhs}, join: {join}];")
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
        Rvalue::Uint8Array(bytes) => write!(f, "b\"<{} bytes>\"", bytes.len()),
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
                AggregateKind::Class {
                    name,
                    type_arg_templates,
                } => {
                    write!(f, "{name}")?;
                    if !type_arg_templates.is_empty() {
                        write!(f, "<")?;
                        for (i, t) in type_arg_templates.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{t:?}")?;
                        }
                        write!(f, ">")?;
                    }
                }
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
        Rvalue::IsType {
            operand,
            ty_template,
        } => {
            write!(f, "is_type(")?;
            write_operand(f, operand)?;
            write!(f, ", {ty_template})")
        }
        Rvalue::MakeClosure {
            lambda_idx,
            captures,
            type_arg_templates,
        } => {
            write!(f, "make_closure lambda[{lambda_idx}]")?;
            if !type_arg_templates.is_empty() {
                write!(f, "<{} type_args>", type_arg_templates.len())?;
            }
            write!(f, "(")?;
            for (i, cap) in captures.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, cap)?;
            }
            write!(f, ")")
        }
        Rvalue::MakeBoundMethod { item_ref, receiver } => {
            write!(f, "make_bound_method {item_ref}(")?;
            write_operand(f, receiver)?;
            write!(f, ")")
        }
        Rvalue::LoadType(template) => {
            write!(f, "load_type({template})")
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
        Constant::OmittedArg => write!(f, "const <omitted>"),
        Constant::Function(qn) => write!(f, "const fn {qn}"),
        Constant::EnumVariant { enum_ref, variant } => write!(f, "const {enum_ref}.{variant}"),
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
