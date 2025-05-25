use anyhow::Result;
use baml_runtime::client_registry::{ClientProperty, ClientProvider, ClientRegistry};
use baml_types::{BamlMedia, BamlValue, BamlValueWithMeta, HasFieldType, ToUnionName};

#[allow(non_snake_case)]
#[path = "cffi/cffi_generated.rs"]
mod cffi_generated;

use cffi_generated::cffi::*;

use crate::BamlFunctionArguments;

pub fn buffer_to_cffi_value_holder(buffer: &[u8]) -> Result<BamlValue> {
    let root = flatbuffers::root::<CFFIValueHolder>(buffer)?;
    Ok(root.into())
}

pub fn buffer_to_cffi_function_arguments(buffer: &[u8]) -> Result<BamlFunctionArguments> {
    let root = flatbuffers::root::<CFFIValueHolder>(buffer)?;
    Ok(root
        .value_as_cffifunction_arguments()
        .expect("Failed to convert CFFIValueHolder to CFFIFunctionArguments")
        .into())
}

impl From<cffi_generated::cffi::CFFIValueHolder<'_>> for BamlValue {
    fn from(value: cffi_generated::cffi::CFFIValueHolder) -> Self {
        let value_type = value.value_type();
        match value_type {
            CFFIValueUnion::NONE => BamlValue::Null,
            CFFIValueUnion::CFFIValueString => value
                .value_as_cffivalue_string()
                .and_then(|s| s.value().map(|s| BamlValue::String(s.to_string())))
                .expect("Failed to convert CFFIValueString to BamlValue"),
            CFFIValueUnion::CFFIValueInt => value
                .value_as_cffivalue_int()
                .map(|i| BamlValue::Int(i.value()))
                .expect("Failed to convert CFFIValueInt to BamlValue"),
            CFFIValueUnion::CFFIValueFloat => value
                .value_as_cffivalue_float()
                .map(|f| BamlValue::Float(f.value()))
                .expect("Failed to convert CFFIValueFloat to BamlValue"),
            CFFIValueUnion::CFFIValueBool => value
                .value_as_cffivalue_bool()
                .map(|b| BamlValue::Bool(b.value()))
                .expect("Failed to convert CFFIValueBool to BamlValue"),
            CFFIValueUnion::CFFIValueList => value
                .value_as_cffivalue_list()
                .and_then(|l| l.values())
                .map(|v| v.into_iter().map(|v| v.into()))
                .map(|l| BamlValue::List(l.collect()))
                .expect("Failed to convert CFFIValueList to BamlValue"),
            CFFIValueUnion::CFFIValueMap => value
                .value_as_cffivalue_map()
                .and_then(|m| m.entries())
                .map(|v| v.into_iter().map(|v| v.into()).collect())
                .map(|kv| BamlValue::Map(kv))
                .expect("Failed to convert CFFIValueMap to BamlValue"),
            CFFIValueUnion::CFFIValueClass => value
                .value_as_cffivalue_class()
                .expect("Failed to convert CFFIValueClass to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueEnum => value
                .value_as_cffivalue_enum()
                .expect("Failed to convert CFFIValueEnum to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueMedia => value
                .value_as_cffivalue_media()
                .map(|m| BamlValue::Media(m.into()))
                .expect("Failed to convert CFFIValueMedia to BamlValue"),
            CFFIValueUnion::CFFIValueTuple => value
                .value_as_cffivalue_tuple()
                .expect("Failed to convert CFFIValueTuple to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueUnionVariant => value
                .value_as_cffivalue_union_variant()
                .expect("Failed to convert CFFIValueUnionVariant to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueChecked => value
                .value_as_cffivalue_checked()
                .expect("Failed to convert CFFIValueChecked to BamlValue")
                .into(),
            CFFIValueUnion::CFFIValueStreamingState => value
                .value_as_cffivalue_streaming_state()
                .expect("Failed to convert CFFIValueStreamingState to BamlValue")
                .into(),
            CFFIValueUnion::CFFIFunctionArguments => {
                panic!("CFFIFunctionArguments is not supported in BamlValue");
            }
            other => {
                panic!("Unsupported value type: {:?}", other);
            }
        }
    }
}

impl From<CFFIMapEntry<'_>> for (String, BamlValue) {
    fn from(value: CFFIMapEntry) -> Self {
        let key = value
            .key()
            .expect("Failed to have CFFIMapEntry key")
            .to_string();
        let value = value
            .value()
            .expect("Failed to have CFFIMapEntry value")
            .into();
        (key, value)
    }
}

impl From<CFFIEnvVar<'_>> for (String, String) {
    fn from(value: CFFIEnvVar<'_>) -> (String, String) {
        let key = value
            .key()
            .expect("Failed to have CFFIEnvVar key")
            .to_string();
        let value = value
            .value()
            .expect("Failed to have CFFIEnvVar value")
            .to_string();
        (key, value)
    }
}

impl From<CFFIValueClass<'_>> for BamlValue {
    fn from(value: CFFIValueClass) -> Self {
        BamlValue::Class(
            value
                .name()
                .expect("Failed to have CFFIValueClass name")
                .to_string(),
            value
                .fields()
                .expect("Failed to have CFFIValueClass fields")
                .into_iter()
                .map(|v| v.into())
                .collect(),
        )
    }
}

