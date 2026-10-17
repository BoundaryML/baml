//! Rvalues: operators, type tests, and aggregates.

use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, Rvalue, TypeTest, UnaryOp, class_display_name, enum_display_name,
};

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    admit::Rejection,
    analysis::{Repr, static_realized},
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
                let element_ty = static_realized(self.index, template).ok_or_else(|| {
                    Rejection::Unsupported("array of a non-primitive type".into())
                })?;
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
                if !type_arg_templates.is_empty() {
                    return unsupported("instance of a generic class");
                }
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
                Ok(Expr::new(
                    format!(
                        "vm.new_instance({}.as_object_ptr().expect(\"class object\"), vec![{}])",
                        class.code,
                        values.join(", ")
                    ),
                    Repr::Value,
                ))
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ..
            } => unsupported("untyped array aggregate"),
            Rvalue::Map(..) => unsupported("map literal"),
            Rvalue::Uint8Array(_) => unsupported("byte array literal"),
            Rvalue::MakeClosure { .. } => unsupported("closure"),
            Rvalue::MakeBoundMethod { .. }
            | Rvalue::MakeGenericFunction { .. }
            | Rvalue::MakeGenericFunctionFromValue { .. } => {
                unsupported("function used as a value")
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
                let (iface, iface_args) = self.static_interface(iface)?;
                let mut types = Vec::with_capacity(type_args.len());
                for template in type_args {
                    let Some(ty) = static_realized(self.index, template) else {
                        return unsupported(
                            "interface method value with a type argument known only at runtime",
                        );
                    };
                    types.push(ty);
                }
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
                let Some(self_ty) = static_realized(self.index, self_ty) else {
                    return unsupported("interface function value on a type known only at runtime");
                };
                let (iface, iface_args) = self.static_interface(iface)?;
                let types = self.static_type_operands("interface function value", type_args)?;
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
                let (iface, iface_args) = self.static_interface(iface)?;
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
            Rvalue::LoadType(_) => unsupported("type value"),
            Rvalue::CurrentPackage => unsupported("current package"),
            Rvalue::TraceHookSettings { .. } => unsupported("trace hook"),
        }
    }

    pub(super) fn type_test(&mut self, value: &str, test: &TypeTest<'db>) -> Gen<String> {
        match test {
            TypeTest::Class { class, args } => {
                if !args.is_empty() {
                    return unsupported("type test against a generic class");
                }
                let Some((object, _)) = self.index.class(self.db, *class)? else {
                    let name = class_display_name(self.db, *class);
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_instance_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
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
            TypeTest::Template(_) => unsupported("structural type test"),
        }
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
