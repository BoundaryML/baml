//! Every [`TypeHead`] an [`Object`] reaches.
//!
//! A head is a pointer into the heap, so the collector must trace and repoint it
//! exactly like any other reference. The walk *within* a type is generated
//! (`visit_heads`/`visit_heads_mut`), so a new head-bearing position in the type
//! family is covered automatically. The list of type-carrying **object fields**
//! below is not: adding one means adding it here, and the loader's closing
//! assertion is what catches a miss.
//!
//! Three callers share this: the loader binds each head to the declaration it
//! names, the collector traces heads as live edges, and it repoints them when a
//! declaration moves. All three need *every* head, so the shared and mutable
//! forms are expanded from one macro rather than written twice — a body that
//! could drift is a body that will.
//!
//! `Object::Future` walks through [`crate::types::Future::visit_heads`]: its
//! output types live behind accessors, but their heads are ordinary heap
//! edges — the future's *root* walk only repoints the future object itself,
//! so liveness and repointing of the output-type declarations happen here.

use crate::{Object, TypeHead};

macro_rules! walk_object_heads {
    ($name:ident, $visit:ident $(, $mut:tt)?) => {
        /// Call `f` on every head `object` reaches.
        pub fn $name(object: &$($mut)? Object, f: &mut impl FnMut(&$($mut)? TypeHead)) {
            match object {
                Object::Class(class) => {
                    for field in &$($mut)? class.fields {
                        field.field_type.$visit(f);
                        field.field_template.$visit(f);
                        if let Some(exact) = &$($mut)? field.runtime_type {
                            exact.ty.$visit(f);
                        }
                    }
                }
                Object::Interface(iface) => {
                    for (_, bounds) in &$($mut)? iface.args {
                        for bound in bounds {
                            bound.$visit(f);
                        }
                    }
                    for req in &$($mut)? iface.requires {
                        req.$visit(f);
                    }
                    for (_, assoc) in &$($mut)? iface.assoc {
                        assoc.$visit(f);
                    }
                    for field in &$($mut)? iface.fields {
                        field.ty.$visit(f);
                    }
                    for method in &$($mut)? iface.methods {
                        for arg in &$($mut)? method.args {
                            arg.$visit(f);
                        }
                        for (_, kwarg) in &$($mut)? method.kwargs {
                            kwarg.$visit(f);
                        }
                        method.returns.$visit(f);
                        method.errors.$visit(f);
                    }
                }
                Object::ImplRule(rule) => {
                    rule.for_ty_pattern.$visit(f);
                    for bounds in &$($mut)? rule.generic_param_bounds {
                        for bound in bounds {
                            f(&$($mut)? bound.interface);
                            for arg in &$($mut)? bound.args {
                                arg.$visit(f);
                            }
                            for (_, assoc) in &$($mut)? bound.assoc {
                                assoc.$visit(f);
                            }
                        }
                    }
                    for arg in &$($mut)? rule.interface_args {
                        arg.$visit(f);
                    }
                    for (_, assoc) in &$($mut)? rule.interface_assoc {
                        assoc.$visit(f);
                    }
                    for (_, method) in &$($mut)? rule.methods {
                        for frame in &$($mut)? method.frame {
                            frame.$visit(f);
                        }
                    }
                }
                Object::Function(func) => {
                    func.return_type.$visit(f);
                    func.throws_type.$visit(f);
                    for param in &$($mut)? func.param_types {
                        param.$visit(f);
                    }
                    for bounds in &$($mut)? func.generic_param_bounds {
                        for bound in bounds {
                            f(&$($mut)? bound.interface);
                            for arg in &$($mut)? bound.args {
                                arg.$visit(f);
                            }
                            for (_, assoc) in &$($mut)? bound.assoc {
                                assoc.$visit(f);
                            }
                        }
                    }
                    for constant in &$($mut)? func.bytecode.constants {
                        match constant {
                            crate::ConstValue::Type(template) => template.$visit(f),
                            crate::ConstValue::ClassWithTypeArgs {
                                type_args_templates,
                                ..
                            } => {
                                for template in type_args_templates {
                                    template.$visit(f);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Object::TypeAlias(alias) => alias.definition.$visit(f),
                Object::Type(value) => value.ty.$visit(f),
                Object::Instance(instance) => {
                    for arg in &$($mut)? *instance.class_type_args {
                        arg.$visit(f);
                    }
                }
                Object::Closure(closure) => {
                    for arg in &$($mut)? *closure.captured_type_args {
                        arg.$visit(f);
                    }
                }
                Object::BoundMethod(bm) => {
                    for arg in &$($mut)? *bm.type_args {
                        arg.$visit(f);
                    }
                }
                Object::GenericFunction(gf) => {
                    for arg in &$($mut)? *gf.type_args {
                        arg.$visit(f);
                    }
                }
                Object::HostClosure(hc) => {
                    hc.ret_ty.$visit(f);
                    hc.throws_ty.$visit(f);
                    for param in &$($mut)? **hc.params {
                        param.ty.$visit(f);
                    }
                }
                Object::UnscheduledFuture(fut) => {
                    fut.returns.$visit(f);
                    fut.throws.$visit(f);
                }
                Object::Future(fut) => fut.$visit(f),
                Object::Array(array) => array.element_ty.$visit(f),
                Object::Map(map) => {
                    map.key_ty.$visit(f);
                    map.value_ty.$visit(f);
                }
                // Carry no head. `Package` and `Enum` reference declarations by
                // pointer, and the rest are plain values.
                Object::Package(_)
                | Object::Enum(_)
                | Object::Variant(_)
                | Object::Cell(_)
                | Object::String(_)
                | Object::Bigint(_)
                | Object::Uint8Array(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Float(_) => {}
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => {}
            }
        }
    };
}

walk_object_heads!(visit_object_heads, visit_heads);
walk_object_heads!(visit_object_heads_mut, visit_heads_mut, mut);