impl From<CFFIValueEnum<'_>> for BamlValue {
    fn from(value: CFFIValueEnum) -> Self {
        BamlValue::Enum(
            value
                .name()
                .expect("Failed to have CFFIValueEnum name")
                .to_string(),
            value
                .value()
                .expect("Failed to have CFFIValueEnum value")
                .to_string(),
        )
    }
}

impl From<CFFIValueMedia<'_>> for BamlMedia {
    fn from(value: CFFIValueMedia<'_>) -> Self {
        let media_type = value
            .media_type()
            .expect("Failed to have CFFIMediaType")
            .into();
        let media_value = value
            .media_value()
            .expect("Failed to have CFFIMediaType media_value");
        let mime_type = media_value.mime_type().map(|s| s.to_string());
        match media_value.content_type() {
            CFFIMediaContentUnion::CFFIMediaContentBase64 => BamlMedia::base64(
                media_type,
                media_value
                    .content_as_cffimedia_content_base_64()
                    .expect("Failed to have CFFIMediaContentBase64")
                    .data()
                    .expect("Failed to have CFFIMediaContentBase64 data")
                    .to_string(),
                mime_type,
            ),
            CFFIMediaContentUnion::CFFIMediaContentFile => BamlMedia::file(
                media_type,
                "./cffi_place_holder.baml".into(),
                media_value
                    .content_as_cffimedia_content_file()
                    .expect("Failed to have CFFIMediaContentFile")
                    .path()
                    .expect("Failed to have CFFIMediaContentFile path")
                    .to_string()
                    .into(),
                mime_type,
            ),
            CFFIMediaContentUnion::CFFIMediaContentUrl => BamlMedia::url(
                media_type,
                media_value
                    .content_as_cffimedia_content_url()
                    .expect("Failed to have CFFIMediaContentUrl")
                    .url()
                    .expect("Failed to have CFFIMediaContentUrl url")
                    .to_string(),
                mime_type,
            ),
            _ => unimplemented!(
                "Unsupported media content type: {:?}",
                media_value.content_type()
            ),
        }
    }
}

impl From<CFFIMediaType<'_>> for baml_types::BamlMediaType {
    fn from(value: CFFIMediaType) -> Self {
        match value.type_() {
            MediaTypeEnum::Image => baml_types::BamlMediaType::Image,
            MediaTypeEnum::Audio => baml_types::BamlMediaType::Audio,
            MediaTypeEnum::Other => unimplemented!("Other media type is not supported"),
            _ => unimplemented!("Unsupported media type: {:?}", value.type_()),
        }
    }
}

