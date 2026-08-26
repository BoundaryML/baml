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
                BuiltinKind::AwaitAny => "await_any",
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
        StatementKind::VirtualFieldStore {
            iface,
            receiver,
            field_index,
            field,
            value,
        } => {
            write_operand(f, receiver)?;
            write!(f, ".{field}#{field_index} as {iface} = ")?;
            write_operand(f, value)?;
            write!(f, ";")
        }
        StatementKind::Drop(place) => {
            write!(f, "drop({place});")
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
                IntrinsicOp::BindType(slot) => return write!(f, "bind_type({slot}, {args:?});"),
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
        Terminator::NarrowBind {
            source,
            ty_template,
            destination,
            then_block,
            else_block,
        } => {
            write!(f, "{destination} = narrow_bind ")?;
            write_operand(f, source)?;
            write!(f, " as {ty_template:?} -> [{then_block}, {else_block}];")
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
            runtime_id,
            runtime_type_check,
            destination,
            target,
            unwind,
            ..
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
            let mut wrote_arg = false;
            for arg in args.iter().skip(*ntypeargs) {
                if wrote_arg {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
                wrote_arg = true;
            }
            write_runtime_id_arg(f, wrote_arg, runtime_id.as_ref())?;
            if *runtime_type_check {
                write!(f, "; runtime_type_check")?;
            }
            write!(f, ") -> [{target}")?;
            if let Some(u) = unwind {
                write!(f, ", unwind: {u}")?;
            }
            write!(f, "];")
        }
        Terminator::VirtualCall {
            iface,
            method,
            args,
            ntypeargs,
            runtime_id,
            runtime_type_check,
            destination,
            target,
            unwind,
            ..
        } => {
            write!(f, "{destination} = virtual_call {method} as {iface}")?;
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
            let mut wrote_arg = false;
            for arg in args.iter().skip(*ntypeargs) {
                if wrote_arg {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
                wrote_arg = true;
            }
            write_runtime_id_arg(f, wrote_arg, runtime_id.as_ref())?;
            if *runtime_type_check {
                write!(f, "; runtime_type_check")?;
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
            runtime_id,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = sys_op ")?;
            write_operand(f, callee)?;
            write!(f, "(")?;
            let mut wrote_arg = false;
            for arg in args {
                if wrote_arg {
                    write!(f, ", ")?;
                }
                write_operand(f, arg)?;
                wrote_arg = true;
            }
            write_runtime_id_arg(f, wrote_arg, runtime_id.as_ref())?;
            write!(f, ") -> {target}")?;
            if let Some(u) = unwind {
                write!(f, " unwind {u}")?;
            }
            write!(f, ";")
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future_ty,
            future,
            resume,
        } => {
            write!(
                f,
                "{future} = spawn<{}, {}> ",
                future_ty.returns, future_ty.throws
            )?;
            write_operand(f, closure)?;
            write!(f, " name=")?;
            write_operand(f, name)?;
            if let Some(config) = config {
                write!(f, " config=")?;
                write_operand(f, config)?;
            }
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
        Terminator::AwaitAny {
            futures,
            destination,
            target,
            unwind,
        } => {
            write!(f, "{destination} = await_any ")?;
            write_operand(f, futures)?;
            write!(f, " -> [{target}")?;
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
        Terminator::Rethrow { value } => {
            write!(f, "rethrow ")?;
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

fn write_runtime_id_arg(
    f: &mut impl Write,
    wrote_arg: bool,
    runtime_id: Option<&Operand>,
) -> fmt::Result {
    if let Some(runtime_id) = runtime_id {
        if wrote_arg {
            write!(f, ", ")?;
        }
        write!(f, "$id = ")?;
        write_operand(f, runtime_id)?;
    }
    Ok(())
}

fn write_rvalue(f: &mut impl Write, rvalue: &Rvalue) -> fmt::Result {
    match rvalue {
        Rvalue::Use(operand) => write_operand(f, operand),
        Rvalue::VirtualFieldAccess {
            iface,
            receiver,
            field_index,
            field,
        } => {
            write_operand(f, receiver)?;
            write!(f, ".{field}#{field_index} as {iface}")
        }
        Rvalue::BinaryOp { op, left, right } => {
            write_operand(f, left)?;
            write!(f, " {op} ")?;
            write_operand(f, right)
        }
        Rvalue::UnaryOp { op, operand } => {
            write!(f, "{op}")?;
            write_operand(f, operand)
        }
        Rvalue::Array(element_template, elements) => {
            write!(f, "[")?;
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, elem)?;
            }
            // Show the emitted element-type template so MIR snapshots can catch a
            // wrong array element type (not just the later bytecode `load_type`).
            write!(f, "]: {element_template}[]")
        }
        Rvalue::Uint8Array(bytes) => write!(f, "b\"<{} bytes>\"", bytes.len()),
        Rvalue::Map(key_template, value_template, entries) => {
            write!(f, "{{ ")?;
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_operand(f, key)?;
                write!(f, ": ")?;
                write_operand(f, value)?;
            }
            write!(f, " }}: map<{key_template}, {value_template}>")
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
                            write!(f, "{t}")?;
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
        Rvalue::IsTypeTag { operand, tag } => {
            write!(f, "is_type_tag(")?;
            write_operand(f, operand)?;
            write!(f, ", {})", type_tag_name(*tag))
        }
        Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => {
            write!(f, "runtime_is_type(")?;
            write_operand(f, operand)?;
            write!(f, ", ")?;
            write_operand(f, type_value)?;
            write!(f, ")")
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
        Rvalue::MakeVirtualBoundMethod {
            iface,
            method,
            receiver,
            type_args,
        } => {
            write!(f, "make_virtual_bound_method {iface:?}.{method}")?;
            if !type_args.is_empty() {
                write!(f, "<{type_args:?}>")?;
            }
            write!(f, "(")?;
            write_operand(f, receiver)?;
            write!(f, ")")
        }
        Rvalue::MakeVirtualFunction {
            self_ty,
            iface,
            method,
            type_args,
        } => {
            write!(f, "make_virtual_function ({self_ty} as {iface:?}).{method}")?;
            if !type_args.is_empty() {
                write!(f, "<")?;
                for (index, arg) in type_args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write_operand(f, arg)?;
                }
                write!(f, ">")?;
            }
            Ok(())
        }
        Rvalue::LoadType(template) => {
            write!(f, "load_type({template})")
        }
        Rvalue::CurrentPackage(package) => {
            write!(f, "current_package({package})")
        }
        Rvalue::MakeGenericFunction {
            item,
            type_arg_templates,
        } => {
            let args: Vec<String> = type_arg_templates.iter().map(ToString::to_string).collect();
            write!(f, "make_generic_function {item}<{}>", args.join(", "))
        }
        Rvalue::MakeGenericFunctionFromValue {
            value,
            type_arg_templates,
        } => {
            write!(f, "make_generic_function_from_value(")?;
            write_operand(f, value)?;
            let args: Vec<String> = type_arg_templates.iter().map(ToString::to_string).collect();
            write!(f, ")<{}>", args.join(", "))
        }
    }
}

/// Symbolic name for a `baml_type::typetag` constant in `is_type_tag` renders.
/// Class tags (`CLASS_BASE + n`) and anything unrecognized print numerically.
/// The full tag set is named for robustness, though the only MIR producer
/// today (`emit_is_type_tag_branch`) emits `LIST`/`MAP`.
fn type_tag_name(tag: i64) -> std::borrow::Cow<'static, str> {
    use baml_type::typetag as t;
    std::borrow::Cow::Borrowed(match tag {
        t::INT => "INT",
        t::STRING => "STRING",
        t::BOOL => "BOOL",
        t::NULL => "NULL",
        t::FLOAT => "FLOAT",
        t::ENUM => "ENUM",
        t::LIST => "LIST",
        t::MAP => "MAP",
        t::FUNCTION => "FUNCTION",
        t::FUTURE => "FUTURE",
        t::TYPE => "TYPE",
        t::COLLECTOR => "COLLECTOR",
        t::UINT8ARRAY => "UINT8ARRAY",
        t::BIGINT => "BIGINT",
        other => return std::borrow::Cow::Owned(other.to_string()),
    })
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
        Constant::Bigint(n) => write!(f, "const {n}n"),
        Constant::Float(n) => write!(f, "const {n}_f64"),
        Constant::String(s) => write!(f, "const {s:?}"),
        Constant::Bool(b) => write!(f, "const {b}"),
        Constant::Null => write!(f, "const null"),
        Constant::OmittedArg => write!(f, "const <omitted>"),
        Constant::Function(qn) => write!(f, "const fn {qn}"),
        Constant::GlobalItem(qn) => write!(f, "const item {qn}"),
        Constant::GenericFunction { item, type_args } => {
            let args: Vec<String> = type_args.iter().map(ToString::to_string).collect();
            write!(f, "const fn {item}<{}>", args.join(", "))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockId, Place};

    fn render_terminator(terminator: &Terminator) -> String {
        let mut output = String::new();
        write_terminator(&mut output, terminator).expect("terminator renders");
        output
    }

    fn local_copy(local: usize) -> Operand {
        Operand::copy_local(Local(local))
    }

    #[test]
    fn call_runtime_id_without_visible_args_has_no_leading_comma() {
        let terminator = Terminator::Call {
            callee: local_copy(1),
            args: Vec::new(),
            ntypeargs: 0,
            runtime_type_check: false,
            runtime_id: Some(local_copy(9)),
            destination: Place::local(Local(0)),
            target: BlockId(1),
            unwind: None,
        };

        assert_eq!(
            render_terminator(&terminator),
            "_0 = call copy _1($id = copy _9) -> [bb1];"
        );
    }

    #[test]
    fn virtual_call_runtime_id_without_visible_args_has_no_leading_comma() {
        let terminator = Terminator::VirtualCall {
            iface: baml_type::TyTemplateInterface::new(
                baml_type::TypeName::from_dotted_path("baml.ops.Equals"),
                Box::new([]),
                Box::new([]),
            ),
            method: "eq".to_string(),
            args: Vec::new(),
            ntypeargs: 0,
            runtime_type_check: false,
            runtime_id: Some(local_copy(9)),
            destination: Place::local(Local(0)),
            target: BlockId(1),
            unwind: None,
        };

        assert_eq!(
            render_terminator(&terminator),
            "_0 = virtual_call eq as baml.ops.Equals($id = copy _9) -> [bb1];"
        );
    }

    #[test]
    fn sys_op_runtime_id_without_visible_args_has_no_leading_comma() {
        let terminator = Terminator::SysOp {
            callee: local_copy(1),
            args: Vec::new(),
            runtime_id: Some(local_copy(9)),
            destination: Place::local(Local(0)),
            target: BlockId(1),
            unwind: None,
        };

        assert_eq!(
            render_terminator(&terminator),
            "_0 = sys_op copy _1($id = copy _9) -> bb1;"
        );
    }
}
