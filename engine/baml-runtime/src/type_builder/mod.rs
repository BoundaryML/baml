use std::sync::{Arc, Mutex};

use baml_types::{BamlValue, EvaluationContext, FieldType};
use indexmap::IndexMap;
use internal_baml_core::{
    internal_baml_parser_database::ParserDatabase, ir::repr::TypeBuilderEntry,
};

use crate::runtime_context::{PropertyAttributes, RuntimeClassOverride, RuntimeEnumOverride};

type MetaData = Arc<Mutex<IndexMap<String, BamlValue>>>;

trait Meta {
    fn meta(&self) -> MetaData;
}

pub trait WithMeta {
    fn with_meta(&self, key: &str, value: BamlValue) -> &Self;
}

macro_rules! impl_meta {
    ($type:ty) => {
        impl Meta for $type {
            fn meta(&self) -> MetaData {
                self.meta.clone()
            }
        }
    };
}

impl<T> WithMeta for T
where
    T: Meta,
{
    fn with_meta(&self, key: &str, value: BamlValue) -> &T {
        let meta = self.meta();
        let mut meta = meta.lock().unwrap();
        meta.insert(key.to_string(), value);
        self
    }
}

impl<T: Meta> From<&Arc<Mutex<T>>> for PropertyAttributes {
    fn from(value: &Arc<Mutex<T>>) -> Self {
        let value = value.lock().unwrap();
        let meta = value.meta();
        let meta = meta.lock().unwrap();
        let properties = meta.clone();
        let alias = properties.get("alias").cloned();
        let skip = properties.get("skip").and_then(|v| v.as_bool());

        Self {
            alias,
            skip,
            meta: properties,
        }
    }
}

pub struct ClassBuilder {
    properties: Arc<Mutex<IndexMap<String, Arc<Mutex<ClassPropertyBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(ClassBuilder);

pub struct ClassPropertyBuilder {
    r#type: Arc<Mutex<Option<FieldType>>>,
    meta: MetaData,
}
impl_meta!(ClassPropertyBuilder);

impl ClassPropertyBuilder {
    pub fn r#type(&self, r#type: FieldType) -> &Self {
        *self.r#type.lock().unwrap() = Some(r#type);
        self
    }
}

impl Default for ClassBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassBuilder {
    pub fn new() -> Self {
        Self {
            properties: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn property(&self, name: &str) -> Arc<Mutex<ClassPropertyBuilder>> {
        let mut properties = self.properties.lock().unwrap();
        Arc::clone(properties.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(ClassPropertyBuilder {
                r#type: Default::default(),
                meta: Default::default(),
            }))
        }))
    }
}

pub struct EnumBuilder {
    values: Arc<Mutex<IndexMap<String, Arc<Mutex<EnumValueBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(EnumBuilder);

pub struct EnumValueBuilder {
    meta: MetaData,
}
impl_meta!(EnumValueBuilder);

impl Default for EnumBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EnumBuilder {
    pub fn new() -> Self {
        Self {
            values: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn value(&self, name: &str) -> Arc<Mutex<EnumValueBuilder>> {
        let mut values = self.values.lock().unwrap();
        Arc::clone(values.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(EnumValueBuilder {
                meta: Default::default(),
            }))
        }))
    }
}

pub struct TypeAliasBuilder {
    target: Arc<Mutex<Option<FieldType>>>,
    meta: MetaData,
}
impl_meta!(TypeAliasBuilder);

impl TypeAliasBuilder {
    pub fn new() -> Self {
        Self {
            target: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn target(&self, target: FieldType) -> &Self {
        *self.target.lock().unwrap() = Some(target);
        self
    }
}

impl std::fmt::Debug for TypeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Start the debug printout with the struct name
        writeln!(f, "TypeBuilder {{")?;

        // Safely attempt to acquire the lock and print classes
        write!(f, "  classes: ")?;
        match self.classes.lock() {
            Ok(classes) => {
                // We iterate through the keys only to avoid deadlocks and because we might not be able to print the values
                // safely without deep control over locking mechanisms
                let keys: Vec<_> = classes.keys().collect();
                writeln!(f, "{:?},", keys)?
            }
            Err(_) => writeln!(f, "Cannot acquire lock,")?,
        }

        // Safely attempt to acquire the lock and print enums
        write!(f, "  enums: ")?;
        match self.enums.lock() {
            Ok(enums) => {
                // Similarly, print only the keys
                let keys: Vec<_> = enums.keys().collect();
                writeln!(f, "{:?}", keys)?
            }
            Err(_) => writeln!(f, "Cannot acquire lock,")?,
        }

        // Close the struct printout
        write!(f, "}}")
    }
}

