use std::{
    fmt,
    ops::Deref,
    sync::{Arc, Mutex},
};

use baml_types::{BamlValue, EvaluationContext, TypeIR};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::{
    internal_baml_parser_database::ParserDatabase, ir::repr::TypeBuilderEntry,
};

use crate::{
    runtime_context::{PropertyAttributes, RuntimeClassOverride, RuntimeEnumOverride},
    BamlRuntime,
};

type MetaData = Arc<Mutex<IndexMap<String, BamlValue>>>;

trait Meta {
    fn meta(&self) -> MetaData;
}

pub trait WithMeta {
    fn with_meta(&self, key: &str, value: BamlValue) -> &Self;
    fn get_meta(&self, key: &str) -> Option<BamlValue>;
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

    fn get_meta(&self, key: &str) -> Option<BamlValue> {
        let meta = self.meta();
        let meta = meta.lock().unwrap();
        meta.get(key).cloned()
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
            constraints: Vec::new(),
            streaming_behavior: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassBuilder {
    pub name: String,
    properties: Arc<Mutex<IndexMap<String, Arc<Mutex<ClassPropertyBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(ClassBuilder);

#[derive(Debug, Clone)]
pub struct ClassPropertyBuilder {
    r#type: Arc<Mutex<Option<TypeIR>>>,
    meta: MetaData,
}
impl_meta!(ClassPropertyBuilder);

impl ClassPropertyBuilder {
    pub fn set_type(&self, r#type: TypeIR) -> &Self {
        *self.r#type.lock().unwrap() = Some(r#type);
        self
    }

    pub fn r#type(&self) -> Option<TypeIR> {
        self.r#type.lock().unwrap().clone()
    }
}

impl ClassBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            properties: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    // TODO: Figure out captured lifetime issue and return Iterator.
    // Iterator that holds mutex lock seems tricky.
    pub fn list_properties_key_value(&self) -> Vec<(String, ClassPropertyBuilder)> {
        self.properties
            .lock()
            .unwrap()
            .iter()
            .map(|(name, prop)| (name.clone(), prop.lock().unwrap().deref().to_owned()))
            .collect()
    }

    // TODO: Unify function above and this one (split because of CFFI).
    pub fn list_properties(&self) -> Vec<String> {
        let properties = self.properties.lock().unwrap();
        properties.keys().cloned().collect()
    }

    pub fn maybe_get_property(&self, name: &str) -> Option<Arc<Mutex<ClassPropertyBuilder>>> {
        let properties = self.properties.lock().unwrap();
        properties.get(name).cloned()
    }

    pub fn upsert_property(&self, name: &str) -> Arc<Mutex<ClassPropertyBuilder>> {
        let mut properties = self.properties.lock().unwrap();
        Arc::clone(properties.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(ClassPropertyBuilder {
                r#type: Default::default(),
                meta: Default::default(),
            }))
        }))
    }

    pub fn remove_property(&self, name: &str) {
        let mut properties = self.properties.lock().unwrap();
        properties.shift_remove(name);
    }

    pub fn reset(&self) {
        self.properties.lock().unwrap().clear();
    }
}

#[derive(Debug, Clone)]
pub struct EnumBuilder {
    pub name: String,
    values: Arc<Mutex<IndexMap<String, Arc<Mutex<EnumValueBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(EnumBuilder);

#[derive(Debug, Clone)]
pub struct EnumValueBuilder {
    meta: MetaData,
}
impl_meta!(EnumValueBuilder);

impl EnumBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            values: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn maybe_get_value(&self, name: &str) -> Option<Arc<Mutex<EnumValueBuilder>>> {
        let values = self.values.lock().unwrap();
        values.get(name).cloned()
    }

    pub fn upsert_value(&self, name: &str) -> Arc<Mutex<EnumValueBuilder>> {
        let mut values = self.values.lock().unwrap();
        Arc::clone(values.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(EnumValueBuilder {
                meta: Default::default(),
            }))
        }))
    }

    pub fn list_values(&self) -> Vec<String> {
        let values = self.values.lock().unwrap();
        values.keys().cloned().collect()
    }
}

