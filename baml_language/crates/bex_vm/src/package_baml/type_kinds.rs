//! BEP-066 reflection kind views over the existing minted `Object::Type`.

use bex_heap::TlabHolder;
use bex_vm_types::types::{Object, Value};
use indexmap::IndexMap;

use super::{
    BamlClassReflectArrayType, BamlClassReflectClassType, BamlClassReflectEnumType,
    BamlClassReflectFunctionType, BamlClassReflectInterfaceType, BamlClassReflectLiteralType,
    BamlClassReflectMapType, BamlClassReflectPrimitiveType, BamlClassReflectUnionType,
    BamlClassTypeValue, BamlNamespaceReflectArray, BamlNamespaceReflectClass,
    BamlNamespaceReflectEnum, BamlNamespaceReflectFunction, BamlNamespaceReflectInterface,
    BamlNamespaceReflectLiteral, BamlNamespaceReflectMap, BamlNamespaceReflectPrimitive,
    BamlNamespaceReflectUnion, PackageBamlImpl, copy,
};
use crate::BexVm;

impl BamlNamespaceReflectArray for PackageBamlImpl {}
impl BamlNamespaceReflectClass for PackageBamlImpl {}
impl BamlNamespaceReflectEnum for PackageBamlImpl {}
impl BamlNamespaceReflectFunction for PackageBamlImpl {}
impl BamlNamespaceReflectInterface for PackageBamlImpl {}
impl BamlNamespaceReflectLiteral for PackageBamlImpl {}
impl BamlNamespaceReflectMap for PackageBamlImpl {}
impl BamlNamespaceReflectPrimitive for PackageBamlImpl {}
impl BamlNamespaceReflectUnion for PackageBamlImpl {}

fn reflected_ty(vm: &BexVm, value: Value) -> baml_type::RealizedTy {
    super::type_class::type_value_ty(vm, value)
        .unwrap_or_else(|| unreachable!("kind method receiver must be Object::Type"))
}

fn reflected_class(vm: &BexVm, value: Value) -> (bex_vm_types::Class, Vec<baml_type::RealizedTy>) {
    let type_value = value
        .as_object_ptr()
        .and_then(|ptr| match vm.get_object(ptr) {
            Object::Type(value) => Some(&**value),
            _ => None,
        })
        .unwrap_or_else(|| unreachable!("class.Type receiver must be Object::Type"));
    let baml_type::RealizedTy::Class(name, args, _) = &type_value.ty else {
        unreachable!("class.Type receiver must wrap a class type")
    };
    let ptr =
        if type_value.owner.as_ptr().is_null() {
            vm.lookup_type(name)
        } else {
            let Object::Package(package) = vm.get_object(type_value.owner) else {
                unreachable!("runtime type owner must be a Package")
            };
            package.classes.values().find(|ptr| {
            matches!(vm.get_object(**ptr), Object::Class(class) if class.name == *name)
        }).copied()
        }
        .unwrap_or_else(|| unreachable!("reflected class {name} must be loaded"));
    let Object::Class(class) = vm.get_object(ptr) else {
        unreachable!("reflected class name resolved to a non-class")
    };
    ((**class).clone(), args.clone())
}

fn reflected_enum(vm: &BexVm, value: Value) -> bex_vm_types::Enum {
    let baml_type::RealizedTy::Enum(name, _) = reflected_ty(vm, value) else {
        unreachable!("enum.Type receiver must wrap an enum type")
    };
    let ptr = vm
        .lookup_type(&name)
        .unwrap_or_else(|| unreachable!("reflected enum {name} must be loaded"));
    let Object::Enum(enm) = vm.get_object(ptr) else {
        unreachable!("reflected enum name resolved to a non-enum")
    };
    (**enm).clone()
}

fn opt_string(vm: &mut BexVm, value: Option<&str>) -> Value {
    value.map_or(Value::NULL, |s| Value::object(vm.alloc_string(s)))
}

fn alloc_meta(
    vm: &mut BexVm,
    alias: Option<&str>,
    description: Option<&str>,
    docstring: Option<&str>,
    other: &IndexMap<String, String>,
) -> Value {
    let mut entries = IndexMap::with_capacity(other.len());
    for (key, value) in other {
        entries.insert(
            bex_str::BexStr::from(key.as_str()),
            Value::object(vm.alloc_string(value.as_str())),
        );
    }
    let other = Value::object(vm.alloc_map(
        baml_type::RealizedTy::string(),
        baml_type::RealizedTy::string(),
        entries,
    ));
    let alias = opt_string(vm, alias);
    let description = opt_string(vm, description);
    let docstring = opt_string(vm, docstring);
    copy::reflect::Meta {
        alias,
        description,
        docstring,
        other,
    }
    .to_value(vm)
}

