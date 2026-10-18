//! Rvalues: operators, type tests, and aggregates.

use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, Rvalue, TyTemplate, TypeTest, UnaryOp, class_display_name,
    enum_display_name,
};
use baml_type::typetag;
use bex_vm_types::ConstValue;

use baml_compiler2_mir::function_display_name;

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    Rejection,
    analysis::{CallShape, Repr, builtin_is_callable, reads_frame, static_realized},
    render::{binop_name, cmp_name},
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn rvalue(&mut self, block: BlockId, rvalue: &Rvalue<'db>) -> Gen<Expr> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(block, operand),
            Rvalue::BinaryOp { op, left, right } => {
                let left = self.operand(block, left)?;
                let right = self.operand(block, right)?;
                self.binary(block, *op, &left, &right)
            }
            Rvalue::UnaryOp { op, operand } => {
                let operand = self.operand(block, operand)?;
                self.unary(block, *op, &operand)
            }
            Rvalue::Len(place) => {
                let container = self.load(block, place)?;
                let container = self.value_arg(&container.code, container.repr);
                let code = self.fallible(block, &format!("vm.container_len({container})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Rvalue::Discriminant(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value.code, value.repr);
                let code = self.fallible(block, &format!("vm.discriminant({value})"))?;
                Ok(Expr::new(code, Repr::Int))
            }
            Rvalue::TypeTag(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value.code, value.repr);
                Ok(Expr::new(
                    format!("bex_vm::compiled_rt::type_tag({value})"),
                    Repr::Int,
                ))
            }
            Rvalue::IsTypeTag { operand, tag } => {
                let value = self.operand(block, operand)?;
                let value = self.value_arg(&value.code, value.repr);
                Ok(Expr::new(
                    format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)"),
                    Repr::Bool,
                ))
            }
            Rvalue::IsType { operand, test } => {
                let value = self.operand(block, operand)?;
                let value = self.value_arg(&value.code, value.repr);
                let code = self.type_test(&format!("({value})"), test)?;
                Ok(Expr::new(code, Repr::Bool))
            }
            Rvalue::Array(template, elements) => {
                let element_ty = self.realized_type(template)?;
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    let element = self.operand(block, element)?;
                    values.push(self.value_arg(&element.code, element.repr));
                }
                Ok(Expr::new(
                    format!("vm.new_array({element_ty}, vec![{}])", values.join(", ")),
                    Repr::Value,
                ))
            }
            Rvalue::Aggregate {
                kind:
                    AggregateKind::Class {
                        class,
                        type_arg_templates,
                    },
                fields,
            } => {
                let name = class_display_name(self.db, *class);
                let Some((object, field_count)) = self.index.class(self.db, *class)? else {
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                if fields.len() != field_count {
                    return unsupported(format!(
                        "`{name}` literal with {} of {field_count} fields",
                        fields.len()
                    ));
                }
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    let field = self.operand(block, field)?;
                    values.push(self.value_arg(&field.code, field.repr));
                }
                let class = self.hoist(Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ));
                if type_arg_templates.is_empty() {
                    return Ok(Expr::new(
                        format!(
                            "vm.new_instance({}.as_object_ptr().expect(\"class object\"), vec![{}])",
                            class.code,
                            values.join(", ")
                        ),
                        Repr::Value,
                    ));
                }
                // An instance of a generic class carries its class type
                // arguments, as `AllocInstance` with type operands stores them.
                let type_args = self.realized_all(type_arg_templates)?;
                Ok(Expr::new(
                    format!(
                        "vm.new_generic_instance({}.as_object_ptr().expect(\"class object\"), Box::new([{}]), vec![{}])",
                        class.code,
                        type_args.join(", "),
                        values.join(", ")
                    ),
                    Repr::Value,
                ))
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ..
            } => unsupported("untyped array aggregate"),
            // A map literal is an empty map followed by `baml.Map.set` calls.
            Rvalue::Map(key, value, entries) => {
                if !entries.is_empty() {
                    return unsupported("map literal with inline entries");
                }
                let key = self.realized_type(key)?;
                let value = self.realized_type(value)?;
                Ok(Expr::new(
                    format!("vm.new_map({key}, {value})"),
                    Repr::Value,
                ))
            }
            // The literal's pooled template, deep-copied on every evaluation
            // so the value is mutable, as bytecode does.
            Rvalue::Uint8Array(bytes) => {
                let Some(&object) = self.index.byte_arrays.get(bytes) else {
                    return Err(Rejection::Invalid(
                        "byte array literal not in the program's pool".into(),
                    ));
                };
                let deep_copy = baml_compiler2_hir_ty::callable::lang_function(
                    self.db,
                    baml_base::LangPackage::Baml,
                    &[],
                    "deep_copy",
                );
                let Some((_, info)) = deep_copy
                    .map(|function| self.index.function(self.db, function))
                    .transpose()?
                    .flatten()
                else {
                    return unsupported("`baml.deep_copy` not found in the program");
                };
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported("builtin `baml.deep_copy` has no native key");
                };
                let shape = CallShape {
                    type_args_primitive: false,
                    map_key_primitive: false,
                };
                if !builtin_is_callable(key, shape) {
                    return unsupported(format!("builtin `{key}` may yield"));
                }
                let ordinal = self.native_ordinal(key);
                let template = self.hoist(Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ));
                let code = self.fallible(
                    block,
                    &format!("vm.call_builtin(native_{ordinal}(), &[{}])", template.code),
                )?;
                Ok(Expr::new(code, Repr::Value))
            }
            // A closure over a lambda, which is a unit of its own (compiled
            // or not: the object is the same): its cells, the enclosing
            // function's type arguments it captures, and the lambda's object.
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                type_arg_templates,
            } => {
                let Some(lambda) = self.lambdas.get(*lambda_idx) else {
                    return Err(Rejection::Invalid(format!(
                        "closure over lambda {lambda_idx} of a function with {}",
                        self.lambdas.len()
                    )));
                };
                let Some(object) = self.index.lambda(self.db, &lambda.identity)? else {
                    let name = lambda.identity.display_name(self.db);
                    return unsupported(format!("lambda `{name}` not found in the program"));
                };
                // The enclosing function's type arguments the closure
                // captures, read from the frame where they name its own.
                let types = self.realized_all(type_arg_templates)?;
                let mut cells = Vec::with_capacity(captures.len());
                for capture in captures {
                    let cell = self.operand(block, capture)?;
                    cells.push(self.value_arg(&cell.code, cell.repr));
                }
                Ok(Expr::new(
                    format!(
                        "vm.make_closure(vm.idx_to_ptr(ObjectIndex::from_raw({object})), \
                         vec![{}], vec![{}].into_boxed_slice())",
                        cells.join(", "),
                        types.join(", ")
                    ),
                    Repr::Value,
                ))
            }
            // `receiver.method` as a value, as bytecode's `MakeBoundMethod`
            // builds it.
            Rvalue::MakeBoundMethod { func, receiver } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func)? else {
                    return unsupported(format!("method `{name}` not found in the program"));
                };
                let receiver = self.operand(block, receiver)?;
                let receiver = self.value_arg(&receiver.code, receiver.repr);
                Ok(Expr::new(
                    format!(
                        "vm.make_bound_method(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {receiver})"
                    ),
                    Repr::Value,
                ))
            }
            // `foo<T>`: a `GenericFunction` over the function's global slot
            // at the frame-derived arguments, as `MakeGenericFunction`
            // builds it (`foo<int>` is a pooled constant instead).
            Rvalue::MakeGenericFunction {
                func,
                type_arg_templates,
            } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func)? else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let Some(&global) = self.index.globals.get(&object) else {
                    return unsupported(format!("function `{name}` has no global slot"));
                };
                let type_args = self.realized_all(type_arg_templates)?;
                Ok(Expr::new(
                    format!(
                        "vm.make_generic_function(depth, GlobalIndex::from_raw({global}), Box::new([{}]))",
                        type_args.join(", ")
                    ),
                    Repr::Value,
                ))
            }
            // `g<T>` on a function value: a closure carrying the arguments,
            // as `MakeGenericFunctionFromValue` builds it.
            Rvalue::MakeGenericFunctionFromValue {
                value,
                type_arg_templates,
            } => {
                let value = self.operand(block, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let type_args = self.realized_all(type_arg_templates)?;
                let code = self.fallible(
                    block,
                    &format!(
                        "vm.specialize_function_value({value}, &[{}])",
                        type_args.join(", ")
                    ),
                )?;
                Ok(Expr::new(code, Repr::Value))
            }
            // Interface dispatch resolved at run time against a statically
            // known interface, as the interpreter's `MakeVirtual*` and
            // `VirtualLoadField` do; see `bex_vm::compiled_rt::virtual`.
            Rvalue::MakeVirtualBoundMethod {
                iface,
                method,
                receiver,
                type_args,
            } => {
                let (iface, iface_args, _) = self.interface_operands(iface)?;
                let types = self.realized_all(type_args)?;
                let receiver = self.operand(block, receiver)?;
                let receiver = self.value_arg(&receiver.code, receiver.repr);
                let code = self.fallible(
                    block,
                    &format!(
                        "vm.make_virtual_bound_method({receiver}, {iface}, {iface_args}, {method:?}, &[{}])",
                        types.join(", ")
                    ),
                )?;
                Ok(Expr::new(code, Repr::Value))
            }
            Rvalue::MakeVirtualFunction {
                self_ty,
                iface,
                method,
                type_args,
            } => {
                let self_ty = self.realized(self_ty)?;
                let (iface, iface_args, _) = self.interface_operands(iface)?;
                let (types, _) = self.type_operands("interface function value", type_args)?;
                let code = self.fallible(
                    block,
                    &format!(
                        "vm.make_virtual_function(&{self_ty}, {iface}, {iface_args}, {method:?}, &[{}])",
                        types.join(", ")
                    ),
                )?;
                Ok(Expr::new(code, Repr::Value))
            }
            Rvalue::VirtualFieldAccess {
                iface,
                receiver,
                field_index,
                ..
            } => {
                let (iface, iface_args, _) = self.interface_operands(iface)?;
                let receiver = self.operand(block, receiver)?;
                let receiver = self.value_arg(&receiver.code, receiver.repr);
                let code = self.fallible(
                    block,
                    &format!(
                        "vm.virtual_load_field({receiver}, {iface}, {iface_args}, {field_index})"
                    ),
                )?;
                Ok(Expr::new(code, Repr::Value))
            }
            // A type local is consumed where it is used (`Generator::load`);
            // one reaching here is a local MIR also writes a value to, so the
            // type is stored as a `type` value like any other.
            Rvalue::LoadType(template) => {
                let slot = match template {
                    TyTemplate::TypeArgRef(slot) => format!("Some({slot})"),
                    _ => "None".to_string(),
                };
                let ty = self.realized(template)?;
                Ok(Expr::new(
                    format!("vm.type_value(depth, {slot}, {ty})"),
                    Repr::Value,
                ))
            }
            Rvalue::CurrentPackage => {
                let Some(ordinal) = self.package else {
                    return unsupported("current package of a function the program does not place");
                };
                Ok(Expr::new(
                    format!("vm.current_package({ordinal})"),
                    Repr::Value,
                ))
            }
            Rvalue::TraceHookSettings { .. } => unsupported("trace hook"),
        }
    }

    pub(super) fn type_test(&mut self, value: &str, test: &TypeTest<'db>) -> Gen<String> {
        match test {
            TypeTest::Class { class, args } => {
                let Some((object, _)) = self.index.class(self.db, *class)? else {
                    let name = class_display_name(self.db, *class);
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                if args.is_empty() {
                    return Ok(format!(
                        "vm.is_instance_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
                    ));
                }
                // `is C<..>`: the class, then each argument invariantly, as
                // the interpreter's test against a class with type arguments.
                let type_args = self.realized_all(args)?;
                Ok(format!(
                    "vm.is_instance_of_at({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})), &[{}])",
                    type_args.join(", ")
                ))
            }
            TypeTest::Enum(enum_ref) => {
                let Some(object) = self.index.enum_(self.db, *enum_ref)? else {
                    let name = enum_display_name(self.db, *enum_ref);
                    return unsupported(format!("enum `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_variant_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
                ))
            }
            // `is T` for a structural type. One that reads the frame (or a
            // generic class instance) is membership in the realized type, by
            // the canonical algebra; any other is decided through the constant
            // the emitter pooled for it, as bytecode's `IsType` decides it.
            TypeTest::Template(template)
                if reads_frame(template)
                    || matches!(template, TyTemplate::Class(..) | TyTemplate::Enum(..)) =>
            {
                let ty = self.realized_ref(template)?;
                Ok(format!("vm.value_is_member_of({value}, {ty})"))
            }
            TypeTest::Template(template) => self.template_test(value, template),
        }
    }

    /// `value is T` for a type template, deciding it as bytecode's emitter
    /// does for the same template (`is_type_template`): a realized leaf by
    /// its type tag, `unknown` as true, and a literal type or a structural
    /// type (a container, union, interface, alias, …) through the constant
    /// the emitter pooled for this test in the function's own constant pool,
    /// so the VM's value matcher decides both the same way.
    fn template_test(&mut self, value: &str, template: &TyTemplate) -> Gen<String> {
        frame_free(template)?;
        let tag = |tag: i64| format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)");
        let constant = match template {
            TyTemplate::Class(..) | TyTemplate::Enum(..) => {
                return Err(Rejection::Invalid(
                    "a class or enum test reaches the backend as a declaration test".into(),
                ));
            }
            // The top type: every value inhabits it.
            TyTemplate::Unknown => return Ok("true".to_string()),
            TyTemplate::Int => return Ok(tag(typetag::INT)),
            TyTemplate::Bigint => return Ok(tag(typetag::BIGINT)),
            TyTemplate::Float => return Ok(tag(typetag::FLOAT)),
            TyTemplate::String => return Ok(tag(typetag::STRING)),
            TyTemplate::Bool => return Ok(tag(typetag::BOOL)),
            TyTemplate::Null => return Ok(tag(typetag::NULL)),
            TyTemplate::Uint8Array => return Ok(tag(typetag::UINT8ARRAY)),
            TyTemplate::Type => return Ok(tag(typetag::TYPE)),
            TyTemplate::Literal(literal, _) => ConstValue::Literal(literal.clone()),
            _ => {
                let Some(anchored) = self.index.anchor(template)? else {
                    return unsupported(
                        "structural type test naming a declaration outside the program",
                    );
                };
                ConstValue::Type(anchored)
            }
        };
        let Some(index) = self.index.pooled_constant(self.object, &constant) else {
            return Err(Rejection::Invalid(
                "type test constant not in the function's pool".into(),
            ));
        };
        Ok(format!(
            "vm.is_type_constant(vm.idx_to_ptr(ObjectIndex::from_raw({})), {index}, {value})?",
            self.object
        ))
    }

    /// The `RealizedTy` expression for a type template in this function:
    /// built in place when it needs no declaration lookup
    /// ([`static_realized`]), otherwise read from the constant the emitter
    /// pooled for the same template (`LoadType`), bound to a temporary so it
    /// can be a call's argument.
    fn realized_type(&mut self, template: &TyTemplate) -> Gen<String> {
        if reads_frame(template) {
            return self.realized(template);
        }
        if let Some(code) = static_realized(self.index, template) {
            return Ok(code);
        }
        frame_free(template)?;
        let Some(anchored) = self.index.anchor(template)? else {
            return unsupported("type naming a declaration outside the program");
        };
        let Some(index) = self
            .index
            .pooled_constant(self.object, &ConstValue::Type(anchored))
        else {
            return Err(Rejection::Invalid(
                "type constant not in the function's pool".into(),
            ));
        };
        let name = format!("ty{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: RealizedTy = vm.type_constant(vm.idx_to_ptr(ObjectIndex::from_raw({})), {index})?;",
            self.object
        ));
        Ok(name)
    }

    /// The module's ordinal for the builtin `key`, binding it on first use.
    fn native_ordinal(&mut self, key: &str) -> usize {
        let next = self.natives.len();
        *self.natives.entry(key.to_owned()).or_insert(next)
    }

    fn binary(&mut self, block: BlockId, op: BinOp, left: &Expr, right: &Expr) -> Gen<Expr> {
        use Repr::{Bool, Float, Int};
        let (l, r) = (&left.code, &right.code);
        let expr = match (left.repr, right.repr, op) {
            (Int, Int, BinOp::Add) => {
                self.fallible_expr(block, &format!("vm.add_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Sub) => {
                self.fallible_expr(block, &format!("vm.sub_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Mul) => {
                self.fallible_expr(block, &format!("vm.mul_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Div) => {
                self.fallible_expr(block, &format!("vm.div_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Mod) => {
                self.fallible_expr(block, &format!("vm.mod_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Shl) => {
                self.fallible_expr(block, &format!("vm.shl_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Shr) => {
                self.fallible_expr(block, &format!("vm.shr_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::BitAnd) => Expr::new(format!("({l} & {r})"), Int),
            (Int, Int, BinOp::BitOr) => Expr::new(format!("({l} | {r})"), Int),
            (Int, Int, BinOp::BitXor) => Expr::new(format!("({l} ^ {r})"), Int),
            (Int, Int, BinOp::Eq) => Expr::new(format!("({l} == {r})"), Bool),
            (Int, Int, BinOp::Ne) => Expr::new(format!("({l} != {r})"), Bool),
            (Int, Int, BinOp::Lt) => Expr::new(format!("({l} < {r})"), Bool),
            (Int, Int, BinOp::Le) => Expr::new(format!("({l} <= {r})"), Bool),
            (Int, Int, BinOp::Gt) => Expr::new(format!("({l} > {r})"), Bool),
            (Int, Int, BinOp::Ge) => Expr::new(format!("({l} >= {r})"), Bool),
            (Float, Float, BinOp::Add) => Expr::new(format!("({l} + {r})"), Float),
            (Float, Float, BinOp::Sub) => Expr::new(format!("({l} - {r})"), Float),
            (Float, Float, BinOp::Mul) => Expr::new(format!("({l} * {r})"), Float),
            (Float, Float, BinOp::Div) => Expr::new(format!("({l} / {r})"), Float),
            (Float, Float, BinOp::Mod) => Expr::new(format!("({l} % {r})"), Float),
            (
                Float,
                Float,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge,
            ) => Expr::new(
                format!(
                    "bex_vm_types::float_order::apply(CmpOp::{}, {l}, {r})",
                    cmp_name(op)
                ),
                Bool,
            ),
            (Bool, Bool, BinOp::Eq) => Expr::new(format!("({l} == {r})"), Bool),
            (Bool, Bool, BinOp::Ne) => Expr::new(format!("({l} != {r})"), Bool),
            (Bool, Bool, BinOp::BitAnd) => Expr::new(format!("({l} & {r})"), Bool),
            (Bool, Bool, BinOp::BitOr) => Expr::new(format!("({l} | {r})"), Bool),
            (Bool, Bool, BinOp::BitXor) => Expr::new(format!("({l} ^ {r})"), Bool),
            _ => {
                // The interpreter's generic operator over tagged values.
                let l = self.value_arg(l, left.repr);
                let r = self.value_arg(r, right.repr);
                match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let code = self.fallible(
                            block,
                            &format!("vm.cmpop(CmpOp::{}, {l}, {r})", cmp_name(op)),
                        )?;
                        Expr::new(code, Repr::Value)
                    }
                    _ => {
                        let code = self.fallible(
                            block,
                            &format!("vm.binop(BinOp::{}, {l}, {r})", binop_name(op)),
                        )?;
                        Expr::new(code, Repr::Value)
                    }
                }
            }
        };
        Ok(expr)
    }

    fn unary(&mut self, block: BlockId, op: UnaryOp, operand: &Expr) -> Gen<Expr> {
        let code = &operand.code;
        Ok(match (op, operand.repr) {
            (UnaryOp::Not, Repr::Bool) => Expr::new(format!("(!{code})"), Repr::Bool),
            (UnaryOp::Not, repr) => {
                let value = self.value_arg(code, repr);
                Expr::new(format!("vm.not({value})"), Repr::Bool)
            }
            (UnaryOp::Neg, Repr::Int) => {
                self.fallible_expr(block, &format!("vm.neg_i64({code})"), Repr::Int)?
            }
            (UnaryOp::Neg, Repr::Float) => Expr::new(format!("(-{code})"), Repr::Float),
            (UnaryOp::Neg, repr) => {
                let value = self.value_arg(code, repr);
                self.fallible_expr(block, &format!("vm.neg({value})"), Repr::Value)?
            }
            (UnaryOp::Truthy, Repr::Bool) => Expr::new(code.clone(), Repr::Bool),
            (UnaryOp::Truthy, repr) => {
                let value = self.value_arg(code, repr);
                Expr::new(format!("vm.is_truthy({value})"), Repr::Bool)
            }
        })
    }
}

/// A template that reads no frame type argument: the only kind a body with
/// no type arguments of its own can resolve.
fn frame_free(template: &TyTemplate) -> Gen<()> {
    let mut refs = 0;
    template.for_each_type_arg_ref(&mut |_| refs += 1);
    if refs > 0 {
        return unsupported("type known only at runtime");
    }
    Ok(())
}