// displays a class property along with its current state and metadata
// the format shows two key pieces of information:
// 1. the property name as defined in the class
// 2. any metadata attached to the property in parentheses
//
// metadata is shown in key=value format, with values formatted according to their type
// multiple metadata entries are separated by commas for readability
//
// examples of the output format:
//   name string (alias='username', description='full name')
//   - shows a property with both alias and description metadata
//   age unset
//   - shows a property without a defined type or metadata
//   email string (required=true, format='email')
//   - shows a property with multiple metadata values of different types
impl fmt::Display for ClassPropertyBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.meta.lock().unwrap();
        let type_str = self
            .r#type
            .lock()
            .unwrap()
            .as_ref()
            .map_or("(unknown-type)".to_string(), |t| format!("{}", t.clone()));

        write!(f, "{type_str}")?;

        if !meta.is_empty() {
            write!(f, " (")?;
            for (i, (key, value)) in meta.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{key}={value}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

// displays an enum value and its associated metadata
// the format focuses on clarity and completeness:
// 1. the enum value name in uppercase (following enum conventions)
// 2. any metadata in parentheses, showing all attached information
//
// metadata is displayed in a consistent key=value format:
// - each piece of metadata is separated by commas
// - values are formatted based on their type (quotes for strings, etc.)
// - all metadata is shown, not just common fields like alias
//
// examples of the output format:
//   ACTIVE (alias='active', priority=1, enabled=true)
//   - shows an enum value with multiple metadata types
//   PENDING
//   - shows a simple enum value with no metadata
//   INACTIVE (description='not currently in use', status=null)
//   - shows how null values and longer descriptions are formatted
impl fmt::Display for EnumValueBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.meta.lock().unwrap();

        if !meta.is_empty() {
            write!(f, " (")?;
            for (i, (key, value)) in meta.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{key}={value}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

// displays a complete class definition with all its properties
// the format provides a clear hierarchical structure:
// 1. class name followed by an opening brace
// 2. indented list of properties, each on its own line
// 3. closing brace aligned with the class name
//
// properties are displayed with consistent indentation and formatting:
// - each property starts on a new line with proper indentation
// - properties are separated by commas for valid syntax
// - the last property doesn't have a trailing comma
//
// example of the complete format:
//   User {
//     name string (alias='username', description='user\'s full name'),
//     age int
//     email string
//     status unset
//   }
impl fmt::Display for ClassBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let properties = self.properties.lock().unwrap();
        write!(f, "{{")?;
        if !properties.is_empty() {
            for (i, (name, prop)) in properties.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "\n      {} {}", name, prop.lock().unwrap())?;
            }
            write!(f, "\n    ")?;
        }
        write!(f, "}}")
    }
}

// displays a complete enum definition with all its values
// the format creates a clear and readable structure:
// 1. enum name followed by an opening brace
// 2. indented list of enum values, each on its own line
// 3. closing brace aligned with the enum name
//
// values are displayed with consistent formatting:
// - each value starts on a new line with proper indentation
// - values are separated by commas for valid syntax
// - metadata is shown in parentheses when present
// - empty enums are shown with empty braces
//
// example of the complete format:
//   Status {
//     ACTIVE (alias='active', weight=1.0),
//     PENDING (description='awaiting processing'),
//     INACTIVE (enabled=false),
//     ARCHIVED
//   }
impl fmt::Display for EnumBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values = self.values.lock().unwrap();
        write!(f, "{{")?;
        if !values.is_empty() {
            for (i, (name, value)) in values.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "\n      {}{}", name, value.lock().unwrap())?;
            }
            write!(f, "\n    ")?;
        }
        write!(f, "}}")
    }
}