#[derive(Clone)]
pub struct TypeBuilder {
    classes: Arc<Mutex<IndexMap<String, Arc<Mutex<ClassBuilder>>>>>,
    enums: Arc<Mutex<IndexMap<String, Arc<Mutex<EnumBuilder>>>>>,
    type_aliases: Arc<Mutex<IndexMap<String, Arc<Mutex<TypeAliasBuilder>>>>>,
    recursive_type_aliases: Arc<Mutex<Vec<IndexMap<String, FieldType>>>>,

    parser_database: ParserDatabase,
}

impl Default for TypeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeBuilder {
    pub fn new() -> Self {
        Self {
            classes: Default::default(),
            enums: Default::default(),
            type_aliases: Default::default(),
            recursive_type_aliases: Default::default(),
            parser_database: Default::default(),
        }
    }

    pub fn class(&self, name: &str) -> Arc<Mutex<ClassBuilder>> {
        Arc::clone(
            self.classes
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(ClassBuilder::new()))),
        )
    }

    pub fn r#enum(&self, name: &str) -> Arc<Mutex<EnumBuilder>> {
        Arc::clone(
            self.enums
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(EnumBuilder::new()))),
        )
    }

    pub fn type_alias(&self, name: &str) -> Arc<Mutex<TypeAliasBuilder>> {
        Arc::clone(
            self.type_aliases
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(TypeAliasBuilder::new()))),
        )
    }

    pub fn recursive_type_aliases(&self) -> Arc<Mutex<Vec<IndexMap<String, FieldType>>>> {
        Arc::clone(&self.recursive_type_aliases)
    }

    pub fn add_entries(&self, entries: &[TypeBuilderEntry]) {
        for entry in entries {
            match entry {
                TypeBuilderEntry::Class(cls) => {
                    let mutex = self.class(&cls.elem.name);
                    let class_builder = mutex.lock().unwrap();
                    for f in &cls.elem.static_fields {
                        class_builder
                            .property(&f.elem.name)
                            .lock()
                            .unwrap()
                            .r#type(f.elem.r#type.elem.to_owned())
                            .with_meta(
                                "alias",
                                f.attributes.get("alias").map_or(BamlValue::Null, |v| {
                                    v.resolve_string(&EvaluationContext::default())
                                        .map_or(BamlValue::Null, BamlValue::String)
                                }),
                            )
                            .with_meta(
                                "description",
                                f.attributes
                                    .get("description")
                                    .map_or(BamlValue::Null, |v| {
                                        v.resolve_string(&EvaluationContext::default())
                                            .map_or(BamlValue::Null, BamlValue::String)
                                    }),
                            );
                    }
                }

                TypeBuilderEntry::Enum(enm) => {
                    let mutex = self.r#enum(&enm.elem.name);
                    let enum_builder = mutex.lock().unwrap();
                    for (variant, _) in &enm.elem.values {
                        enum_builder
                            .value(&variant.elem.0)
                            .lock()
                            .unwrap()
                            .with_meta(
                                "alias",
                                variant
                                    .attributes
                                    .get("alias")
                                    .map_or(BamlValue::Null, |v| {
                                        v.resolve_string(&EvaluationContext::default())
                                            .map_or(BamlValue::Null, BamlValue::String)
                                    }),
                            )
                            .with_meta(
                                "description",
                                variant.attributes.get("description").map_or(
                                    BamlValue::Null,
                                    |v| {
                                        v.resolve_string(&EvaluationContext::default())
                                            .map_or(BamlValue::Null, BamlValue::String)
                                    },
                                ),
                            )
                            .with_meta(
                                "skip",
                                variant.attributes.get("skip").map_or(BamlValue::Null, |v| {
                                    v.resolve_bool(&EvaluationContext::default())
                                        .map_or(BamlValue::Null, BamlValue::Bool)
                                }),
                            );
                    }
                }

                TypeBuilderEntry::TypeAlias(alias) => {
                    let mutex = self.type_alias(&alias.elem.name);
                    let alias_builder = mutex.lock().unwrap();
                    alias_builder.target(alias.elem.r#type.elem.to_owned());
                }
            }
        }
    }

    /// Internal API of `TypeBuilder::add_baml`.
    ///
    /// Python, TS and Ruby wrappers will call this function when the user runs
    /// `type_builder.add_baml("BAML CODE")`
    pub fn add_baml(&self, baml: &str, rt: &crate::BamlRuntime) -> anyhow::Result<()> {
        use internal_baml_core::{
            internal_baml_diagnostics::{Diagnostics, SourceFile},
            internal_baml_schema_ast::parse_type_builder_contents_from_str,
            ir::repr::IntermediateRepr,
            run_validation_pipeline_on_db, validate_type_builder_entries,
        };

        let path = std::path::PathBuf::from("TypeBuilder::add_baml");
        let source = SourceFile::from((path.clone().into(), baml));

        let mut diagnostics = Diagnostics::new(path);
        diagnostics.set_source(&source);

        let type_builder_entries = parse_type_builder_contents_from_str(baml, &mut diagnostics)?;

        if diagnostics.has_errors() {
            anyhow::bail!("{}", diagnostics.to_pretty_string());
        }

        // TODO: A bunch of mem usage here but at least we drop this one at the
        // end of the function, unlike scoped DBs for type builders.
        let mut scoped_db = rt.inner.db.clone();

        let local_ast =
            validate_type_builder_entries(&mut diagnostics, &scoped_db, &type_builder_entries);
        scoped_db.add_ast(local_ast);

        if let Err(d) = scoped_db.validate(&mut diagnostics) {
            diagnostics.push(d);
            anyhow::bail!("{}", diagnostics.to_pretty_string());
        }

        run_validation_pipeline_on_db(&mut scoped_db, &mut diagnostics);

        if diagnostics.has_errors() {
            anyhow::bail!("{}", diagnostics.to_pretty_string());
        }

        let (classes, enums, type_aliases, recursive_aliases) =
            IntermediateRepr::type_builder_entries_from_scoped_db(&scoped_db, &rt.inner.db)
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;

        self.add_entries(
            &classes
                .into_iter()
                .map(TypeBuilderEntry::Class)
                .chain(enums.into_iter().map(TypeBuilderEntry::Enum))
                .chain(type_aliases.into_iter().map(TypeBuilderEntry::TypeAlias))
                .collect::<Vec<_>>(),
        );

        self.recursive_type_aliases()
            .lock()
            .unwrap()
            .extend(recursive_aliases);

        Ok(())
    }

    pub fn to_overrides(
        &self,
    ) -> (
        IndexMap<String, RuntimeClassOverride>,
        IndexMap<String, RuntimeEnumOverride>,
        IndexMap<String, FieldType>,
        Vec<IndexMap<String, FieldType>>,
    ) {
        log::debug!("Converting types to overrides");
        let cls = self
            .classes
            .lock()
            .unwrap()
            .iter()
            .map(|(name, cls)| {
                log::debug!("Converting class: {}", name);
                let mut overrides = RuntimeClassOverride {
                    alias: None,
                    new_fields: Default::default(),
                    update_fields: Default::default(),
                };

                cls.lock()
                    .unwrap()
                    .properties
                    .lock()
                    .unwrap()
                    .iter()
                    .for_each(|(property_name, f)| {
                        let attrs = PropertyAttributes::from(f);
                        let t = {
                            let property = f.lock().unwrap();
                            let t = property.r#type.lock().unwrap();
                            t.clone()
                        };
                        match t.as_ref() {
                            Some(r#type) => {
                                overrides
                                    .new_fields
                                    .insert(property_name.to_string(), (r#type.clone(), attrs));
                            }
                            None => {
                                overrides
                                    .update_fields
                                    .insert(property_name.to_string(), attrs);
                            }
                        }
                    });
                (name.clone(), overrides)
            })
            .collect();

        let enm = self
            .enums
            .lock()
            .unwrap()
            .iter()
            .map(|(name, enm)| {
                let attributes = PropertyAttributes::from(enm);
                let values = enm
                    .lock()
                    .unwrap()
                    .values
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(value_name, value)| {
                        (value_name.clone(), PropertyAttributes::from(value))
                    })
                    .collect();
                (
                    name.clone(),
                    RuntimeEnumOverride {
                        values,
                        alias: attributes.alias,
                    },
                )
            })
            .collect();

        let aliases = self
            .type_aliases
            .lock()
            .unwrap()
            .iter()
            .map(|(name, builder)| {
                let mutex = builder.lock().unwrap();
                let target = mutex.target.lock().unwrap();
                // TODO: target.unwrap() might not be guaranteed here.
                (name.clone(), target.to_owned().unwrap())
            })
            .collect();

        log::debug!(
            "Dynamic types: \n {:#?} \n Dynamic enums\n {:#?} enums",
            cls,
            enm
        );

        let recursive_aliases = self.recursive_type_aliases.lock().unwrap().clone();

        (cls, enm, aliases, recursive_aliases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_builder() {
        let builder = TypeBuilder::new();
        let cls = builder.class("Person");
        let property = cls.lock().unwrap().property("name");
        property.lock().unwrap().r#type(FieldType::string());
        cls.lock()
            .unwrap()
            .property("age")
            .lock()
            .unwrap()
            .r#type(FieldType::int())
            .with_meta("alias", BamlValue::String("years".to_string()));
    }
}
