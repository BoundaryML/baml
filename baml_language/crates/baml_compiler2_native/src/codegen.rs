//! MIR to Cranelift IR to a native object file.
//!
//! Every function of the program becomes one object-local function with the
//! platform's C calling convention: an `int` is an `i64` holding a
//! sign-extended 63-bit value, a `float` an `f64`, and a `bool` or `null` an
//! `i8`. The object exports one symbol, [`ENTRY_SYMBOL`], which calls the
//! entry function and prints its result through the runtime.
//!
//! Integer arithmetic keeps the interpreter's range: a result outside
//! `-2^62..2^62` panics rather than wraps.

use std::{collections::HashMap, fmt::Write as _};

use baml_compiler2_hir::loc::FunctionLoc;
use baml_compiler2_mir::{
    BinOp, BlockId, Constant, Local, MirFunction, MirFunctionBody, Operand, Rvalue,
    ShortCircuitKind, StatementKind, SwitchKey, Terminator, UnaryOp,
};
use cranelift_codegen::{
    Context,
    ir::{
        self, AbiParam, InstBuilder, TrapCode,
        condcodes::{FloatCC, IntCC},
        types,
    },
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch, Variable};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::{
    CompileError, CompileOptions,
    check::{Signature, body_of, direct_callee, signature},
    program::Program,
    scalar::{
        BinaryMode, Scalar, binary_typing, is_comparison, local_scalar, place_local, unary_typing,
    },
};

/// The symbol the runtime's process entry calls.
pub(crate) const ENTRY_SYMBOL: &str = "baml_native_entry";

/// The trap after a call into a runtime function that does not return.
const TRAP_AFTER_PANIC: TrapCode = TrapCode::unwrap_user(1);
/// The trap for a MIR `Unreachable` terminator.
const TRAP_UNREACHABLE: TrapCode = TrapCode::unwrap_user(2);

/// Shift counts at or above this give a result that no longer depends on the
/// count: `int` is 63 bits wide.
const INT_BITS: i64 = 63;

pub(crate) struct Output {
    pub(crate) object: Vec<u8>,
    pub(crate) clif: Option<String>,
    pub(crate) disassembly: Option<String>,
}

fn internal(error: impl std::fmt::Display) -> CompileError {
    CompileError::Codegen(error.to_string())
}

fn clif_type(kind: Scalar) -> ir::Type {
    match kind {
        Scalar::Int => types::I64,
        Scalar::Float => types::F64,
        Scalar::Bool | Scalar::Null => types::I8,
    }
}

/// A function of the runtime library, called by symbol.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Runtime {
    PrintInt,
    PrintFloat,
    PrintBool,
    PanicIntOverflow,
    PanicNegOverflow,
    PanicDivisionByZero,
    PanicNegativeShift,
    FloatRem,
}

impl Runtime {
    fn symbol(self) -> &'static str {
        match self {
            Self::PrintInt => "baml_rt_print_int",
            Self::PrintFloat => "baml_rt_print_float",
            Self::PrintBool => "baml_rt_print_bool",
            Self::PanicIntOverflow => "baml_rt_panic_int_overflow",
            Self::PanicNegOverflow => "baml_rt_panic_neg_overflow",
            Self::PanicDivisionByZero => "baml_rt_panic_division_by_zero",
            Self::PanicNegativeShift => "baml_rt_panic_negative_shift",
            Self::FloatRem => "baml_rt_float_rem",
        }
    }

    fn signature(self, call_conv: CallConv) -> ir::Signature {
        let int = AbiParam::new(types::I64);
        let float = AbiParam::new(types::F64);
        // The C ABI has the caller extend an argument narrower than a
        // register.
        let byte = AbiParam::new(types::I8).uext();
        let (params, returns) = match self {
            Self::PrintInt
            | Self::PanicNegOverflow
            | Self::PanicDivisionByZero
            | Self::PanicNegativeShift => (vec![int], vec![]),
            Self::PrintFloat => (vec![float], vec![]),
            Self::PrintBool => (vec![byte], vec![]),
            Self::PanicIntOverflow => (vec![int, byte, int], vec![]),
            Self::FloatRem => (vec![float, float], vec![float]),
        };
        let mut signature = ir::Signature::new(call_conv);
        signature.params = params;
        signature.returns = returns;
        signature
    }
}