// displays the complete type builder state in a clear, hierarchical format
// this is the top-level representation that shows all defined types
//
//
// 1. starts with "TypeBuilder(" to identify the structure
// 2. contains two main sections: Classes and Enums
// 3. each section is properly indented and bracketed
// 4. empty sections are omitted for conciseness
//
// the structure maintains consistent formatting:
// - each class and enum starts on a new line
// - proper indentation shows the hierarchy
// - commas separate multiple items
// - empty classes/enums are shown with empty braces
//
// example of the complete format:
// TypeBuilder(
//   Classes: [
//     User {
//       name string (alias='username'),
//       email string (required=true)
//     },
//     Address { }
//   ],
//   Enums: [
//     Status {
//       ACTIVE (alias='active'),
//       PENDING,
//       INACTIVE (enabled=false)
//     }
//   ]
// )
//
// this format makes it easy to:
// - understand the overall structure of defined types
// - see relationships between classes and their properties
// - identify enum values and their metadata
// - spot any missing or incomplete definitions
impl fmt::Display for TypeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let classes = self.classes.lock().unwrap();
        let enums = self.enums.lock().unwrap();
        let type_aliases = self.type_aliases.lock().unwrap();
        let recursive_type_aliases = self.recursive_type_aliases.lock().unwrap();

        write!(f, "TypeBuilder(")?;

        if !classes.is_empty() {
            write!(f, "\n  Classes: [")?;
            for (i, (name, cls)) in classes.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "\n    {} {}", name, cls.lock().unwrap())?;
            }
            write!(f, "\n  ]")?;
        }

        if !type_aliases.is_empty() {
            write!(f, "\n  type_aliases: ")?;
            let keys: Vec<_> = type_aliases.keys().collect();
            writeln!(f, "{keys:?}")?
        }

        if !recursive_type_aliases.is_empty() {
            write!(f, "\n  recursive_type_aliases: ")?;
            let keys: Vec<_> = recursive_type_aliases.iter().map(|v| v.keys()).collect();
            writeln!(f, "{keys:?}")?
        }

        if !enums.is_empty() {
            if !classes.is_empty() {
                write!(f, ",")?;
            }
            write!(f, "\n  Enums: [")?;
            for (i, (name, e)) in enums.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "\n    {} {}", name, e.lock().unwrap())?;
            }
            write!(f, "\n  ]")?;
        }

        write!(f, "\n)")
    }
}

pub struct TypeAliasBuilder {
    target: Arc<Mutex<Option<TypeIR>>>,
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

    pub fn target(&self, target: TypeIR) -> &Self {
        *self.target.lock().unwrap() = Some(target);
        self
    }
}

