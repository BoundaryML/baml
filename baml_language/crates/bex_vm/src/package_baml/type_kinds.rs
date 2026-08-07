//! BEP-066 reflection kind views over the existing minted `Object::Type`.

use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase};
use bex_heap::TlabHolder;
use bex_vm_types::types::{
    Class, ClassField, DynTypeDefs, Enum, EnumVariant, MintId, Object, RuntimeTypeProvenance,
    TypeValue, Value,
};
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
impl BamlNamespaceReflectClass for PackageBamlImpl {
    fn new(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        fields: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let class_name = name.as_str();
        let mut diagnostics = Vec::new();
        if !is_baml_identifier(class_name) {
            diagnostics.push(compiler_diagnostic(
                DiagnosticId::InvalidSyntax,
                format!("invalid class name `{class_name}`"),
            ));
        }

        let mut class_fields = Vec::with_capacity(fields.len());
        let mut child_defs = DynTypeDefs::default();
        let mut seen_serialized_keys = std::collections::HashSet::new();
        for (field_name, value) in fields {
            if !is_baml_identifier(field_name.as_str()) {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("invalid field name `{class_name}.{field_name}`"),
                ));
            }
            let row = match reflected_type_row(vm, *value) {
                Ok(row) => row,
                Err(message) => {
                    diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                    continue;
                }
            };
            let serialized_key = row.alias.as_deref().unwrap_or(field_name.as_str());
            if !seen_serialized_keys.insert(serialized_key.to_string()) {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::DuplicateFieldAlias,
                    format!("duplicate serialized key `{serialized_key}` in class `{class_name}`"),
                ));
            }
            child_defs.merge_from(row.type_value.defs());
            class_fields.push(ClassField {
                name: field_name.to_string(),
                field_type: row.type_value.ty.clone().into(),
                field_template: baml_type::TyTemplate::from(row.type_value.ty.clone()),
                description: row.description,
                alias: row.alias,
                docstring: row.docstring,
                other: row.other,
                skip: false,
                runtime_type: Some(row.type_value),
            });
        }
        if !diagnostics.is_empty() {
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let mint = vm.tlab.heap().mint_runtime_id();
        let MintId::Runtime(mint_number) = mint else {
            unreachable!("BexHeap::mint_runtime_id always returns a runtime mint")
        };
        let type_name = baml_type::QualifiedTypeName::runtime_local(
            baml_type::Name::new(class_name),
            mint_number,
        );
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(Class {
            name: type_name.clone(),
            fields: class_fields,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag: baml_type::typetag::class_type_tag(&type_name.to_string()),
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            runtime_type: Some(RuntimeTypeProvenance {
                mint,
                defs: child_defs.clone(),
            }),
        })));
        let ty = baml_type::RealizedTy::Class(
            type_name.clone(),
            Vec::new(),
            baml_type::TyAttr::default(),
        );
        child_defs.classes.insert(type_name, class_ptr);
        Ok(Value::object(vm.tlab.alloc_type(
            TypeValue::from_parts_with_defs(ty, mint, child_defs),
        )))
    }

    fn builder(vm: &mut BexVm, name: &bex_str::BexStr) -> Value {
        super::runtime_class_builder::alloc_builder(vm, name.as_str())
    }

    fn get_field(
        vm: &mut BexVm,
        value: &Value,
        name: &bex_str::BexStr,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let fail = |vm: &mut BexVm, message: String| {
            crate::errors::VmRustFnError::Thrown(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
            ))
        };
        let Some(instance_ptr) = value.as_object_ptr() else {
            return Err(fail(
                vm,
                format!(
                    "reflect.class.get_field expected a class instance, got {}",
                    vm.type_of(value)
                ),
            ));
        };
        let (field_value, class_name) = {
            let Object::Instance(instance) = vm.get_object(instance_ptr) else {
                return Err(fail(
                    vm,
                    format!(
                        "reflect.class.get_field expected a class instance, got {}",
                        vm.type_of(value)
                    ),
                ));
            };
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            let Some(index) = class
                .fields
                .iter()
                .position(|field| field.name == name.as_str())
            else {
                return Err(fail(
                    vm,
                    format!(
                        "class `{}` has no field `{name}`",
                        class.name.display_name()
                    ),
                ));
            };
            (
                instance.load_field(index),
                class.name.display_name().to_string(),
            )
        };
        let expected = vm
            .current_call_type_args()
            .first()
            .cloned()
            .unwrap_or_else(baml_type::RealizedTy::unknown);
        let matches = crate::type_match::value_matches_template(
            vm,
            field_value,
            &baml_type::TyTemplate::from(expected.clone()),
            &[],
        )
        .map_err(crate::errors::VmRustFnError::InternalError)?;
        if !matches {
            let got = vm.value_concrete_ty(field_value).map_or_else(
                || "unknown".to_string(),
                |ty| baml_type::RealizedTy::from(ty).to_string(),
            );
            return Err(fail(
                vm,
                format!("field `{class_name}.{name}` has type `{got}`, expected `{expected}`"),
            ));
        }
        Ok(field_value)
    }
}
impl BamlNamespaceReflectEnum for PackageBamlImpl {
    fn value(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        alias: Option<&bex_str::BexStr>,
        description: Option<&bex_str::BexStr>,
        docstring: Option<&bex_str::BexStr>,
        other: Option<&IndexMap<bex_str::BexStr, Value>>,
    ) -> Value {
        let other = other.map_or_else(IndexMap::new, |other| string_map(vm, other));
        let meta = alloc_meta(
            vm,
            alias.map(bex_str::BexStr::as_str),
            description.map(bex_str::BexStr::as_str),
            docstring.map(bex_str::BexStr::as_str),
            &other,
        );
        let name = Value::object(vm.alloc_string(name.clone()));
        copy::reflect::r#enum::Value { name, meta }.to_value(vm)
    }

