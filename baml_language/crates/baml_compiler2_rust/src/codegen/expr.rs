//! Rvalues: operators, type tests, and aggregates.

use baml_compiler2_hir_ty::extern_loc::{ClassRef, FunctionRef};
use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, Operand, Rvalue, TyTemplate, TyTemplateInterface, TypeTest,
    UnaryOp, class_display_name, enum_display_name, function_display_name,
};
use baml_type::typetag;
use bex_vm_types::ConstValue;

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
                let value = self.value_operand(block, operand)?;
                Ok(Expr::new(
                    format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)"),
                    Repr::Bool,
                ))
            }
            Rvalue::IsType { operand, test } => {
                let value = self.value_operand(block, operand)?;
                let code = self.type_test(&format!("({value})"), test)?;
                Ok(Expr::new(code, Repr::Bool))
            }
            Rvalue::Array(template, elements) => self.array_literal(block, template, elements),
            Rvalue::Aggregate {
                kind:
                    AggregateKind::Class {
                        class,
                        type_arg_templates,
                    },
                fields,
            } => self.class_literal(block, *class, type_arg_templates, fields),
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ..
            } => unsupported("untyped array aggregate"),
            Rvalue::Map(key, value, entries) => {
                // Map entries must be lowered to Map.set calls.
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
            Rvalue::Uint8Array(bytes) => self.byte_array_literal(block, bytes),
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                type_arg_templates,
            } => self.closure_value(block, *lambda_idx, captures, type_arg_templates),
            Rvalue::MakeBoundMethod { func, receiver } => {
                self.bound_method_value(block, *func, receiver)
            }
            Rvalue::MakeGenericFunction {
                func,
                type_arg_templates,
            } => self.generic_function_value(*func, type_arg_templates),
            Rvalue::MakeGenericFunctionFromValue {
                value,
                type_arg_templates,
            } => self.specialized_function_value(block, value, type_arg_templates),
            Rvalue::MakeVirtualBoundMethod {
                iface,
                method,
                receiver,
                type_args,
            } => self.virtual_bound_method_value(block, iface, method, receiver, type_args),
            Rvalue::MakeVirtualFunction {
                self_ty,
                iface,
                method,
                type_args,
            } => self.virtual_function_value(block, self_ty, iface, method, type_args),
            Rvalue::VirtualFieldAccess {
                iface,
                receiver,
                field_index,
                ..
            } => self.virtual_field_value(block, iface, receiver, *field_index),
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
            // Null when the hook is not to run; the hook call then takes the
            // VM's call path (see `analysis::hook_call`).
            Rvalue::TraceHookSettings { with_settings } => self.fallible_expr(
                block,
                &format!("vm.begin_trace_hook(depth, {with_settings})"),
                Repr::Value,
            ),
            Rvalue::SatisfiesBound { ty, bound } => {
                let value = self.value_operand(block, ty)?;
                let template = bound.to_template();
                let realized = self.realized(&template)?;
                let bound = self.hoist(Expr::new(
                    format!("vm.type_value(depth, None, {realized})"),
                    Repr::Value,
                ));
                self.fallible_expr(
                    block,
                    &format!("vm.bound_satisfied({value}, {})", bound.code),
                    Repr::Bool,
                )
            }
        }
    }

    fn array_literal(
        &mut self,
        block: BlockId,
        template: &TyTemplate,
        elements: &[Operand<'db>],
    ) -> Gen<Expr> {
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

    fn class_literal(
        &mut self,
        block: BlockId,
        class: ClassRef<'db>,
        type_arg_templates: &[TyTemplate],
        fields: &[Operand<'db>],
    ) -> Gen<Expr> {
        let name = class_display_name(self.db, class);
        let Some((object, field_count)) = self.index.class(self.db, class)? else {
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
        let class = format!("vm.idx_to_ptr(ObjectIndex::from_raw({object}))");
        if type_arg_templates.is_empty() {
            return Ok(Expr::new(
                format!("vm.new_instance({class}, vec![{}])", values.join(", ")),
                Repr::Value,
            ));
        }
        let type_args = self.realized_all(type_arg_templates)?;
        Ok(Expr::new(
            format!(
                "vm.new_generic_instance({class}, Box::new([{}]), vec![{}])",
                type_args.join(", "),
                values.join(", ")
            ),
            Repr::Value,
        ))
    }

    fn byte_array_literal(&mut self, block: BlockId, bytes: &[u8]) -> Gen<Expr> {
        // Each evaluation copies the pooled literal so mutations stay local.
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
            return Err(Rejection::Invalid(format!(
                "yielding builtin `{key}` emitted inline"
            )));
        }
        let ordinal = self.builtin_ordinal(key);
        let template = self.hoist(Expr::new(
            format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
            Repr::Value,
        ));
        let code = self.fallible(
            block,
            &format!("vm.call_builtin(builtin_{ordinal}(), &[{}])", template.code),
        )?;
        Ok(Expr::new(code, Repr::Value))
    }

    fn closure_value(
        &mut self,
        block: BlockId,
        lambda_idx: usize,
        captures: &[Operand<'db>],
        type_arg_templates: &[TyTemplate],
    ) -> Gen<Expr> {
        let Some(lambda) = self.lambdas.get(lambda_idx) else {
            return Err(Rejection::Invalid(format!(
                "closure over lambda {lambda_idx} of a function with {}",
                self.lambdas.len()
            )));
        };
        let Some(object) = self.index.lambda(self.db, &lambda.identity)? else {
            let name = lambda.identity.display_name(self.db);
            return unsupported(format!("lambda `{name}` not found in the program"));
        };
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

    fn bound_method_value(
        &mut self,
        block: BlockId,
        func: FunctionRef<'db>,
        receiver: &Operand<'db>,
    ) -> Gen<Expr> {
        let name = function_display_name(self.db, func);
        let Some((object, _)) = self.index.function(self.db, func)? else {
            return unsupported(format!("method `{name}` not found in the program"));
        };
        let receiver = self.value_operand(block, receiver)?;
        Ok(Expr::new(
            format!(
                "vm.make_bound_method(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {receiver})"
            ),
            Repr::Value,
        ))
    }

    fn generic_function_value(
        &mut self,
        func: FunctionRef<'db>,
        type_arg_templates: &[TyTemplate],
    ) -> Gen<Expr> {
        let name = function_display_name(self.db, func);
        let Some((object, _)) = self.index.function(self.db, func)? else {
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

    fn specialized_function_value(
        &mut self,
        block: BlockId,
        value: &Operand<'db>,
        type_arg_templates: &[TyTemplate],
    ) -> Gen<Expr> {
        let value = self.value_operand(block, value)?;
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

    fn virtual_bound_method_value(
        &mut self,
        block: BlockId,
        iface: &TyTemplateInterface,
        method: &str,
        receiver: &Operand<'db>,
        type_args: &[TyTemplate],
    ) -> Gen<Expr> {
        let (iface, iface_args, _) = self.interface_operands(iface)?;
        let types = self.realized_all(type_args)?;
        let receiver = self.value_operand(block, receiver)?;
        let code = self.fallible(
            block,
            &format!(
                "vm.make_virtual_bound_method({receiver}, {iface}, {iface_args}, {method:?}, &[{}])",
                types.join(", ")
            ),
        )?;
        Ok(Expr::new(code, Repr::Value))
    }

    fn virtual_function_value(
        &mut self,
        block: BlockId,
        self_ty: &TyTemplate,
        iface: &TyTemplateInterface,
        method: &str,
        type_args: &[Operand<'db>],
    ) -> Gen<Expr> {
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

    fn virtual_field_value(
        &mut self,
        block: BlockId,
        iface: &TyTemplateInterface,
        receiver: &Operand<'db>,
        field_index: u32,
    ) -> Gen<Expr> {
        let (iface, iface_args, _) = self.interface_operands(iface)?;
        let receiver = self.value_operand(block, receiver)?;
        let code = self.fallible(
            block,
            &format!("vm.virtual_load_field({receiver}, {iface}, {iface_args}, {field_index})"),
        )?;
        Ok(Expr::new(code, Repr::Value))
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
                // Class type arguments are invariant.
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

    fn template_test(&mut self, value: &str, template: &TyTemplate) -> Gen<String> {
        frame_free(template)?;
        let tag = |tag: i64| format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)");
        let constant = match template {
            TyTemplate::Class(..) | TyTemplate::Enum(..) => {
                return Err(Rejection::Invalid(
                    "a class or enum test reaches the backend as a declaration test".into(),
                ));
            }
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

    fn builtin_ordinal(&mut self, key: &str) -> usize {
        let next = self.builtins.len();
        *self.builtins.entry(key.to_owned()).or_insert(next)
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

fn frame_free(template: &TyTemplate) -> Gen<()> {
    let mut refs = 0;
    template.for_each_type_arg_ref(&mut |_| refs += 1);
    if refs > 0 {
        return unsupported("type known only at runtime");
    }
    Ok(())
}