impl Default for TypeAliasBuilder {
    fn default() -> Self {
        Self::new()
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
                writeln!(f, "{keys:?},")?
            }
            Err(_) => writeln!(f, "Cannot acquire lock,")?,
        }

        // Safely attempt to acquire the lock and print enums
        write!(f, "  enums: ")?;
        match self.enums.lock() {
            Ok(enums) => {
                // Similarly, print only the keys
                let keys: Vec<_> = enums.keys().collect();
                writeln!(f, "{keys:?}")?
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
    recursive_type_aliases: Arc<Mutex<Vec<IndexMap<String, TypeIR>>>>,
    recursive_classes: Arc<Mutex<Vec<IndexSet<String>>>>,
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
            recursive_classes: Default::default(),
        }
    }

    pub fn reset(&self) {
        self.classes.lock().unwrap().clear();
        self.enums.lock().unwrap().clear();
        self.type_aliases.lock().unwrap().clear();
        self.recursive_type_aliases.lock().unwrap().clear();
        self.recursive_classes.lock().unwrap().clear();
    }

    pub fn upsert_class(&self, name: &str) -> Arc<Mutex<ClassBuilder>> {
        Arc::clone(
            self.classes
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(ClassBuilder::new(name.to_string())))),
        )
    }

    pub fn maybe_get_class(&self, name: &str) -> Option<Arc<Mutex<ClassBuilder>>> {
        self.classes.lock().unwrap().get(name).cloned()
    }

    pub fn upsert_enum(&self, name: &str) -> Arc<Mutex<EnumBuilder>> {
        Arc::clone(
            self.enums
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(EnumBuilder::new(name.to_string())))),
        )
    }

    pub fn list_enums(&self) -> Vec<String> {
        self.enums.lock().unwrap().keys().cloned().collect()
    }

    pub fn list_classes(&self) -> Vec<String> {
        self.classes.lock().unwrap().keys().cloned().collect()
    }

    pub fn maybe_get_enum(&self, name: &str) -> Option<Arc<Mutex<EnumBuilder>>> {
        self.enums.lock().unwrap().get(name).cloned()
    }

    pub fn upsert_type_alias(&self, name: &str) -> Arc<Mutex<TypeAliasBuilder>> {
        Arc::clone(
            self.type_aliases
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(TypeAliasBuilder::new()))),
        )
    }

    pub fn maybe_get_type_alias(&self, name: &str) -> Option<Arc<Mutex<TypeAliasBuilder>>> {
        self.type_aliases.lock().unwrap().get(name).cloned()
    }

    pub fn recursive_type_aliases(&self) -> Arc<Mutex<Vec<IndexMap<String, TypeIR>>>> {
        Arc::clone(&self.recursive_type_aliases)
    }

    pub fn recursive_classes(&self) -> Arc<Mutex<Vec<IndexSet<String>>>> {
        Arc::clone(&self.recursive_classes)
    }

    pub fn add_entries(&self, entries: &[TypeBuilderEntry]) {
        for entry in entries {
            match entry {
                TypeBuilderEntry::Class(cls) => {
                    let mutex = self.upsert_class(&cls.elem.name);
                    let class_builder = mutex.lock().unwrap();
                    for f in &cls.elem.static_fields {
                        class_builder
                            .upsert_property(&f.elem.name)
                            .lock()
                            .unwrap()
                            .set_type(f.elem.r#type.elem.to_owned())
                            .with_meta(
                                "alias",
                                f.attributes.alias().map_or(BamlValue::Null, |v| {
                                    v.resolve(&EvaluationContext::default())
                                        .map_or(BamlValue::Null, BamlValue::String)
                                }),
                            )
                            .with_meta(
                                "description",
                                f.attributes.description().map_or(BamlValue::Null, |v| {
                                    v.resolve(&EvaluationContext::default())
                                        .map_or(BamlValue::Null, BamlValue::String)
                                }),
                            );
                    }
                }

                TypeBuilderEntry::Enum(enm) => {
                    let mutex = self.upsert_enum(&enm.elem.name);
                    let enum_builder = mutex.lock().unwrap();
                    for (variant, _) in &enm.elem.values {
                        enum_builder
                            .upsert_value(&variant.elem.0)
                            .lock()
                            .unwrap()
                            .with_meta(
                                "alias",
                                variant.attributes.alias().map_or(BamlValue::Null, |v| {
                                    v.resolve(&EvaluationContext::default())
                                        .map_or(BamlValue::Null, BamlValue::String)
                                }),
                            )
                            .with_meta(
                                "description",
                                variant
                                    .attributes
                                    .description()
                                    .map_or(BamlValue::Null, |v| {
                                        v.resolve(&EvaluationContext::default())
                                            .map_or(BamlValue::Null, BamlValue::String)
                                    }),
                            )
                            .with_meta(
                                "skip",
                                if variant.attributes.skip() {
                                    BamlValue::Bool(true)
                                } else {
                                    BamlValue::Bool(false)
                                },
                            );
                    }
                }

                TypeBuilderEntry::TypeAlias(alias) => {
                    let mutex = self.upsert_type_alias(&alias.elem.name);
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
    pub fn add_baml(&self, baml: &str, rt: &BamlRuntime) -> anyhow::Result<()> {
        use internal_baml_core::{
            internal_baml_ast::parse_type_builder_contents_from_str,
            internal_baml_diagnostics::{Diagnostics, SourceFile},
            ir::repr::IntermediateRepr,
            run_validation_pipeline_on_db, validate_type_builder_entries,
        };

        let path = std::path::PathBuf::from("TypeBuilder::add_baml");
        let source = SourceFile::from((path.clone(), baml));

        let mut diagnostics = Diagnostics::new(path);
        diagnostics.set_source(&source);

        let type_builder_entries = parse_type_builder_contents_from_str(baml, &mut diagnostics)?;

        if diagnostics.has_errors() {
            anyhow::bail!("{}", diagnostics.to_pretty_string());
        }

        // TODO: A bunch of mem usage here but at least we drop this one at the
        // end of the function, unlike scoped DBs for type builders.
        let mut scoped_db = rt.db.clone();

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

        let (classes, enums, type_aliases, recursive_classes, recursive_aliases) =
            IntermediateRepr::type_builder_entries_from_scoped_db(&scoped_db, &rt.db)
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

        self.recursive_classes()
            .lock()
            .unwrap()
            .extend(recursive_classes);

        Ok(())
    }

    pub fn to_overrides(
        &self,
    ) -> (
        IndexMap<String, RuntimeClassOverride>,
        IndexMap<String, RuntimeEnumOverride>,
        IndexMap<String, TypeIR>,
        Vec<IndexSet<String>>,
        Vec<IndexMap<String, TypeIR>>,
    ) {
        log::debug!("Converting types to overrides");
        let cls = self
            .classes
            .lock()
            .unwrap()
            .iter()
            .map(|(name, cls)| {
                log::debug!("Converting class: {name}");
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

        log::debug!("Dynamic types: \n {cls:#?} \n Dynamic enums\n {enm:#?} enums");

        let recursive_aliases = self.recursive_type_aliases.lock().unwrap().clone();
        let recursive_classes = self.recursive_classes.lock().unwrap().clone();

        (cls, enm, aliases, recursive_classes, recursive_aliases)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use internal_baml_core::feature_flags::FeatureFlags;

    use super::*;

    #[test]
    fn test_type_builder() {
        let builder = TypeBuilder::new();

        // Add a class with properties and metadata
        let cls = builder.upsert_class("User");
        {
            let cls = cls.lock().unwrap();
            // Add name property with alias and description
            cls.upsert_property("name")
                .lock()
                .unwrap()
                .set_type(TypeIR::string())
                .with_meta("alias", BamlValue::String("username".to_string()))
                .with_meta(
                    "description",
                    BamlValue::String("The user's full name".to_string()),
                );

            // Add age property with description only
            cls.upsert_property("age")
                .lock()
                .unwrap()
                .set_type(TypeIR::int())
                .with_meta(
                    "description",
                    BamlValue::String("User's age in years".to_string()),
                );

            // Add email property with no metadata
            cls.upsert_property("email")
                .lock()
                .unwrap()
                .set_type(TypeIR::string());
        }

        // Add an enum with values and metadata
        let enm = builder.upsert_enum("Status");
        {
            let enm = enm.lock().unwrap();
            // Add ACTIVE value with alias and description
            enm.upsert_value("ACTIVE")
                .lock()
                .unwrap()
                .with_meta("alias", BamlValue::String("active".to_string()))
                .with_meta(
                    "description",
                    BamlValue::String("User is active".to_string()),
                );

            // Add INACTIVE value with alias only
            enm.upsert_value("INACTIVE")
                .lock()
                .unwrap()
                .with_meta("alias", BamlValue::String("inactive".to_string()));

            // Add PENDING value with no metadata
            enm.upsert_value("PENDING");
        }

        // Convert to string and verify the format
        let output = builder.to_string();
        assert_eq!(
            output,
            r#"TypeBuilder(
  Classes: [
    User {
      name string (alias=String("username"), description=String("The user's full name")),
      age int (description=String("User's age in years")),
      email string
    }
  ],
  Enums: [
    Status {
      ACTIVE (alias=String("active"), description=String("User is active")),
      INACTIVE (alias=String("inactive")),
      PENDING
    }
  ]
)"#
        );
    }

    // this  test is to ensure that the string representation is correct
    // and that the to_overrides method is working as expected

    #[test]
    fn test_type_builder_advanced() {
        let builder = TypeBuilder::new();

        // 1. Complex class with nested types and all field types
        let address = builder.upsert_class("Address");
        {
            let address = address.lock().unwrap();
            // String with all metadata
            address
                .upsert_property("street")
                .lock()
                .unwrap()
                .set_type(TypeIR::string())
                .with_meta("alias", BamlValue::String("streetAddress".to_string()))
                .with_meta(
                    "description",
                    BamlValue::String("Street address including number".to_string()),
                );

            // Optional int with description
            address
                .upsert_property("unit")
                .lock()
                .unwrap()
                .set_type(TypeIR::int().as_optional())
                .with_meta(
                    "description",
                    BamlValue::String("Apartment/unit number if applicable".to_string()),
                );

            // List of strings with alias
            address
                .upsert_property("tags")
                .lock()
                .unwrap()
                .set_type(TypeIR::string().as_list())
                .with_meta("alias", BamlValue::String("labels".to_string()));

            // Boolean with no metadata
            address
                .upsert_property("is_primary")
                .lock()
                .unwrap()
                .set_type(TypeIR::bool());

            // Float with skip metadata
            address
                .upsert_property("coordinates")
                .lock()
                .unwrap()
                .set_type(TypeIR::float())
                .with_meta("skip", BamlValue::Bool(true));
        }

        // 2. Empty class
        builder.upsert_class("EmptyClass");

        // 3. Complex enum with various metadata combinations
        let priority = builder.upsert_enum("Priority");
        {
            let priority = priority.lock().unwrap();
            // All metadata
            priority
                .upsert_value("HIGH")
                .lock()
                .unwrap()
                .with_meta("alias", BamlValue::String("urgent".to_string()))
                .with_meta(
                    "description",
                    BamlValue::String("Needs immediate attention".to_string()),
                )
                .with_meta("skip", BamlValue::Bool(false));

            // Only description
            priority.upsert_value("MEDIUM").lock().unwrap().with_meta(
                "description",
                BamlValue::String("Standard priority".to_string()),
            );

            // Only skip
            priority
                .upsert_value("LOW")
                .lock()
                .unwrap()
                .with_meta("skip", BamlValue::Bool(true));

            // No metadata
            priority.upsert_value("NONE");
        }

        // 4. Empty enum
        builder.upsert_enum("EmptyEnum");

        // Test string representation
        let output = builder.to_string();
        assert_eq!(
            output,
            r#"TypeBuilder(
  Classes: [
    Address {
      street string (alias=String("streetAddress"), description=String("Street address including number")),
      unit (int | null) (description=String("Apartment/unit number if applicable")),
      tags string[] (alias=String("labels")),
      is_primary bool,
      coordinates float (skip=Bool(true))
    },
    EmptyClass {}
  ],
  Enums: [
    Priority {
      HIGH (alias=String("urgent"), description=String("Needs immediate attention"), skip=Bool(false)),
      MEDIUM (description=String("Standard priority")),
      LOW (skip=Bool(true)),
      NONE
    },
    EmptyEnum {}
  ]
)"#
        );

        // Test to_overrides()
        let (classes, enums, aliases, recursive_classes, recursive_aliases) =
            builder.to_overrides();

        // Verify class overrides
        assert_eq!(classes.len(), 2);
        let address_override = classes.get("Address").unwrap();
        assert_eq!(address_override.new_fields.len(), 5); // All fields are new
        assert!(address_override
            .new_fields
            .get("street")
            .unwrap()
            .1
            .alias
            .is_some());
        assert!(address_override
            .new_fields
            .get("coordinates")
            .unwrap()
            .1
            .skip
            .unwrap());

        // Verify enum overrides
        assert_eq!(enums.len(), 2);
        let priority_override = enums.get("Priority").unwrap();
        assert_eq!(priority_override.values.len(), 4);
        assert!(priority_override
            .values
            .get("HIGH")
            .unwrap()
            .alias
            .is_some());
        assert!(priority_override.values.get("LOW").unwrap().skip.unwrap());
    }

    #[test]
    fn test_recursive_property() {
        let builder = TypeBuilder::new();

        // Create a 'Node' class where the 'child' property recursively refers to 'Node'
        let node = builder.upsert_class("Node");
        {
            let node = node.lock().unwrap();
            node.upsert_property("child")
                .lock()
                .unwrap()
                .set_type(TypeIR::class("Node"))
                .with_meta(
                    "description",
                    BamlValue::String("recursive self reference".to_string()),
                );
        }

        // Optionally, print the builder's string representation.
        let output = builder.to_string();
        // println!("{}", output);

        // Verify that the output string contains the recursive property information.
        assert!(
            output.contains(
                r#"TypeBuilder(
  Classes: [
    Node {
      child Node (description=String("recursive self reference"))
    }
  ]
)"#
            ),
            "Output did not contain the expected recursive property format: {output}",
        );

        // Verify via to_overrides() that the recursive field is set correctly.
        let (class_overrides, _enum_overrides, _aliases, _recursive_classes, _recursive_aliases) =
            builder.to_overrides();
        let node_override = class_overrides
            .get("Node")
            .expect("Expected override for Node");
        let (child_field_type, _child_attrs) = node_override
            .new_fields
            .get("child")
            .expect("Expected a 'child' property in Node");

        // The child's field type should exactly be a recursive reference to 'Node'
        assert_eq!(
            child_field_type,
            &TypeIR::class("Node"),
            "The 'child' field is not correctly set as a recursive reference to 'Node'"
        );
    }

    use crate::BamlRuntime;

    #[test]
    fn test_type_builder_recursive_2() -> anyhow::Result<()> {
        let builder = TypeBuilder::new();

        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"

          class Output {
            hello string
            @@dynamic
          }


          client<llm> GPT4Turbo {
            provider baml-openai-chat
            options {
              model gpt-4-1106-preview
              api_key env.OPENAI_API_KEY
            }
          }


          function Extract(input: string) -> Output {
            client GPT4Turbo
            prompt #"

              {{ ctx.output_format }}
            "#
          }

          test Test {
            functions [Extract]
            args {
              input "hi"
            }
          }
          "##,
        );

        let function_name = "Extract";
        let test_name = "Test";

        let runtime = BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
            FeatureFlags::new(),
        )
        .unwrap();

        let baml = r##"

            class Two {
                three string
                dynamicField Output
            }

            dynamic class Output {
                two Node
                three string

            }

            class C {
                hello string
            }
            class D {
                hello string
            }
            class Node {
                child C | D | Node
            }
        "##;

        builder.add_baml(baml, &runtime)?;
        println!("{builder}");
        builder.to_overrides();
        Ok(())
    }
}