    fn new(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        values: &[Value],
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let enum_name = name.as_str();
        let mut diagnostics = Vec::new();
        if !is_baml_identifier(enum_name) {
            diagnostics.push(compiler_diagnostic(
                DiagnosticId::InvalidSyntax,
                format!("invalid enum name `{enum_name}`"),
            ));
        }

        let mut variants = Vec::with_capacity(values.len());
        for value in values {
            match enum_row(vm, *value) {
                Ok(variant) => variants.push(variant),
                Err(message) => {
                    diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                }
            }
        }

        for variant in &variants {
            if !is_baml_identifier(&variant.name) {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::InvalidSyntax,
                    format!("invalid enum variant name `{}.{}`", enum_name, variant.name),
                ));
            }
        }

        let mut seen_names = std::collections::HashSet::new();
        for variant in &variants {
            if !seen_names.insert(variant.name.as_str()) {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::DuplicateField,
                    format!("duplicate variant `{enum_name}.{}`", variant.name),
                ));
            }
        }

        let mut seen_keys = std::collections::HashSet::new();
        for variant in &variants {
            let key = variant.alias.as_deref().unwrap_or(&variant.name);
            if !seen_keys.insert(key) {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::DuplicateFieldAlias,
                    format!("duplicate serialized key `{key}` in enum `{enum_name}`"),
                ));
            }
        }

        if !diagnostics.is_empty() {
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let mint = vm.tlab.heap().mint_runtime_id();
        let MintId::Runtime(mint_number) = mint else {
            unreachable!("BexHeap::mint_runtime_id always returns a runtime mint")
        };
        let type_name = baml_type::QualifiedTypeName::runtime_local(
            baml_type::Name::new(enum_name),
            mint_number,
        );
        let enum_ptr = vm.tlab.alloc(Object::Enum(Box::new(Enum {
            name: type_name.clone(),
            variants,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            ty_attr: baml_type::TyAttr::default(),
            runtime_type: Some(RuntimeTypeProvenance {
                mint,
                defs: DynTypeDefs::default(),
            }),
        })));
        let ty = baml_type::RealizedTy::Enum(type_name.clone(), baml_type::TyAttr::default());
        let defs = DynTypeDefs::with_enum(type_name, enum_ptr);
        let type_ptr = vm
            .tlab
            .alloc_type(TypeValue::from_parts_with_defs(ty, mint, defs));
        Ok(Value::object(type_ptr))
    }

    fn get_value(
        vm: &mut BexVm,
        value: &Value,
    ) -> Result<bex_str::BexStr, crate::errors::VmRustFnError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(invalid_enum_value(vm, *value));
        };
        let Object::Variant(variant) = vm.get_object(ptr) else {
            return Err(invalid_enum_value(vm, *value));
        };
        let Object::Enum(enm) = vm.get_object(variant.enm) else {
            unreachable!("Variant.enm must point to Object::Enum")
        };
        let name = enm
            .variants
            .get(variant.index)
            .unwrap_or_else(|| unreachable!("variant index validated at allocation"))
            .name
            .clone();
        Ok(bex_str::BexStr::from(name))
    }
}
impl BamlNamespaceReflectFunction for PackageBamlImpl {}
impl BamlNamespaceReflectInterface for PackageBamlImpl {}
impl BamlNamespaceReflectLiteral for PackageBamlImpl {
    fn new(vm: &mut BexVm, value: &Value) -> Value {
        let literal = if let Some(value) = value.as_int() {
            baml_type::Literal::Int(value)
        } else if let Some(value) = value.as_bool() {
            baml_type::Literal::Bool(value)
        } else if let Ok(value) = vm.as_string(value) {
            baml_type::Literal::String(value.to_string())
        } else {
            unreachable!("literal.new argument checked by native glue")
        };
        alloc_runtime_composite(
            vm,
            baml_type::RealizedTy::Literal(
                literal,
                baml_type::Freshness::Regular,
                baml_type::TyAttr::default(),
            ),
            DynTypeDefs::default(),
        )
    }
}
impl BamlNamespaceReflectMap for PackageBamlImpl {
    fn new(vm: &mut BexVm, key: &Value, value: &Value) -> Value {
        let key = reflected_type_value(vm, *key);
        let value = reflected_type_value(vm, *value);
        let mut defs = key.defs().clone();
        defs.merge_from(value.defs());
        alloc_runtime_composite(
            vm,
            baml_type::RealizedTy::Map {
                key: Box::new(key.ty),
                value: Box::new(value.ty),
                attr: baml_type::TyAttr::default(),
            },
            defs,
        )
    }
}
impl BamlNamespaceReflectPrimitive for PackageBamlImpl {}
impl BamlNamespaceReflectUnion for PackageBamlImpl {
    fn new(vm: &mut BexVm, types: &[Value]) -> Result<Value, crate::errors::VmRustFnError> {
        if types.is_empty() {
            let diagnostic = compiler_diagnostic(
                DiagnosticId::RuntimeEmptyUnion,
                "a runtime union must contain at least one member".to_string(),
            );
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let mut members = Vec::with_capacity(types.len());
        let mut defs = DynTypeDefs::default();
        for value in types {
            let type_value = reflected_type_value(vm, *value);
            members.push(type_value.ty.clone());
            defs.merge_from(type_value.defs());
        }
        Ok(alloc_runtime_composite(
            vm,
            baml_type::RealizedTy::Union(members, baml_type::TyAttr::default()),
            defs,
        ))
    }
}

fn reflected_ty(vm: &BexVm, value: Value) -> baml_type::RealizedTy {
    super::type_class::type_value_ty(vm, value)
        .unwrap_or_else(|| unreachable!("kind method receiver must be Object::Type"))
}

fn reflected_type_value(vm: &BexVm, value: Value) -> TypeValue {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("type argument must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("type argument must be Object::Type")
    };
    (**type_value).clone()
}

fn alloc_runtime_composite(vm: &mut BexVm, ty: baml_type::RealizedTy, defs: DynTypeDefs) -> Value {
    let mint = vm.tlab.heap().mint_runtime_id();
    Value::object(
        vm.tlab
            .alloc_type(TypeValue::from_parts_with_defs(ty, mint, defs)),
    )
}

pub(super) struct ReflectedTypeRow {
    pub(super) type_value: TypeValue,
    pub(super) alias: Option<String>,
    pub(super) description: Option<String>,
    pub(super) docstring: Option<String>,
    pub(super) other: IndexMap<String, String>,
}

pub(super) fn reflected_type_row(vm: &BexVm, value: Value) -> Result<ReflectedTypeRow, String> {
    let Some(ptr) = value.as_object_ptr() else {
        return Err("class fields must be type values or reflect.WithMeta<type> rows".into());
    };
    match vm.get_object(ptr) {
        Object::Type(type_value) => Ok(ReflectedTypeRow {
            type_value: (**type_value).clone(),
            alias: None,
            description: None,
            docstring: None,
            other: IndexMap::new(),
        }),
        Object::Instance(instance) => {
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if class.name.to_string() != "baml.reflect.WithMeta" {
                return Err(
                    "class fields must be type values or reflect.WithMeta<type> rows".into(),
                );
            }
            let type_value = reflected_type_value(vm, instance.load_field(0));
            let optional_string = |index| {
                let value = instance.load_field(index);
                if value.is_null() {
                    Ok(None)
                } else {
                    vm.as_string(&value)
                        .map(|value| Some(value.to_string()))
                        .map_err(|_| {
                            "reflect.WithMeta string field has an invalid value".to_string()
                        })
                }
            };
            let other = vm
                .as_map(&instance.load_field(4))
                .map_err(|_| "reflect.WithMeta.other must be map<string, string>".to_string())?
                .to_index_map()
                .iter()
                .map(|(key, value)| {
                    vm.as_string(value)
                        .map(|value| (key.to_string(), value.to_string()))
                        .map_err(|_| {
                            "reflect.WithMeta.other must be map<string, string>".to_string()
                        })
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            Ok(ReflectedTypeRow {
                type_value,
                alias: optional_string(1)?,
                description: optional_string(2)?,
                docstring: optional_string(3)?,
                other,
            })
        }
        _ => Err("class fields must be type values or reflect.WithMeta<type> rows".into()),
    }
}

fn reflected_class(vm: &BexVm, value: Value) -> (bex_vm_types::Class, Vec<baml_type::RealizedTy>) {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("class.Type receiver must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("class.Type receiver must be Object::Type")
    };
    let baml_type::RealizedTy::Class(name, args, _) = &type_value.ty else {
        unreachable!("class.Type receiver must wrap a class type")
    };
    let ptr = type_value
        .defs()
        .classes
        .get(name)
        .copied()
        .or_else(|| vm.lookup_type(name))
        .unwrap_or_else(|| unreachable!("reflected class {name} must be loaded"));
    let Object::Class(class) = vm.get_object(ptr) else {
        unreachable!("reflected class name resolved to a non-class")
    };
    ((**class).clone(), args.clone())
}

fn reflected_enum(vm: &BexVm, value: Value) -> bex_vm_types::Enum {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("enum.Type receiver must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("enum.Type receiver must be Object::Type")
    };
    let baml_type::RealizedTy::Enum(name, _) = &type_value.ty else {
        unreachable!("enum.Type receiver must wrap an enum type")
    };
    let ptr = type_value
        .defs()
        .enums
        .get(name)
        .copied()
        .or_else(|| vm.lookup_type(name))
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

fn string_map(
    vm: &mut BexVm,
    values: &IndexMap<bex_str::BexStr, Value>,
) -> IndexMap<String, String> {
    values
        .iter()
        .map(|(key, value)| {
            let value = vm
                .as_string(value)
                .expect("map<string, string> value checked by native glue")
                .to_string();
            (key.to_string(), value)
        })
        .collect()
}

pub(super) fn is_baml_identifier(value: &str) -> bool {
    fn segment(value: &str, allow_hyphen: bool) -> bool {
        let mut chars = value.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || (allow_hyphen && c == '-'))
    }

    if let Some(rest) = value.strip_prefix('$') {
        return segment(rest, false);
    }
    let mut segments = value.split('$');
    segments.next().is_some_and(|head| segment(head, true))
        && segments.all(|part| segment(part, true))
}

pub(super) fn compiler_diagnostic(id: DiagnosticId, message: String) -> Diagnostic {
    Diagnostic::error(id, message).with_phase(DiagnosticPhase::Hir)
}

pub(super) fn alloc_compilation_error(vm: &mut BexVm, diagnostics: &[Diagnostic]) -> Value {
    alloc_compilation_error_with_span(vm, diagnostics, None)
}

pub(super) fn alloc_compilation_error_at_current_call(
    vm: &mut BexVm,
    diagnostics: &[Diagnostic],
) -> Value {
    let span = vm.current_source_span();
    alloc_compilation_error_with_span(vm, diagnostics, span.as_ref())
}

fn alloc_compilation_error_with_span(
    vm: &mut BexVm,
    diagnostics: &[Diagnostic],
    span: Option<&(String, u32, u32)>,
) -> Value {
    let values = diagnostics
        .iter()
        .map(|diagnostic| {
            let code = Value::object(vm.alloc_string(diagnostic.code()));
            let message = Value::object(vm.alloc_string(diagnostic.message.as_str()));
            let span = span.map_or(Value::NULL, |(file, start, end)| {
                let file = Value::object(vm.alloc_string(file.as_str()));
                copy::reflect::Span {
                    file,
                    start: i64::from(*start),
                    end: i64::from(*end),
                }
                .to_value(vm)
            });
            copy::reflect::Diagnostic {
                code,
                span,
                message,
            }
            .to_value(vm)
        })
        .collect();
    let diagnostic_ty = baml_type::RealizedTy::Class(
        baml_type::QualifiedTypeName::from_dotted_path("baml.reflect.Diagnostic"),
        vec![],
        baml_type::TyAttr::default(),
    );
    let diagnostics = Value::object(vm.alloc_array(diagnostic_ty, values));
    let class = vm.resolve_class("baml.reflect.errors.CompilationError");
    Value::object(vm.alloc_instance(class, vec![diagnostics]))
}

fn enum_row(vm: &BexVm, value: Value) -> Result<EnumVariant, String> {
    let Some(ptr) = value.as_object_ptr() else {
        return Err("reflect.enum.new values must be strings or reflect.enum.Value rows".into());
    };
    match vm.get_object(ptr) {
        Object::String(name) => Ok(EnumVariant {
            name: name.to_string(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip: false,
        }),
        Object::Instance(instance) => {
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if class.name.to_string() != "baml.reflect.enum.Value" {
                return Err(
                    "reflect.enum.new values must be strings or reflect.enum.Value rows".into(),
                );
            }
            let name = vm
                .as_string(&instance.load_field(0))
                .map_err(|_| "reflect.enum.Value.name must be a string")?
                .to_string();
            let meta_value = instance.load_field(1);
            let Some(meta_ptr) = meta_value.as_object_ptr() else {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            };
            let Object::Instance(meta) = vm.get_object(meta_ptr) else {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            };
            let Object::Class(meta_class) = vm.get_object(meta.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if meta_class.name.to_string() != "baml.reflect.Meta" {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            }
            let optional_string = |index| {
                let value = meta.load_field(index);
                if value.is_null() {
                    Ok(None)
                } else {
                    vm.as_string(&value)
                        .map(|value| Some(value.to_string()))
                        .map_err(|_| "reflect.Meta string field has an invalid value".to_string())
                }
            };
            let other = vm
                .as_map(&meta.load_field(3))
                .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())?
                .to_index_map()
                .iter()
                .map(|(key, value)| {
                    vm.as_string(value)
                        .map(|value| (key.to_string(), value.to_string()))
                        .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            Ok(EnumVariant {
                name,
                alias: optional_string(0)?,
                description: optional_string(1)?,
                docstring: optional_string(2)?,
                other,
                skip: false,
            })
        }
        _ => Err("reflect.enum.new values must be strings or reflect.enum.Value rows".into()),
    }
}

fn invalid_enum_value(vm: &BexVm, value: Value) -> crate::errors::VmRustFnError {
    crate::errors::VmRustFnError::BamlError(crate::errors::VmBamlError::InvalidArgument {
        message: format!(
            "reflect.enum.get_value expects an enum value, got {}",
            vm.type_of(&value)
        ),
    })
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
                let r#type = if let Some(type_value) = &field.runtime_type {
                    Value::object(vm.tlab.alloc_type(type_value.clone()))
                } else {
                    let ty = field
                        .field_template
                        .substitute(&args, vm)
                        .unwrap_or_else(|err| {
                            unreachable!("emitted class field template must realize: {err}")
                        });
                    Value::object(vm.alloc_static_type(ty))
                };
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
