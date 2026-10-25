//! Rvalues: operators, type tests, and aggregates.

use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, BoundFrame, Rvalue, TyTemplate, TypeTest, UnaryOp,
    class_display_name, enum_display_name, function_display_name,
};
use baml_type::typetag;
use bex_vm_types::ConstValue;

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    Rejection,
    analysis::{CallShape, Repr, builtin_is_callable, reads_frame, static_realized},
    render::{binop_name, cmp_name, object_value},
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn rvalue(&mut self, block: BlockId, rvalue: &Rvalue<'db>) -> Gen<Expr> {
        let code = match rvalue {
            Rvalue::Use(operand) => return self.operand(block, operand),
            Rvalue::BinaryOp { op, left, right } => {
                let left = self.operand(block, left)?;
                let right = self.operand(block, right)?;
                return self.binary(block, *op, &left, &right);
            }
            Rvalue::UnaryOp { op, operand } => {
                let operand = self.operand(block, operand)?;
                return self.unary(block, *op, &operand);
            }
            Rvalue::Len(place) => {
                let needs = self.analysis.escape.object_needs(place);
                let container = self.load(block, place)?;
                let container = self.value_arg(&container);
                let len = self.by_locality(
                    needs,
                    &format!("vm.container_len_local({container})"),
                    &format!("vm.container_len({container})"),
                );
                self.fallible(block, &len)?
            }
            Rvalue::Discriminant(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value);
                let code = self.fallible(block, &format!("vm.discriminant({value})"))?;
                return Ok(Expr::new(code, Repr::Int));
            }
            Rvalue::TypeTag(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value);
                return Ok(Expr::new(
                    format!("bex_vm::compiled_rt::type_tag({value})"),
                    Repr::Int,
                ));
            }
            Rvalue::IsTypeTag { operand, tag } => {
                let value = self.value_operand(block, operand)?;
                // PANIC is a membership pseudo-tag, never a value's own tag.
                // Match the bytecode IsType check so catch wildcards skip panics.
                if *tag == typetag::PANIC {
                    return Ok(Expr::new(format!("vm.is_panic({value})"), Repr::Bool));
                }
                return Ok(Expr::new(
                    format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)"),
                    Repr::Bool,
                ));
            }
            Rvalue::IsType { operand, test } => {
                let value = self.value_operand(block, operand)?;
                let code = self.type_test(&format!("({value})"), test)?;
                return Ok(Expr::new(code, Repr::Bool));
            }
            Rvalue::Array(template, elements) => {
                let element_ty = self.realized_type(template)?;
                let values = self.arg_values(block, elements)?;
                format!("vm.new_array({element_ty}, vec![{}])", values.join(", "))
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
                let Some((object, field_count)) = self.index.class(self.db, *class) else {
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                if fields.len() != field_count {
                    return unsupported(format!(
                        "`{name}` literal with {} of {field_count} fields",
                        fields.len()
                    ));
                }
                let values = self.arg_values(block, fields)?;
                let class = format!("vm.record_object(fr, ObjectIndex::from_raw({object}))");
                let values = values.join(", ");
                if type_arg_templates.is_empty() {
                    format!("vm.new_instance({class}, vec![{values}])")
                } else {
                    let type_args = self.realized_all(type_arg_templates)?.join(", ");
                    format!(
                        "vm.new_generic_instance({class}, Box::new([{type_args}]), vec![{values}])"
                    )
                }
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ..
            } => return unsupported("untyped array aggregate"),
            Rvalue::Map(key, value, entries) => {
                // Map entries must be lowered to Map.set calls.
                if !entries.is_empty() {
                    return unsupported("map literal with inline entries");
                }
                let key = self.realized_type(key)?;
                let value = self.realized_type(value)?;
                format!("vm.new_map({key}, {value})")
            }
            Rvalue::Uint8Array(bytes) => {
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
                let Some((_, info)) =
                    deep_copy.and_then(|function| self.index.function(self.db, function))
                else {
                    return unsupported("`baml.deep_copy` not found in the program");
                };
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported("builtin `baml.deep_copy` has no native key");
                };
                let shape = CallShape {
                    type_args_primitive: false,
                    map_key_primitive: false,
                    equality_primitive: false,
                };
                if !builtin_is_callable(key, shape) {
                    return Err(Rejection::Invalid(format!(
                        "yielding builtin `{key}` emitted inline"
                    )));
                }
                let ordinal = self.builtin_ordinal(key);
                let template = self.hoist(Expr::new(object_value(object), Repr::Value));
                self.fallible(
                    block,
                    &format!(
                        "vm.call_builtin(builtin_{ordinal}()?, &[{}])",
                        template.code
                    ),
                )?
            }
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
                let Some(object) = self.index.lambda(self.db, &lambda.identity) else {
                    let name = lambda.identity.display_name(self.db);
                    return unsupported(format!("lambda `{name}` not found in the program"));
                };
                let types = self.realized_all(type_arg_templates)?;
                let cells = self.arg_values(block, captures)?;
                format!(
                    "vm.make_closure(vm.record_object(fr, ObjectIndex::from_raw({object})), \
                 vec![{}], vec![{}].into_boxed_slice())",
                    cells.join(", "),
                    types.join(", ")
                )
            }
            Rvalue::MakeBoundMethod {
                func,
                receiver,
                frame: BoundFrame::Receiver,
            } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func) else {
                    return unsupported(format!("method `{name}` not found in the program"));
                };
                let receiver = self.value_operand(block, receiver)?;
                format!(
                    "vm.make_bound_method(vm.record_object(fr, ObjectIndex::from_raw({object})), {receiver})"
                )
            }
            Rvalue::MakeBoundMethod {
                frame: BoundFrame::Explicit(_),
                ..
            } => return unsupported("bound method with an explicit frame"),
            Rvalue::MakeGenericFunction {
                func,
                type_arg_templates,
            } => {
                let name = function_display_name(self.db, *func);
                let Some((object, _)) = self.index.function(self.db, *func) else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                let Some(&global) = self.index.globals.get(&object) else {
                    return unsupported(format!("function `{name}` has no global slot"));
                };
                let type_args = self.realized_all(type_arg_templates)?;
                format!(
                    "vm.make_generic_function(fr, GlobalIndex::from_raw({global}), Box::new([{}]))",
                    type_args.join(", ")
                )
            }
            Rvalue::MakeGenericFunctionFromValue {
                value,
                type_arg_templates,
            } => {
                let value = self.value_operand(block, value)?;
                let type_args = self.realized_all(type_arg_templates)?;
                self.fallible(
                    block,
                    &format!(
                        "vm.specialize_function_value({value}, &[{}])",
                        type_args.join(", ")
                    ),
                )?
            }
            Rvalue::MakeVirtualBoundMethod {
                iface,
                method,
                receiver,
                type_args,
            } => {
                let (iface, iface_args) = self.interface_operands(iface)?;
                let types = self.realized_all(type_args)?;
                let receiver = self.value_operand(block, receiver)?;
                self.fallible(block, &format!(
                "vm.make_virtual_bound_method({receiver}, {iface}, {iface_args}, {method:?}, &[{}])",
                types.join(", ")
            ))?
            }
            Rvalue::MakeVirtualFunction {
                self_ty,
                iface,
                method,
                type_args,
            } => {
                let self_ty = self.realized(self_ty)?;
                let (iface, iface_args) = self.interface_operands(iface)?;
                let types = self.type_operands("interface function value", type_args)?;
                self.fallible(block, &format!(
                "vm.make_virtual_function(&{self_ty}, {iface}, {iface_args}, {method:?}, &[{}])",
                types.join(", ")
            ))?
            }
            Rvalue::VirtualFieldAccess {
                iface,
                receiver,
                field_index,
                ..
            } => {
                let (iface, iface_args) = self.interface_operands(iface)?;
                let receiver = self.value_operand(block, receiver)?;
                self.fallible(
                    block,
                    &format!(
                        "vm.virtual_load_field({receiver}, {iface}, {iface_args}, {field_index})"
                    ),
                )?
            }
            Rvalue::LoadType(template) => return self.type_value(template),
            Rvalue::CurrentPackage => {
                let Some(ordinal) = self.package else {
                    return unsupported("current package of a function the program does not place");
                };
                format!("vm.current_package({ordinal})")
            }
            // Null when the hook is not to run; the hook call then takes the
            // VM's call path.
            Rvalue::TraceHookSettings { with_settings } => self.fallible(
                block,
                &format!("vm.compiled_begin_trace_hook(fr, {with_settings})"),
            )?,
            Rvalue::SatisfiesBound { ty, bound } => {
                let value = self.value_operand(block, ty)?;
                let template = bound.to_template();
                let realized = self.realized(&template)?;
                let bound = self.hoist(Expr::new(
                    format!("vm.type_value(fr, None, {realized})"),
                    Repr::Value,
                ));
                return self.fallible_expr(
                    block,
                    &format!("vm.bound_satisfied({value}, {})", bound.code),
                    Repr::Bool,
                );
            }
        };
        Ok(Expr::new(code, Repr::Value))
    }

    pub(super) fn type_test(&mut self, value: &str, test: &TypeTest<'db>) -> Gen<String> {
        match test {
            TypeTest::Class { class, args } => {
                let Some((object, _)) = self.index.class(self.db, *class) else {
                    let name = class_display_name(self.db, *class);
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                if args.is_empty() {
                    return Ok(format!(
                        "vm.is_instance_of({value}, vm.record_object(fr, ObjectIndex::from_raw({object})))"
                    ));
                }
                // Class type arguments are invariant.
                let type_args = self.realized_all(args)?;
                Ok(format!(
                    "vm.is_instance_of_at({value}, vm.record_object(fr, ObjectIndex::from_raw({object})), &[{}])",
                    type_args.join(", ")
                ))
            }
            TypeTest::Enum(enum_ref) => {
                let Some(object) = self.index.enum_(self.db, *enum_ref) else {
                    let name = enum_display_name(self.db, *enum_ref);
                    return unsupported(format!("enum `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_variant_of({value}, vm.record_object(fr, ObjectIndex::from_raw({object})))"
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
                let Some(anchored) = self.index.anchor(template) else {
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
            "vm.is_type_constant(vm.record_object(fr, ObjectIndex::from_raw({})), {index}, {value})?",
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
        let Some(anchored) = self.index.anchor(template) else {
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
            "let {name}: RealizedTy = vm.type_constant(vm.record_object(fr, ObjectIndex::from_raw({})), {index})?;",
            self.object
        ));
        Ok(name)
    }

    pub(super) fn builtin_ordinal(&mut self, key: &str) -> usize {
        let next = self.builtins.len();
        *self.builtins.entry(key.to_owned()).or_insert(next)
    }

    fn binary(&mut self, block: BlockId, op: BinOp, left: &Expr, right: &Expr) -> Gen<Expr> {
        use Repr::{Bool, Float, Int};
        let (l, r) = (&left.code, &right.code);
        let comparison = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        );
        let repr = match (left.repr, right.repr, op) {
            (
                Int,
                Int,
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::Shl
                | BinOp::Shr,
            ) => {
                return self.fallible_expr(
                    block,
                    &format!("vm.{}_i64({l}, {r})", binop_name(op).to_ascii_lowercase()),
                    Int,
                );
            }
            (Float, Float, _) if comparison => {
                return Ok(Expr::new(
                    format!(
                        "bex_vm_types::float_order::apply(CmpOp::{}, {l}, {r})",
                        cmp_name(op)
                    ),
                    Bool,
                ));
            }
            (Int, Int, _) if comparison => Bool,
            (Int, Int, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) => Int,
            (Bool, Bool, BinOp::Eq | BinOp::Ne | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) => {
                Bool
            }
            (Float, Float, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod) => Float,
            _ => {
                let l = self.value_arg(left);
                let r = self.value_arg(right);
                let call = if comparison {
                    format!("vm.cmpop(CmpOp::{}, {l}, {r})", cmp_name(op))
                } else {
                    format!("vm.binop(BinOp::{}, {l}, {r})", binop_name(op))
                };
                return self.fallible_expr(block, &call, Repr::Value);
            }
        };
        Ok(Expr::new(format!("({l} {op} {r})"), repr))
    }

    fn unary(&mut self, block: BlockId, op: UnaryOp, operand: &Expr) -> Gen<Expr> {
        let code = &operand.code;
        Ok(match (op, operand.repr) {
            (UnaryOp::Not, Repr::Bool) => Expr::new(format!("(!{code})"), Repr::Bool),
            (UnaryOp::Not, _) => {
                let value = self.value_arg(operand);
                Expr::new(format!("vm.not({value})"), Repr::Bool)
            }
            (UnaryOp::Neg, Repr::Int) => {
                self.fallible_expr(block, &format!("vm.neg_i64({code})"), Repr::Int)?
            }
            (UnaryOp::Neg, Repr::Float) => Expr::new(format!("(-{code})"), Repr::Float),
            (UnaryOp::Neg, _) => {
                let value = self.value_arg(operand);
                self.fallible_expr(block, &format!("vm.neg({value})"), Repr::Value)?
            }
            (UnaryOp::Truthy, Repr::Bool) => Expr::new(code.clone(), Repr::Bool),
            (UnaryOp::Truthy, _) => {
                let value = self.value_arg(operand);
                Expr::new(format!("vm.is_truthy({value})"), Repr::Bool)
            }
        })
    }
}