/// A runtime panic and the values its message is built from.
#[derive(Clone, Copy)]
enum Panic {
    IntOverflow(ir::Value, char, ir::Value),
    NegOverflow(ir::Value),
    DivisionByZero(ir::Value),
    NegativeShift(ir::Value),
}

struct Backend {
    module: ObjectModule,
    runtime: HashMap<Runtime, FuncId>,
}

impl Backend {
    /// A backend for the machine the compiler runs on, using every CPU
    /// feature that machine has.
    fn new() -> Result<Self, CompileError> {
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").map_err(internal)?;
        flags.set("is_pic", "true").map_err(internal)?;
        let mut isa = cranelift_native::builder().map_err(internal)?;
        if cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
            // The host defaults sign return addresses, which the platform's
            // ABI for ordinary (not arm64e) code does not ask for.
            isa.set("sign_return_address", "false").map_err(internal)?;
        }
        let isa = isa.finish(settings::Flags::new(flags)).map_err(internal)?;
        let builder =
            ObjectBuilder::new(isa, "baml_program", default_libcall_names()).map_err(internal)?;
        Ok(Self {
            module: ObjectModule::new(builder),
            runtime: HashMap::new(),
        })
    }

    fn call_conv(&self) -> CallConv {
        self.module.isa().default_call_conv()
    }

    fn clif_signature(&self, signature: &Signature) -> ir::Signature {
        let mut clif = ir::Signature::new(self.call_conv());
        clif.params = signature
            .params
            .iter()
            .map(|kind| AbiParam::new(clif_type(*kind)))
            .collect();
        clif.returns = vec![AbiParam::new(clif_type(signature.result))];
        clif
    }

    fn runtime(&mut self, function: Runtime) -> Result<FuncId, CompileError> {
        if let Some(id) = self.runtime.get(&function) {
            return Ok(*id);
        }
        let signature = function.signature(self.call_conv());
        let id = self
            .module
            .declare_function(function.symbol(), Linkage::Import, &signature)
            .map_err(internal)?;
        self.runtime.insert(function, id);
        Ok(id)
    }
}

struct Declared {
    id: FuncId,
    signature: Signature,
}

/// A symbol name that shows the function's display name in a profile or
/// disassembly. `index` keeps it unique.
fn symbol_name(index: usize, display_name: &str) -> String {
    let readable: String = display_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("baml.{readable}.{index}")
}

pub(crate) fn generate<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    program: &Program<'db>,
    options: CompileOptions,
) -> Result<Output, CompileError> {
    let mut backend = Backend::new()?;
    let mut clif = options.emit_clif.then(String::new);
    let mut disassembly = options.emit_disassembly.then(String::new);

    // Declare everything first: calls can go forward and in cycles.
    let mut bodies = Vec::with_capacity(program.functions.len());
    let mut declared: HashMap<FunctionLoc<'db>, Declared> = HashMap::new();
    for (index, function) in program.functions.iter().enumerate() {
        let mir = function
            .mir
            .ok_or_else(|| internal(format!("`{}` has no MIR", function.name)))?;
        let body = body_of(mir).map_err(internal)?;
        let signature = signature(mir, body).map_err(internal)?;
        let id = backend
            .module
            .declare_function(
                &symbol_name(index, &function.name),
                Linkage::Local,
                &backend.clif_signature(&signature),
            )
            .map_err(internal)?;
        declared.insert(function.loc, Declared { id, signature });
        bodies.push((function, mir, body));
    }

    let mut context = backend.module.make_context();
    let mut builder_context = FunctionBuilderContext::new();
    for (function, mir, body) in bodies {
        let this = &declared[&function.loc];
        context.func.signature = backend.clif_signature(&this.signature);
        Translator::new(
            db,
            &mut backend,
            &declared,
            body,
            &mut context.func,
            &mut builder_context,
        )
        .function(mir)?;
        define(
            &mut backend,
            &mut context,
            this.id,
            &function.name,
            &mut clif,
            &mut disassembly,
        )?;
    }

    let entry = program
        .functions
        .first()
        .and_then(|function| declared.get(&function.loc))
        .ok_or_else(|| internal("a program has an entry function"))?;
    if !entry.signature.params.is_empty() {
        return Err(CompileError::EntryTakesArguments);
    }
    let entry_id = backend
        .module
        .declare_function(
            ENTRY_SYMBOL,
            Linkage::Export,
            &ir::Signature::new(backend.call_conv()),
        )
        .map_err(internal)?;
    context.func.signature = ir::Signature::new(backend.call_conv());
    entry_shim(&mut backend, entry, &mut context.func, &mut builder_context)?;
    define(
        &mut backend,
        &mut context,
        entry_id,
        ENTRY_SYMBOL,
        &mut clif,
        &mut disassembly,
    )?;

    let object = backend.module.finish().emit().map_err(internal)?;
    Ok(Output {
        object,
        clif,
        disassembly,
    })
}