impl From<CFFIValueTuple<'_>> for BamlValue {
    fn from(_: CFFIValueTuple) -> Self {
        unimplemented!("BamlValueTuple is not supported");
    }
}

impl From<CFFIValueUnionVariant<'_>> for BamlValue {
    fn from(value: CFFIValueUnionVariant) -> Self {
        value
            .value()
            .expect("Failed to have CFFIValueUnionVariant value")
            .into()
    }
}

impl From<CFFIFunctionArguments<'_>> for BamlFunctionArguments {
    fn from(value: CFFIFunctionArguments) -> Self {
        let kwargs = value
            .kwargs()
            .expect("Failed to have CFFIFunctionArguments kwargs")
            .into_iter()
            .map(|v| v.into())
            .collect();
        let client_registry = value.client_registry().map(|r| r.into());
        let env_vars = value
            .env()
            .map(|e| e.iter().map(|v| v.into()).collect())
            .unwrap_or_default();
        BamlFunctionArguments {
            kwargs,
            client_registry,
            env_vars,
        }
    }
}

impl From<CFFIClientRegistry<'_>> for ClientRegistry {
    fn from(value: CFFIClientRegistry) -> Self {
        let mut client_registry = ClientRegistry::new();
        value
            .primary()
            .map(|s| client_registry.set_primary(s.to_string()));

        value
            .clients()
            .expect("Failed to have CFFIClientRegistry clients")
            .into_iter()
            .map(|v| v.into())
            .for_each(|client| {
                client_registry.add_client(client);
            });

        client_registry
    }
}

impl From<CFFIClientProperty<'_>> for ClientProperty {
    fn from(value: CFFIClientProperty) -> Self {
        let name = value
            .name()
            .expect("Failed to have CFFIClientProperty name")
            .to_string();
        let provider = value
            .provider()
            .expect("Failed to have CFFIClientProperty provider")
            .parse::<ClientProvider>()
            .expect("Failed to parse CFFIClientProperty provider");

        let retry_policy = value.retry_policy().map(|r| r.to_string());
        let options = value
            .options()
            .expect("Failed to have CFFIClientProperty options")
            .into_iter()
            .map(|v| v.into())
            .collect();

        ClientProperty::new(name, provider, retry_policy, options)
    }
}
impl From<CFFIValueChecked<'_>> for BamlValue {
    fn from(_value: CFFIValueChecked) -> Self {
        unimplemented!("CFFIValueChecked is not supported");
    }
}

impl From<CFFIValueStreamingState<'_>> for BamlValue {
    fn from(_value: CFFIValueStreamingState) -> Self {
        unimplemented!("CFFIValueStreamingState is not supported");
    }
}