macro_rules! impl_as_type {
    ($trait_name:ident) => {
        fn as_type(_vm: &BexVm, r#type: &Value) -> Value {
            *r#type
        }
    };
}

impl BamlClassReflectClassType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectClassType);

    fn fields(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let (class, args) = reflected_class(vm, *r#type);
        class
            .fields
            .iter()
            .map(|field| {
                let name = Value::object(vm.alloc_string(field.name.as_str()));
                let ty = field
                    .field_template
                    .substitute(&args, vm)
                    .unwrap_or_else(|err| {
                        unreachable!("emitted class field template must realize: {err}")
                    });
                let r#type = Value::object(vm.alloc_static_type(ty));
                let meta = alloc_meta(
                    vm,
                    field.alias.as_deref(),
                    field.description.as_deref(),
                    field.docstring.as_deref(),
                    &field.other,
                );
                copy::reflect::class::Field { name, r#type, meta }.to_value(vm)
            })
            .collect()
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Value {
        let (class, _) = reflected_class(vm, *r#type);
        alloc_meta(
            vm,
            class.alias.as_deref(),
            class.description.as_deref(),
            class.docstring.as_deref(),
            &class.other,
        )
    }
}

impl BamlClassReflectEnumType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectEnumType);

    fn values(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let enm = reflected_enum(vm, *r#type);
        enm.variants
            .iter()
            .map(|variant| {
                let name = Value::object(vm.alloc_string(variant.name.as_str()));
                let meta = alloc_meta(
                    vm,
                    variant.alias.as_deref(),
                    variant.description.as_deref(),
                    variant.docstring.as_deref(),
                    &variant.other,
                );
                copy::reflect::r#enum::Value { name, meta }.to_value(vm)
            })
            .collect()
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Value {
        let enm = reflected_enum(vm, *r#type);
        alloc_meta(
            vm,
            enm.alias.as_deref(),
            enm.description.as_deref(),
            enm.docstring.as_deref(),
            &enm.other,
        )
    }
}

impl BamlClassReflectUnionType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectUnionType);

    fn member_types(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let baml_type::RealizedTy::Union(members, _) = reflected_ty(vm, *r#type) else {
            unreachable!("union.Type receiver must wrap a union type")
        };
        members
            .into_iter()
            .map(|ty| Value::object(vm.alloc_static_type(ty)))
            .collect()
    }
}

impl BamlClassReflectArrayType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectArrayType);

    fn element_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::List(element, _) = reflected_ty(vm, *r#type) else {
            unreachable!("array.Type receiver must wrap an array type")
        };
        Value::object(vm.alloc_static_type(*element))
    }
}

impl BamlClassReflectMapType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectMapType);

    fn key_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Map { key, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("map.Type receiver must wrap a map type")
        };
        Value::object(vm.alloc_static_type(*key))
    }

    fn value_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Map { value, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("map.Type receiver must wrap a map type")
        };
        Value::object(vm.alloc_static_type(*value))
    }
}

impl BamlClassReflectFunctionType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectFunctionType);

    fn params(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let baml_type::RealizedTy::Function { params, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("function.Type receiver must wrap a function type")
        };
        params
            .into_iter()
            .map(|param| {
                let name = opt_string(vm, param.name.as_ref().map(baml_type::Name::as_str));
                let optional = param.is_optional();
                let r#type = Value::object(vm.alloc_static_type(param.ty));
                copy::reflect::function::Parameter {
                    name,
                    r#type,
                    optional,
                }
                .to_value(vm)
            })
            .collect()
    }

    fn return_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Function { ret, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("function.Type receiver must wrap a function type")
        };
        Value::object(vm.alloc_static_type(*ret))
    }
}

impl BamlClassReflectInterfaceType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectInterfaceType);

    fn implemented_by(vm: &BexVm, r#type: &Value, other: &Value) -> bool {
        <PackageBamlImpl as BamlClassTypeValue>::implemented_by(vm, r#type, other)
    }

    fn implementors(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        <PackageBamlImpl as BamlClassTypeValue>::implementors(vm, r#type)
    }
}

impl BamlClassReflectLiteralType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectLiteralType);
}

impl BamlClassReflectPrimitiveType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectPrimitiveType);
}