/// Compile the function in `context` to machine code under `id`.
fn define(
    backend: &mut Backend,
    context: &mut Context,
    id: FuncId,
    name: &str,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    if let Some(text) = clif {
        let _ = writeln!(text, "; {name}\n{}", context.func.display());
    }
    context.set_disasm(disassembly.is_some());
    backend
        .module
        .define_function(id, context)
        .map_err(|error| internal(format!("compiling `{name}`: {error:?}")))?;
    if let Some(text) = disassembly
        && let Some(code) = context.compiled_code()
        && let Some(listing) = &code.vcode
    {
        let _ = writeln!(text, "; {name}\n{listing}");
    }
    backend.module.clear_context(context);
    Ok(())
}

/// The exported function the runtime calls: run the entry function, then
/// print what it returned.
fn entry_shim(
    backend: &mut Backend,
    entry: &Declared,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) -> Result<(), CompileError> {
    let print = match entry.signature.result {
        Scalar::Int => Some(Runtime::PrintInt),
        Scalar::Float => Some(Runtime::PrintFloat),
        Scalar::Bool => Some(Runtime::PrintBool),
        Scalar::Null => None,
    };
    let print = print.map(|print| backend.runtime(print)).transpose()?;
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let callee = backend.module.declare_func_in_func(entry.id, builder.func);
    let call = builder.ins().call(callee, &[]);
    let result = builder.inst_results(call)[0];
    if let Some(print) = print {
        let print = backend.module.declare_func_in_func(print, builder.func);
        builder.ins().call(print, &[result]);
    }
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

/// Translates one MIR function body into the Cranelift function it borrows.
struct Translator<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    backend: &'a mut Backend,
    declared: &'a HashMap<FunctionLoc<'db>, Declared>,
    body: &'a MirFunctionBody<'db>,
    builder: FunctionBuilder<'a>,
    /// The variable of each local that carries a value; `None` for a `null`
    /// local and for a local of a type outside the subset, which a checked
    /// body never reads or writes.
    variables: Vec<Option<Variable>>,
    blocks: Vec<ir::Block>,
    function_refs: HashMap<FuncId, ir::FuncRef>,
}