pub fn serialize_baml_value_with_meta<'a, 'b, T>(
    value: &'b BamlValueWithMeta<T>,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> &'a [u8]
where
    T: HasFieldType,
{
    let value_holder = from_baml_value_with_meta(value, &mut builder);
    // println!("value_holder: {:#?}", value_holder);
    builder.finish(value_holder, None);
    builder.finished_data()
}

fn from_baml_value_with_meta<'a, 'b, T>(
    value: &'b BamlValueWithMeta<T>,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> flatbuffers::WIPOffset<CFFIValueHolder<'b>>
where
    T: HasFieldType,
{
    let (value_type, value_holder) = match value {
        BamlValueWithMeta::String(val, _) => {
            // Create a FlatBuffers string and get its offset.
            let str_offset = builder.create_string(val);

            // Build the CFFIValueString table.
            let value_string = CFFIValueString::create(
                &mut builder,
                &CFFIValueStringArgs {
                    value: Some(str_offset),
                },
            );
            (
                CFFIValueUnion::CFFIValueString,
                value_string.as_union_value(),
            )
        }
        BamlValueWithMeta::Int(val, _) => {
            let value_int = CFFIValueInt::create(&mut builder, &CFFIValueIntArgs { value: *val });

            (CFFIValueUnion::CFFIValueInt, value_int.as_union_value())
        }
        BamlValueWithMeta::Float(val, _) => {
            let value_float =
                CFFIValueFloat::create(&mut builder, &CFFIValueFloatArgs { value: *val });

            (CFFIValueUnion::CFFIValueFloat, value_float.as_union_value())
        }
        BamlValueWithMeta::Bool(val, _) => {
            let value_bool =
                CFFIValueBool::create(&mut builder, &CFFIValueBoolArgs { value: *val });

            (CFFIValueUnion::CFFIValueBool, value_bool.as_union_value())
        }
        BamlValueWithMeta::List(val, _) => {
            let mut items = Vec::new();
            for v in val.iter() {
                items.push(from_baml_value_with_meta(v, &mut builder));
            }

            let values = builder.create_vector_from_iter(items.into_iter());

            let field_type = field_type_to_cffi_value_holder(value.field_type(), &mut builder);

            let value_list = CFFIValueList::create(
                &mut builder,
                &CFFIValueListArgs {
                    field_type: Some(field_type),
                    values: Some(values),
                },
            );

            (CFFIValueUnion::CFFIValueList, value_list.as_union_value())
        }
        BamlValueWithMeta::Map(val, _) => {
            let mut items = Vec::new();
            for (k, v) in val.iter() {
                let key = builder.create_string(k);
                let value = from_baml_value_with_meta(v, &mut builder);

                items.push(CFFIMapEntry::create(
                    &mut builder,
                    &CFFIMapEntryArgs {
                        key: Some(key),
                        value: Some(value),
                    },
                ));
            }

            let entries = builder.create_vector_from_iter(items.into_iter());

            let field_types = field_type_to_cffi_value_holder(value.field_type(), &mut builder);

            let value_map = CFFIValueMap::create(
                &mut builder,
                &CFFIValueMapArgs {
                    field_types: Some(field_types),
                    entries: Some(entries),
                },
            );

            (CFFIValueUnion::CFFIValueMap, value_map.as_union_value())
        }
        BamlValueWithMeta::Class(class_name, fields, _) => {
            let mut items = Vec::new();
            for (k, v) in fields.iter() {
                let key = builder.create_string(k);
                let value = from_baml_value_with_meta(v, &mut builder);
                items.push(CFFIMapEntry::create(
                    &mut builder,
                    &CFFIMapEntryArgs {
                        key: Some(key),
                        value: Some(value),
                    },
                ));
            }

            let entries = builder.create_vector_from_iter(items.into_iter());

            let class_name = builder.create_string(class_name);
            let value_class = CFFIValueClass::create(
                &mut builder,
                &CFFIValueClassArgs {
                    name: Some(class_name),
                    fields: Some(entries),
                    dynamic_fields: None,
                },
            );

            (CFFIValueUnion::CFFIValueClass, value_class.as_union_value())
        }
        BamlValueWithMeta::Enum(enum_name, enum_value, _) => {
            let enum_name = builder.create_string(enum_name);
            let enum_value = builder.create_string(enum_value);
            let value_enum = CFFIValueEnum::create(
                &mut builder,
                &CFFIValueEnumArgs {
                    name: Some(enum_name),
                    value: Some(enum_value),
                    is_dynamic: false,
                },
            );

            (CFFIValueUnion::CFFIValueEnum, value_enum.as_union_value())
        }
        BamlValueWithMeta::Media(val, _) => {
            let media_type = match val.media_type {
                baml_types::BamlMediaType::Image => MediaTypeEnum::Image,
                baml_types::BamlMediaType::Audio => MediaTypeEnum::Audio,
            };
            let mime_type = val.mime_type.as_ref().map(|s| builder.create_string(s));
            let (media_content_type, media_content_value) = match &val.content {
                baml_types::BamlMediaContent::Base64(data) => {
                    let data = builder.create_string(&data.base64);
                    let content_base64 = CFFIMediaContentBase64::create(
                        &mut builder,
                        &CFFIMediaContentBase64Args { data: Some(data) },
                    );
                    (
                        CFFIMediaContentUnion::CFFIMediaContentBase64,
                        content_base64.as_union_value(),
                    )
                }
                baml_types::BamlMediaContent::File(path) => {
                    let path = builder.create_string(
                        path.relpath
                            .to_str()
                            .expect("Failed to convert BamlMediaContentFile path to string"),
                    );
                    let content_file = CFFIMediaContentFile::create(
                        &mut builder,
                        &CFFIMediaContentFileArgs { path: Some(path) },
                    );
                    (
                        CFFIMediaContentUnion::CFFIMediaContentFile,
                        content_file.as_union_value(),
                    )
                }
                baml_types::BamlMediaContent::Url(url) => {
                    let url = builder.create_string(&url.url);
                    let content_url = CFFIMediaContentUrl::create(
                        &mut builder,
                        &CFFIMediaContentUrlArgs { url: Some(url) },
                    );
                    (
                        CFFIMediaContentUnion::CFFIMediaContentUrl,
                        content_url.as_union_value(),
                    )
                }
            };

            let media_value = CFFIMediaValue::create(
                &mut builder,
                &CFFIMediaValueArgs {
                    content_type: media_content_type,
                    content: Some(media_content_value),
                    mime_type,
                },
            );

            let media_type = CFFIMediaType::create(
                &mut builder,
                &CFFIMediaTypeArgs {
                    type_: media_type,
                    other: None,
                },
            );
            let value_media = CFFIValueMedia::create(
                &mut builder,
                &CFFIValueMediaArgs {
                    media_type: Some(media_type),
                    media_value: Some(media_value),
                },
            );

            (CFFIValueUnion::CFFIValueMedia, value_media.as_union_value())
        }
        BamlValueWithMeta::Null(_) => {
            return CFFIValueHolder::create(
                &mut builder,
                &CFFIValueHolderArgs {
                    value_type: CFFIValueUnion::NONE,
                    value: None,
                },
            )
        }
    };

    let value_holder = CFFIValueHolder::create(
        &mut builder,
        &CFFIValueHolderArgs {
            value_type,
            value: Some(value_holder),
        },
    );

    let target_type = value.field_type().simplify();
    if let baml_types::FieldType::Union(options) = &target_type {
        let mut options_vec = vec![];
        for t in options.iter() {
            options_vec.push(field_type_to_cffi_value_holder(t, &mut builder));
        }

        // figure out which index of the options is the target_type
        let real_type = value.real_type();
        let value_type_index = options
            .iter()
            .position(|t| real_type == *t)
            .expect("Failed to find target_type in options");
        let variant_name = options[value_type_index].to_union_name();
        let options = builder.create_vector_from_iter(options_vec.into_iter());

        let name_offset = builder.create_string(&target_type.to_union_name());
        let variant_name_offset = builder.create_string(&variant_name);

        let value_union_variant = CFFIValueUnionVariant::create(
            &mut builder,
            &CFFIValueUnionVariantArgs {
                name: Some(name_offset),
                variant_name: Some(variant_name_offset),
                field_types: Some(options),
                value_type_index: value_type_index as i32,
                value: Some(value_holder),
            },
        );

        let value_holder = CFFIValueHolder::create(
            &mut builder,
            &CFFIValueHolderArgs {
                value_type: CFFIValueUnion::CFFIValueUnionVariant,
                value: Some(value_union_variant.as_union_value()),
            },
        );

        value_holder
    } else {
        value_holder
    }
}

fn field_type_to_cffi_value_holder<'a, 'b>(
    field_type: &'a baml_types::FieldType,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> flatbuffers::WIPOffset<CFFIFieldTypeHolder<'b>>
where
{
    let (field_type_union, field_type_union_value) = match field_type {
        baml_types::FieldType::Primitive(type_value) => {
            return type_value_to_cffi(type_value, builder)
        }
        baml_types::FieldType::Enum(e) => {
            let enum_name = builder.create_string(e);
            let enum_type = CFFIFieldTypeEnum::create(
                &mut builder,
                &CFFIFieldTypeEnumArgs {
                    name: Some(enum_name),
                },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeEnum,
                enum_type.as_union_value(),
            )
        }
        baml_types::FieldType::Literal(literal_value) => {
            let literal_value = literal_value_to_cffi(literal_value, &mut builder);
            let literal_type = CFFIFieldTypeLiteral::create(&mut builder, &literal_value);
            (
                CFFIFieldTypeUnion::CFFIFieldTypeLiteral,
                literal_type.as_union_value(),
            )
        }
        baml_types::FieldType::Class(cls) => {
            let class_name = builder.create_string(cls);
            let class_type = CFFIFieldTypeClass::create(
                &mut builder,
                &CFFIFieldTypeClassArgs {
                    name: Some(class_name),
                },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeClass,
                class_type.as_union_value(),
            )
        }
        baml_types::FieldType::List(field_type) => {
            let list_type = field_type_to_cffi_value_holder(field_type, &mut builder);
            let element_type = CFFIFieldTypeList::create(
                &mut builder,
                &CFFIFieldTypeListArgs {
                    element: Some(list_type),
                },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeList,
                element_type.as_union_value(),
            )
        }
        baml_types::FieldType::Map(key_type, value_type) => {
            let key_type = field_type_to_cffi_value_holder(key_type, &mut builder);
            let value_type = field_type_to_cffi_value_holder(value_type, &mut builder);
            let map_type = CFFIFieldTypeMap::create(
                &mut builder,
                &CFFIFieldTypeMapArgs {
                    key: Some(key_type),
                    value: Some(value_type),
                },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeMap,
                map_type.as_union_value(),
            )
        }
        baml_types::FieldType::Union(field_types) => {
            let mut options_vec = vec![];
            for t in field_types.iter() {
                options_vec.push(field_type_to_cffi_value_holder(t, &mut builder));
            }
            let options = builder.create_vector_from_iter(options_vec.into_iter());
            let value_union_variant = CFFIFieldTypeUnionVariant::create(
                &mut builder,
                &CFFIFieldTypeUnionVariantArgs {
                    options: Some(options),
                },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeUnionVariant,
                value_union_variant.as_union_value(),
            )
        }
        baml_types::FieldType::Optional(field_type) => {
            let field_type = field_type_to_cffi_value_holder(field_type, &mut builder);
            let null_type = field_type_to_cffi_value_holder(
                &baml_types::FieldType::Primitive(baml_types::TypeValue::Null),
                &mut builder,
            );
            let options = builder.create_vector_from_iter(vec![field_type, null_type].into_iter());
            let value_union_variant = CFFIFieldTypeUnionVariant::create(
                &mut builder,
                &CFFIFieldTypeUnionVariantArgs {
                    options: Some(options),
                },
            );

            (
                CFFIFieldTypeUnion::CFFIFieldTypeUnionVariant,
                value_union_variant.as_union_value(),
            )
        }
        baml_types::FieldType::RecursiveTypeAlias(name) => {
            let name = builder.create_string(name);
            let type_alias = CFFIFieldTypeTypeAlias::create(
                &mut builder,
                &CFFIFieldTypeTypeAliasArgs { name: Some(name) },
            );
            (
                CFFIFieldTypeUnion::CFFIFieldTypeTypeAlias,
                type_alias.as_union_value(),
            )
        }
        baml_types::FieldType::Tuple(_field_types) => unimplemented!("Tuple is not supported"),
        baml_types::FieldType::WithMetadata { .. } => {
            unimplemented!("WithMetadata is not supported")
        }
        baml_types::FieldType::Arrow(_) => unimplemented!("Functions are not supported."),
    };

    CFFIFieldTypeHolder::create(
        &mut builder,
        &CFFIFieldTypeHolderArgs {
            type_type: field_type_union,
            type_: Some(field_type_union_value),
        },
    )
}

fn type_value_to_cffi<'a, 'b>(
    type_value: &'a baml_types::TypeValue,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> flatbuffers::WIPOffset<CFFIFieldTypeHolder<'b>> {
    let (field_type_union, field_type_union_value) = match type_value {
        baml_types::TypeValue::String => (
            CFFIFieldTypeUnion::CFFIFieldTypeString,
            CFFIFieldTypeString::create(&mut builder, &CFFIFieldTypeStringArgs {}).as_union_value(),
        ),
        baml_types::TypeValue::Int => (
            CFFIFieldTypeUnion::CFFIFieldTypeInt,
            CFFIFieldTypeInt::create(&mut builder, &CFFIFieldTypeIntArgs {}).as_union_value(),
        ),
        baml_types::TypeValue::Float => (
            CFFIFieldTypeUnion::CFFIFieldTypeFloat,
            CFFIFieldTypeFloat::create(&mut builder, &CFFIFieldTypeFloatArgs {}).as_union_value(),
        ),
        baml_types::TypeValue::Bool => (
            CFFIFieldTypeUnion::CFFIFieldTypeBool,
            CFFIFieldTypeBool::create(&mut builder, &CFFIFieldTypeBoolArgs {}).as_union_value(),
        ),
        baml_types::TypeValue::Null => (
            CFFIFieldTypeUnion::CFFIFieldTypeNull,
            CFFIFieldTypeNull::create(&mut builder, &CFFIFieldTypeNullArgs {}).as_union_value(),
        ),
        baml_types::TypeValue::Media(baml_media_type) => {
            let media_type = media_type_to_cffi(baml_media_type, &mut builder);
            (
                CFFIFieldTypeUnion::CFFIFieldTypeMedia,
                CFFIFieldTypeMedia::create(&mut builder, &media_type).as_union_value(),
            )
        }
    };

    CFFIFieldTypeHolder::create(
        &mut builder,
        &CFFIFieldTypeHolderArgs {
            type_type: field_type_union,
            type_: Some(field_type_union_value),
        },
    )
}

fn media_type_to_cffi<'a, 'b>(
    media_type: &'a baml_types::BamlMediaType,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> CFFIFieldTypeMediaArgs<'b> {
    match media_type {
        baml_types::BamlMediaType::Image => CFFIFieldTypeMediaArgs {
            media: Some(CFFIMediaType::create(
                &mut builder,
                &CFFIMediaTypeArgs {
                    type_: MediaTypeEnum::Image,
                    other: None,
                },
            )),
        },
        baml_types::BamlMediaType::Audio => CFFIFieldTypeMediaArgs {
            media: Some(CFFIMediaType::create(
                &mut builder,
                &CFFIMediaTypeArgs {
                    type_: MediaTypeEnum::Audio,
                    other: None,
                },
            )),
        },
    }
}
fn literal_value_to_cffi<'a, 'b>(
    literal_value: &'a baml_types::LiteralValue,
    mut builder: &'a mut flatbuffers::FlatBufferBuilder<'b>,
) -> CFFIFieldTypeLiteralArgs {
    match literal_value {
        baml_types::LiteralValue::String(s) => {
            let string = builder.create_string(s);
            CFFIFieldTypeLiteralArgs {
                literal_type: CFFILiteralUnion::CFFILiteralString,
                literal: Some(string.as_union_value()),
            }
        }
        baml_types::LiteralValue::Int(v) => {
            let int = CFFILiteralInt::create(&mut builder, &CFFILiteralIntArgs { value: *v });
            CFFIFieldTypeLiteralArgs {
                literal_type: CFFILiteralUnion::CFFILiteralInt,
                literal: Some(int.as_union_value()),
            }
        }
        baml_types::LiteralValue::Bool(v) => {
            let bool = CFFILiteralBool::create(&mut builder, &CFFILiteralBoolArgs { value: *v });
            CFFIFieldTypeLiteralArgs {
                literal_type: CFFILiteralUnion::CFFILiteralBool,
                literal: Some(bool.as_union_value()),
            }
        }
    }
}