impl<'a, 'db> Translator<'a, 'db> {
    fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        backend: &'a mut Backend,
        declared: &'a HashMap<FunctionLoc<'db>, Declared>,
        body: &'a MirFunctionBody<'db>,
        function: &'a mut ir::Function,
        builder_context: &'a mut FunctionBuilderContext,
    ) -> Self {
        let mut builder = FunctionBuilder::new(function, builder_context);
        let variables = body
            .locals
            .iter()
            .map(|local| match crate::scalar::scalar_of(&local.ty) {
                Some(kind) if kind != Scalar::Null && !local.is_captured => {
                    Some(builder.declare_var(clif_type(kind)))
                }
                _ => None,
            })
            .collect();
        let blocks = body.blocks.iter().map(|_| builder.create_block()).collect();
        Self {
            db,
            backend,
            declared,
            body,
            builder,
            variables,
            blocks,
            function_refs: HashMap::new(),
        }
    }

    fn function(mut self, mir: &MirFunction<'db>) -> Result<(), CompileError> {
        // The MIR entry block can be a loop header, and a Cranelift entry
        // block cannot have predecessors, so parameters land in a block of
        // their own.
        let entry = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry);
        self.builder.switch_to_block(entry);
        for index in 1..=mir.arity {
            let value = self.builder.block_params(entry)[index - 1];
            self.write(Local(index), value)?;
        }
        let first = self.block(self.body.entry);
        self.builder.ins().jump(first, &[]);

        for (index, block) in self.body.blocks.iter().enumerate() {
            self.builder.switch_to_block(self.blocks[index]);
            for statement in &block.statements {
                self.statement(&statement.kind)?;
            }
            let terminator = block
                .terminator
                .as_ref()
                .ok_or_else(|| internal("a checked block has a terminator"))?;
            self.terminator(terminator)?;
        }
        self.builder.seal_all_blocks();
        self.builder.finalize();
        Ok(())
    }

    fn block(&self, id: BlockId) -> ir::Block {
        self.blocks[id.0]
    }

    fn kind(&self, local: Local) -> Result<Scalar, CompileError> {
        local_scalar(self.body, local).map_err(internal)
    }

    fn null(&mut self) -> ir::Value {
        self.builder.ins().iconst(types::I8, 0)
    }

    fn bool_const(&mut self, value: bool) -> ir::Value {
        self.builder.ins().iconst(types::I8, i64::from(value))
    }

    fn read(&mut self, local: Local) -> Result<(ir::Value, Scalar), CompileError> {
        let kind = self.kind(local)?;
        let value = match self.variables[local.0] {
            Some(variable) => self.builder.use_var(variable),
            None => self.null(),
        };
        Ok((value, kind))
    }

    fn write(&mut self, local: Local, value: ir::Value) -> Result<(), CompileError> {
        self.kind(local)?;
        if let Some(variable) = self.variables[local.0] {
            self.builder.def_var(variable, value);
        }
        Ok(())
    }

    fn operand(&mut self, operand: &Operand<'db>) -> Result<(ir::Value, Scalar), CompileError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.read(place_local(place).map_err(internal)?)
            }
            Operand::Constant(Constant::Int(value)) => {
                Ok((self.builder.ins().iconst(types::I64, *value), Scalar::Int))
            }
            Operand::Constant(Constant::Float(value)) => {
                Ok((self.builder.ins().f64const(*value), Scalar::Float))
            }
            Operand::Constant(Constant::Bool(value)) => Ok((self.bool_const(*value), Scalar::Bool)),
            Operand::Constant(Constant::Null) => Ok((self.null(), Scalar::Null)),
            Operand::Constant(_) => Err(internal("a checked operand is a scalar constant")),
        }
    }

    fn statement(&mut self, statement: &StatementKind<'db>) -> Result<(), CompileError> {
        match statement {
            StatementKind::Nop | StatementKind::Drop(_) => Ok(()),
            StatementKind::Assign { destination, value } => {
                let value = self.rvalue(value)?;
                self.write(place_local(destination).map_err(internal)?, value)
            }
            _ => Err(internal("a checked statement is an assignment")),
        }
    }

    fn rvalue(&mut self, value: &Rvalue<'db>) -> Result<ir::Value, CompileError> {
        match value {
            Rvalue::Use(operand) => Ok(self.operand(operand)?.0),
            Rvalue::BinaryOp { op, left, right } => self.binary(*op, left, right),
            Rvalue::UnaryOp { op, operand } => self.unary(*op, operand),
            _ => Err(internal("a checked rvalue is a scalar computation")),
        }
    }

    fn function_ref(&mut self, id: FuncId) -> ir::FuncRef {
        *self.function_refs.entry(id).or_insert_with(|| {
            self.backend
                .module
                .declare_func_in_func(id, self.builder.func)
        })
    }

    fn runtime_ref(&mut self, function: Runtime) -> Result<ir::FuncRef, CompileError> {
        let id = self.backend.runtime(function)?;
        Ok(self.function_ref(id))
    }

    /// Branch to a cold block that raises `panic` when `condition` is true,
    /// and continue in a fresh block otherwise.
    fn panic_if(&mut self, condition: ir::Value, panic: Panic) -> Result<(), CompileError> {
        let raise = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.set_cold_block(raise);
        self.builder.ins().brif(condition, raise, &[], resume, &[]);

        self.builder.switch_to_block(raise);
        let (function, arguments) = match panic {
            Panic::IntOverflow(left, op, right) => {
                let op = self
                    .builder
                    .ins()
                    .iconst(types::I8, i64::from(u32::from(op)));
                (Runtime::PanicIntOverflow, vec![left, op, right])
            }
            Panic::NegOverflow(value) => (Runtime::PanicNegOverflow, vec![value]),
            Panic::DivisionByZero(dividend) => (Runtime::PanicDivisionByZero, vec![dividend]),
            Panic::NegativeShift(count) => (Runtime::PanicNegativeShift, vec![count]),
        };
        let function = self.runtime_ref(function)?;
        self.builder.ins().call(function, &arguments);
        self.builder.ins().trap(TRAP_AFTER_PANIC);

        self.builder.switch_to_block(resume);
        Ok(())
    }

    /// Raise `panic` unless `value` is a 63-bit integer: its top two bits
    /// agree exactly when it is.
    fn require_int_range(&mut self, value: ir::Value, panic: Panic) -> Result<(), CompileError> {
        let doubled = self.builder.ins().ishl_imm(value, 1);
        let top_bits_differ = self.builder.ins().bxor(value, doubled);
        // A single-bit mask as the branch condition is one test-bit branch.
        let out_of_range = self.builder.ins().band_imm(top_bits_differ, i64::MIN);
        self.panic_if(out_of_range, panic)
    }

    fn binary(
        &mut self,
        op: BinOp,
        left: &Operand<'db>,
        right: &Operand<'db>,
    ) -> Result<ir::Value, CompileError> {
        let (left_value, left_kind) = self.operand(left)?;
        let (right_value, right_kind) = self.operand(right)?;
        let (mode, _) = binary_typing(op, left_kind, right_kind).map_err(internal)?;
        match mode {
            BinaryMode::Int if is_comparison(op) => {
                let condition = match op {
                    BinOp::Eq => IntCC::Equal,
                    BinOp::Ne => IntCC::NotEqual,
                    BinOp::Lt => IntCC::SignedLessThan,
                    BinOp::Le => IntCC::SignedLessThanOrEqual,
                    BinOp::Gt => IntCC::SignedGreaterThan,
                    _ => IntCC::SignedGreaterThanOrEqual,
                };
                Ok(self.builder.ins().icmp(condition, left_value, right_value))
            }
            BinaryMode::Int => {
                let right_constant = match right {
                    Operand::Constant(Constant::Int(value)) => Some(*value),
                    _ => None,
                };
                self.int_arithmetic(op, left_value, right_value, right_constant)
            }
            BinaryMode::Float => {
                let left_float = self.float_from(left_value, left_kind);
                let right_float = self.float_from(right_value, right_kind);
                if is_comparison(op) {
                    Ok(self.float_comparison(
                        op,
                        (left_float, may_be_nan(left, left_kind)),
                        (right_float, may_be_nan(right, right_kind)),
                    ))
                } else {
                    self.float_arithmetic(op, left_float, right_float)
                }
            }
            BinaryMode::Bool => {
                let condition = if op == BinOp::Eq {
                    IntCC::Equal
                } else {
                    IntCC::NotEqual
                };
                Ok(self.builder.ins().icmp(condition, left_value, right_value))
            }
            // `null` is the one value of its type.
            BinaryMode::Null => Ok(self.bool_const(op == BinOp::Eq)),
        }
    }

    fn float_from(&mut self, value: ir::Value, kind: Scalar) -> ir::Value {
        if kind == Scalar::Int {
            self.builder.ins().fcvt_from_sint(types::F64, value)
        } else {
            value
        }
    }

    fn int_arithmetic(
        &mut self,
        op: BinOp,
        left: ir::Value,
        right: ir::Value,
        right_constant: Option<i64>,
    ) -> Result<ir::Value, CompileError> {
        match op {
            // Two 63-bit operands cannot overflow 64 bits under `+` or `-`,
            // so only the 63-bit range needs checking.
            BinOp::Add => {
                let sum = self.builder.ins().iadd(left, right);
                self.require_int_range(sum, Panic::IntOverflow(left, '+', right))?;
                Ok(sum)
            }
            BinOp::Sub => {
                let difference = self.builder.ins().isub(left, right);
                self.require_int_range(difference, Panic::IntOverflow(left, '-', right))?;
                Ok(difference)
            }
            // `2 * left` is exact in 64 bits, and `(2 * left) * right`
            // overflows 64 bits exactly when `left * right` leaves 63.
            BinOp::Mul => {
                // The 128-bit product fits 64 bits when its high half is the
                // sign extension of its low half; comparing the two feeds the
                // branch directly.
                let doubled = self.builder.ins().ishl_imm(left, 1);
                let low = self.builder.ins().imul(doubled, right);
                let high = self.builder.ins().smulhi(doubled, right);
                let sign = self.builder.ins().sshr_imm(low, 63);
                let overflow = self.builder.ins().icmp(IntCC::NotEqual, high, sign);
                self.panic_if(overflow, Panic::IntOverflow(left, '*', right))?;
                Ok(self.builder.ins().sshr_imm(low, 1))
            }
            BinOp::Div => {
                self.require_nonzero_divisor(left, right, right_constant)?;
                let quotient = self.builder.ins().sdiv(left, right);
                // Only the smallest int divided by -1 leaves the range.
                if right_constant.is_none_or(|divisor| divisor == -1) {
                    self.require_int_range(quotient, Panic::IntOverflow(left, '/', right))?;
                }
                Ok(quotient)
            }
            // A remainder is smaller in magnitude than its divisor.
            BinOp::Mod => {
                self.require_nonzero_divisor(left, right, right_constant)?;
                Ok(self.builder.ins().srem(left, right))
            }
            BinOp::BitAnd => Ok(self.builder.ins().band(left, right)),
            BinOp::BitOr => Ok(self.builder.ins().bor(left, right)),
            BinOp::BitXor => Ok(self.builder.ins().bxor(left, right)),
            BinOp::Shl => {
                let past_width = self.require_shift_count(right)?;
                // Keep the low 63 bits of the shifted value and sign-extend
                // them.
                let shifted = self.builder.ins().ishl(left, right);
                let truncated = self.builder.ins().ishl_imm(shifted, 1);
                let in_range = self.builder.ins().sshr_imm(truncated, 1);
                let zero = self.builder.ins().iconst(types::I64, 0);
                Ok(self.builder.ins().select(past_width, zero, in_range))
            }
            BinOp::Shr => {
                let past_width = self.require_shift_count(right)?;
                // Shifting by the full width leaves only the sign.
                let width = self.builder.ins().iconst(types::I64, INT_BITS);
                let count = self.builder.ins().select(past_width, width, right);
                Ok(self.builder.ins().sshr(left, count))
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                Err(internal("comparisons are not arithmetic"))
            }
        }
    }

    fn require_nonzero_divisor(
        &mut self,
        dividend: ir::Value,
        divisor: ir::Value,
        divisor_constant: Option<i64>,
    ) -> Result<(), CompileError> {
        if divisor_constant.is_some_and(|divisor| divisor != 0) {
            return Ok(());
        }
        let is_zero = self.builder.ins().icmp_imm(IntCC::Equal, divisor, 0);
        self.panic_if(is_zero, Panic::DivisionByZero(dividend))
    }

    /// Panic on a negative shift count; the result is whether the count is
    /// at least the width of `int`.
    fn require_shift_count(&mut self, count: ir::Value) -> Result<ir::Value, CompileError> {
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, count, 0);
        self.panic_if(negative, Panic::NegativeShift(count))?;
        Ok(self
            .builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThanOrEqual, count, INT_BITS))
    }

    fn float_arithmetic(
        &mut self,
        op: BinOp,
        left: ir::Value,
        right: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        Ok(match op {
            BinOp::Add => self.builder.ins().fadd(left, right),
            BinOp::Sub => self.builder.ins().fsub(left, right),
            BinOp::Mul => self.builder.ins().fmul(left, right),
            BinOp::Div => self.builder.ins().fdiv(left, right),
            BinOp::Mod => {
                let remainder = self.runtime_ref(Runtime::FloatRem)?;
                let call = self.builder.ins().call(remainder, &[left, right]);
                self.builder.inst_results(call)[0]
            }
            _ => return Err(internal("a checked float operator is arithmetic")),
        })
    }

    /// Compare in BAML's total order on floats: IEEE order, except that NaN
    /// equals NaN and is greater than every number. Each operand comes with
    /// whether it can be NaN at all; the NaN tests of one that cannot are
    /// left out.
    fn float_comparison(
        &mut self,
        op: BinOp,
        (left, left_nan): (ir::Value, bool),
        (right, right_nan): (ir::Value, bool),
    ) -> ir::Value {
        let ins =
            |translator: &mut Self, condition, a, b| translator.builder.ins().fcmp(condition, a, b);
        match op {
            BinOp::Eq | BinOp::Ne => {
                let mut equal = ins(self, FloatCC::Equal, left, right);
                if left_nan && right_nan {
                    let left_is_nan = ins(self, FloatCC::Unordered, left, left);
                    let right_is_nan = ins(self, FloatCC::Unordered, right, right);
                    let both = self.builder.ins().band(left_is_nan, right_is_nan);
                    equal = self.builder.ins().bor(equal, both);
                }
                if op == BinOp::Eq {
                    equal
                } else {
                    self.builder.ins().bxor_imm(equal, 1)
                }
            }
            // Less also when only the right side is NaN.
            BinOp::Lt => {
                let less = ins(self, FloatCC::LessThan, left, right);
                if !right_nan {
                    return less;
                }
                let mut right_only = ins(self, FloatCC::Unordered, right, right);
                if left_nan {
                    let left_is_number = ins(self, FloatCC::Ordered, left, left);
                    right_only = self.builder.ins().band(right_only, left_is_number);
                }
                self.builder.ins().bor(less, right_only)
            }
            // Less or equal also whenever the right side is NaN.
            BinOp::Le => {
                let less_or_equal = ins(self, FloatCC::LessThanOrEqual, left, right);
                if !right_nan {
                    return less_or_equal;
                }
                let right_is_nan = ins(self, FloatCC::Unordered, right, right);
                self.builder.ins().bor(less_or_equal, right_is_nan)
            }
            // Greater also when only the left side is NaN.
            BinOp::Gt => {
                let greater = ins(self, FloatCC::GreaterThan, left, right);
                if !left_nan {
                    return greater;
                }
                let mut left_only = ins(self, FloatCC::Unordered, left, left);
                if right_nan {
                    let right_is_number = ins(self, FloatCC::Ordered, right, right);
                    left_only = self.builder.ins().band(left_only, right_is_number);
                }
                self.builder.ins().bor(greater, left_only)
            }
            // Greater or equal also whenever the left side is NaN.
            _ => {
                let greater_or_equal = ins(self, FloatCC::GreaterThanOrEqual, left, right);
                if !left_nan {
                    return greater_or_equal;
                }
                let left_is_nan = ins(self, FloatCC::Unordered, left, left);
                self.builder.ins().bor(greater_or_equal, left_is_nan)
            }
        }
    }

    fn unary(&mut self, op: UnaryOp, operand: &Operand<'db>) -> Result<ir::Value, CompileError> {
        let (value, kind) = self.operand(operand)?;
        unary_typing(op, kind).map_err(internal)?;
        Ok(match (op, kind) {
            (UnaryOp::Neg, Scalar::Float) => self.builder.ins().fneg(value),
            // Negating the smallest int leaves the range.
            (UnaryOp::Neg, _) => {
                let negated = self.builder.ins().ineg(value);
                self.require_int_range(negated, Panic::NegOverflow(value))?;
                negated
            }
            // False, null, zero and 0.0 are the falsy scalars; NaN is truthy.
            (UnaryOp::Truthy, Scalar::Bool) => value,
            (UnaryOp::Truthy, Scalar::Int) => {
                self.builder.ins().icmp_imm(IntCC::NotEqual, value, 0)
            }
            (UnaryOp::Truthy, Scalar::Float) => {
                let zero = self.builder.ins().f64const(0.0);
                self.builder.ins().fcmp(FloatCC::NotEqual, value, zero)
            }
            (UnaryOp::Truthy, Scalar::Null) => self.bool_const(false),
            (UnaryOp::Not, Scalar::Bool | Scalar::Int) => {
                self.builder.ins().icmp_imm(IntCC::Equal, value, 0)
            }
            (UnaryOp::Not, Scalar::Float) => {
                let zero = self.builder.ins().f64const(0.0);
                self.builder.ins().fcmp(FloatCC::Equal, value, zero)
            }
            (UnaryOp::Not, Scalar::Null) => self.bool_const(true),
        })
    }

    fn terminator(&mut self, terminator: &Terminator<'db>) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto { target } => {
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let (condition, _) = self.operand(condition)?;
                let (then_block, else_block) = (self.block(*then_block), self.block(*else_block));
                self.builder
                    .ins()
                    .brif(condition, then_block, &[], else_block, &[]);
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                let (value, _) = self.operand(operand)?;
                // The left operand is the whole result on the edge that
                // skips the right one.
                self.write(place_local(destination).map_err(internal)?, value)?;
                let (join, eval_rhs) = (self.block(*join), self.block(*eval_rhs));
                match kind {
                    ShortCircuitKind::And => {
                        self.builder.ins().brif(value, eval_rhs, &[], join, &[]);
                    }
                    ShortCircuitKind::Or => {
                        self.builder.ins().brif(value, join, &[], eval_rhs, &[]);
                    }
                    ShortCircuitKind::Coalesce => {
                        return Err(internal("a checked short circuit is boolean"));
                    }
                }
            }
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } => {
                let (value, _) = self.operand(discriminant)?;
                let mut switch = Switch::new();
                let mut seen = Vec::with_capacity(arms.len());
                for (key, target) in arms {
                    let SwitchKey::Int(key) = key else {
                        return Err(internal("a checked switch has int keys"));
                    };
                    // The first arm with a key wins, as in a chain of tests.
                    if !seen.contains(key) {
                        seen.push(*key);
                        switch.set_entry(u128::from(key.cast_unsigned()), self.block(*target));
                    }
                }
                let otherwise = self.block(*otherwise);
                switch.emit(&mut self.builder, value, otherwise);
            }
            Terminator::Return => {
                let (value, _) = self.read(Local(0))?;
                self.builder.ins().return_(&[value]);
            }
            Terminator::Unreachable => {
                self.builder.ins().trap(TRAP_UNREACHABLE);
            }
            Terminator::Call {
                callee,
                args,
                destination,
                target,
                ..
            } => {
                let function = direct_callee(self.db, callee).map_err(internal)?;
                let id = self
                    .declared
                    .get(&function)
                    .ok_or_else(|| internal("a called function is part of the program"))?
                    .id;
                let callee = self.function_ref(id);
                let arguments = args
                    .iter()
                    .map(|argument| self.operand(argument).map(|(value, _)| value))
                    .collect::<Result<Vec<_>, _>>()?;
                let call = self.builder.ins().call(callee, &arguments);
                let result = self.builder.inst_results(call)[0];
                self.write(place_local(destination).map_err(internal)?, result)?;
                let target = self.block(*target);
                self.builder.ins().jump(target, &[]);
            }
            _ => return Err(internal("a checked terminator is scalar control flow")),
        }
        Ok(())
    }
}

/// Whether an operand of a float operation can be NaN: a float that is not a
/// constant number. An int converted to float never is.
fn may_be_nan(operand: &Operand<'_>, kind: Scalar) -> bool {
    match operand {
        Operand::Constant(Constant::Float(value)) => value.is_nan(),
        _ => kind == Scalar::Float,
    }
}
