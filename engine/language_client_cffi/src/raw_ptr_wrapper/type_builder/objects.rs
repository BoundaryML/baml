use baml_runtime::{
    type_builder::{self, TypeBuilder as _RuntimeTypeBuilder, WithMeta},
    BamlRuntime, IRHelper, InternalRuntimeInterface,
};
use baml_types::{BamlValue, TypeIR};

type RuntimeTypeBuilder = std::sync::Arc<_RuntimeTypeBuilder>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NodeRW {
    // Only view the data (no modifications allowed)
    ReadOnly,
    // View data, but can modify attributes like alias / description
    LLMOnly,
    // Go wild
    ReadWrite,
}

impl NodeRW {
    fn at_least(&self, other: NodeRW) -> anyhow::Result<()> {
        if self < &other {
            anyhow::bail!(
                "Insufficient permissions to perform this operation: {:?} < {:?}",
                self,
                other
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_rw_at_least() {
        assert!(NodeRW::ReadOnly.at_least(NodeRW::ReadOnly).is_ok());
        assert!(NodeRW::ReadOnly.at_least(NodeRW::LLMOnly).is_err());
        assert!(NodeRW::ReadOnly.at_least(NodeRW::ReadWrite).is_err());

        assert!(NodeRW::LLMOnly.at_least(NodeRW::ReadOnly).is_ok());
        assert!(NodeRW::LLMOnly.at_least(NodeRW::LLMOnly).is_ok());
        assert!(NodeRW::LLMOnly.at_least(NodeRW::ReadWrite).is_err());

        assert!(NodeRW::ReadWrite.at_least(NodeRW::ReadOnly).is_ok());
        assert!(NodeRW::ReadWrite.at_least(NodeRW::LLMOnly).is_ok());
        assert!(NodeRW::ReadWrite.at_least(NodeRW::ReadWrite).is_ok());
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeBuilder {
    type_builder: RuntimeTypeBuilder,
}

impl TypeBuilder {
    pub fn add_enum(&self, name: &str, rt: &BamlRuntime) -> anyhow::Result<EnumBuilder> {
        match rt.internal().ir().find_enum(name) {
            Ok(_) => {
                anyhow::bail!("Enum with name {name} already exists");
            }
            Err(_) => {
                let _ = self.type_builder.upsert_enum(name);
                let builder = EnumBuilder::new(self.type_builder.clone(), name.to_string());
                Ok(builder.mode(NodeRW::ReadWrite))
            }
        }
    }

    pub fn add_class(&self, name: &str, rt: &BamlRuntime) -> anyhow::Result<ClassBuilder> {
        match rt.internal().ir().find_class(name) {
            Ok(_) => {
                anyhow::bail!("Class with name {name} already exists");
            }
            Err(_) => {
                let _ = self.type_builder.upsert_class(name);
                let builder = ClassBuilder::new(self.type_builder.clone(), name.to_string());
                Ok(builder.mode(NodeRW::ReadWrite))
            }
        }
    }

    pub fn class(&self, name: &str, rt: &BamlRuntime) -> anyhow::Result<ClassBuilder> {
        match rt.internal().ir().find_class(name) {
            Ok(cls) => {
                let _ = self.type_builder.upsert_class(name);
                let builder = ClassBuilder::new(self.type_builder.clone(), name.to_string());
                if !cls.item.attributes.dynamic() {
                    Ok(builder.mode(NodeRW::ReadOnly))
                } else {
                    Ok(builder.mode(NodeRW::ReadWrite))
                }
            }
            Err(_) => match self.type_builder.maybe_get_class(name) {
                Some(_) => Ok(ClassBuilder::new(
                    self.type_builder.clone(),
                    name.to_string(),
                )),
                None => {
                    anyhow::bail!("Class with name {name} does not exist");
                }
            },
        }
    }

    pub fn r#enum(&self, name: &str, rt: &BamlRuntime) -> anyhow::Result<EnumBuilder> {
        match rt.internal().ir().find_enum(name) {
            Ok(enm) => {
                let _ = self.type_builder.upsert_enum(name);
                let builder = EnumBuilder::new(self.type_builder.clone(), name.to_string());
                if !enm.item.attributes.dynamic() {
                    return Ok(builder.mode(NodeRW::ReadOnly));
                }
                Ok(builder.mode(NodeRW::ReadWrite))
            }
            Err(_) => match self.type_builder.maybe_get_enum(name) {
                Some(_) => Ok(EnumBuilder::new(
                    self.type_builder.clone(),
                    name.to_string(),
                )),
                None => {
                    anyhow::bail!("Enum with name {name} does not exist");
                }
            },
        }
    }

    pub fn add_baml(&self, baml: &str, rt: &BamlRuntime) -> anyhow::Result<()> {
        self.type_builder.add_baml(baml, rt)
    }
}

#[derive(Debug, Clone)]
pub struct ClassBuilder {
    type_builder: RuntimeTypeBuilder,
    class_name: String,
    mode: NodeRW,
}

impl ClassBuilder {
    fn new(type_builder: RuntimeTypeBuilder, class_name: String) -> Self {
        Self {
            type_builder,
            class_name,
            mode: NodeRW::ReadOnly,
        }
    }

    fn mode(self, mode: NodeRW) -> Self {
        Self { mode, ..self }
    }

    fn create_property(&self, name: &str, rt: &BamlRuntime) -> ClassPropertyBuilder {
        let builder = ClassPropertyBuilder::new(
            self.type_builder.clone(),
            self.class_name.clone(),
            name.to_string(),
        );

        let target_mode = match self.mode {
            NodeRW::ReadOnly => NodeRW::ReadOnly,
            NodeRW::LLMOnly => NodeRW::LLMOnly,
            NodeRW::ReadWrite => {
                if let Ok(cls) = rt.internal().ir().find_class(self.class_name.as_str()) {
                    if cls.find_field(name).is_some() {
                        NodeRW::LLMOnly
                    } else {
                        NodeRW::ReadWrite
                    }
                } else {
                    NodeRW::ReadWrite
                }
            }
        };

        builder.mode(target_mode)
    }

    fn cls(
        &self,
        rt: &BamlRuntime,
    ) -> anyhow::Result<std::sync::Arc<std::sync::Mutex<type_builder::ClassBuilder>>> {
        // if the IR defines the class, then its always valid
        if rt
            .internal()
            .ir()
            .find_class(self.class_name.as_str())
            .is_ok()
        {
            let cls = self.type_builder.upsert_class(self.class_name.as_str());
            return Ok(cls);
        }

        let Some(cls) = self.type_builder.maybe_get_class(self.class_name.as_str()) else {
            anyhow::bail!("Class not found: {}", self.class_name);
        };
        Ok(cls)
    }

    pub fn r#type(&self, rt: &BamlRuntime) -> anyhow::Result<TypeIR> {
        self.mode.at_least(NodeRW::ReadOnly)?;
        let _ = self.cls(rt)?;

        Ok(TypeIR::class(self.class_name.as_str()))
    }

    pub fn list_properties(&self, rt: &BamlRuntime) -> anyhow::Result<Vec<ClassPropertyBuilder>> {
        self.mode.at_least(NodeRW::ReadOnly)?;

        let lock = self.cls(rt)?;
        let builder = lock.lock().unwrap();

        let ir_properties = match rt.internal().ir().find_class(self.class_name.as_str()) {
            Ok(ir_cls) => ir_cls
                .item
                .elem
                .static_fields
                .iter()
                .map(|field| field.elem.name.to_string())
                .collect(),
            Err(_) => vec![],
        };

        let dynamic_properties = builder
            .list_properties()
            .into_iter()
            .filter(|name| !ir_properties.contains(name))
            .collect::<Vec<_>>();

        let properties = ir_properties.into_iter().chain(dynamic_properties);
        Ok(properties
            .into_iter()
            .map(|name| self.create_property(name.as_str(), rt))
            .collect())
    }

    pub fn alias(&self, rt: &BamlRuntime, alias: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let cls = self.cls(rt)?;
        let builder = cls.lock().unwrap();
        builder.with_meta("alias", BamlValue::String(alias.to_string()));
        Ok(())
    }

    pub fn description(&self, rt: &BamlRuntime, description: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let cls = self.cls(rt)?;
        let builder = cls.lock().unwrap();
        builder.with_meta("description", BamlValue::String(description.to_string()));
        Ok(())
    }

    pub fn add_property(
        &self,
        rt: &BamlRuntime,
        name: &str,
        field_type: TypeIR,
    ) -> anyhow::Result<ClassPropertyBuilder> {
        self.mode.at_least(NodeRW::ReadWrite)?;
        let cls = self.cls(rt)?;

        // if the IR already has the property, then its not valid to add it again
        if let Ok(cls) = rt.internal().ir().find_class(self.class_name.as_str()) {
            if cls.find_field(name).is_some() {
                anyhow::bail!(
                    "Property already exists: {} in class {}",
                    name,
                    self.class_name
                );
            }
        }

        let builder = cls.lock().unwrap();
        let prop = builder.upsert_property(name);
        prop.lock().unwrap().r#type(field_type);
        Ok(self.create_property(name, rt))
    }

    pub fn property(&self, rt: &BamlRuntime, name: &str) -> anyhow::Result<ClassPropertyBuilder> {
        self.mode.at_least(NodeRW::ReadOnly)?;
        let cls = self.cls(rt)?;

        let builder = cls.lock().unwrap();
        match builder.maybe_get_property(name) {
            Some(_) => Ok(self.create_property(name, rt)),
            None => {
                // if the IR has the property, then its valid to add it again
                if let Ok(cls) = rt.internal().ir().find_class(self.class_name.as_str()) {
                    if cls.find_field(name).is_some() {
                        let _ = builder.upsert_property(name);
                        Ok(self.create_property(name, rt))
                    } else {
                        anyhow::bail!("Property not found: {} in class {}", name, self.class_name)
                    }
                } else {
                    anyhow::bail!("Property not found: {} in class {}", name, self.class_name)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassPropertyBuilder {
    type_builder: RuntimeTypeBuilder,
    class_name: String,
    property_name: String,
    mode: NodeRW,
}

impl ClassPropertyBuilder {
    fn new(type_builder: RuntimeTypeBuilder, class_name: String, property_name: String) -> Self {
        Self {
            type_builder,
            class_name,
            property_name,
            mode: NodeRW::ReadOnly,
        }
    }

    fn mode(self, mode: NodeRW) -> Self {
        Self { mode, ..self }
    }

    fn prop(
        &self,
        rt: &BamlRuntime,
    ) -> anyhow::Result<std::sync::Arc<std::sync::Mutex<type_builder::ClassPropertyBuilder>>> {
        // if the class is defined in the IR, then its always valid
        if let Ok(cls) = rt.internal().ir().find_class(self.class_name.as_str()) {
            if cls.find_field(&self.property_name).is_some() {
                let cls = self.type_builder.upsert_class(self.class_name.as_str());
                let builder = cls.lock().unwrap();
                let prop = builder.upsert_property(&self.property_name);
                return Ok(prop);
            }
        }

        let Some(cls) = self.type_builder.maybe_get_class(self.class_name.as_str()) else {
            return Err(anyhow::anyhow!("Class not found: {}", self.class_name));
        };
        let builder = cls.lock().unwrap();
        match builder.maybe_get_property(&self.property_name) {
            Some(prop) => Ok(prop),
            None => {
                anyhow::bail!(
                    "Property not found: {} in class {}",
                    self.property_name,
                    self.class_name
                )
            }
        }
    }

    pub fn description(&self, rt: &BamlRuntime, description: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let prop = self.prop(rt)?;
        let builder = prop.lock().unwrap();
        builder.with_meta("description", BamlValue::String(description.to_string()));
        Ok(())
    }

    pub fn alias(&self, rt: &BamlRuntime, alias: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let prop = self.prop(rt)?;
        let builder = prop.lock().unwrap();
        builder.with_meta("alias", BamlValue::String(alias.to_string()));
        Ok(())
    }

    pub fn r#type(&self, rt: &BamlRuntime, field_type: TypeIR) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::ReadWrite)?;

        let prop = self.prop(rt)?;
        let builder = prop.lock().unwrap();
        builder.r#type(field_type);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EnumBuilder {
    type_builder: RuntimeTypeBuilder,
    enum_name: String,
    mode: NodeRW,
}

impl EnumBuilder {
    fn new(type_builder: RuntimeTypeBuilder, enum_name: String) -> Self {
        Self {
            type_builder,
            enum_name,
            mode: NodeRW::ReadOnly,
        }
    }

    fn mode(self, mode: NodeRW) -> Self {
        Self { mode, ..self }
    }

    fn enm(
        &self,
        rt: &BamlRuntime,
    ) -> anyhow::Result<std::sync::Arc<std::sync::Mutex<type_builder::EnumBuilder>>> {
        // if the IR defines the enum, then its always valid
        if rt
            .internal()
            .ir()
            .find_enum(self.enum_name.as_str())
            .is_ok()
        {
            return Ok(self.type_builder.upsert_enum(self.enum_name.as_str()));
        }

        let Some(enm) = self.type_builder.maybe_get_enum(self.enum_name.as_str()) else {
            anyhow::bail!("Enum not found: {}", self.enum_name);
        };
        Ok(enm)
    }

    fn create_value(&self, name: &str, rt: &BamlRuntime) -> EnumValueBuilder {
        let target_mode = match self.mode {
            NodeRW::ReadOnly => NodeRW::ReadOnly,
            NodeRW::LLMOnly => NodeRW::LLMOnly,
            NodeRW::ReadWrite => {
                if let Ok(enm) = rt.internal().ir().find_enum(self.enum_name.as_str()) {
                    if enm.find_value(name).is_some() {
                        NodeRW::LLMOnly
                    } else {
                        NodeRW::ReadWrite
                    }
                } else {
                    NodeRW::ReadWrite
                }
            }
        };

        EnumValueBuilder::new(
            self.type_builder.clone(),
            self.enum_name.clone(),
            name.to_string(),
        )
        .mode(target_mode)
    }

    pub fn add_value(&self, rt: &BamlRuntime, value: &str) -> anyhow::Result<EnumValueBuilder> {
        self.mode.at_least(NodeRW::ReadWrite)?;
        let enm = self.enm(rt)?;

        // if the IR already has the value, then its not valid to add it again
        if let Ok(enm_ir) = rt.internal().ir().find_enum(self.enum_name.as_str()) {
            if enm_ir.find_value(value).is_some() {
                anyhow::bail!(
                    "Enum value already exists: {} in enum {}",
                    value,
                    self.enum_name
                );
            }
        }

        let builder = enm.lock().unwrap();
        let _ = builder.upsert_value(value);
        Ok(self.create_value(value, rt))
    }

    pub fn description(&self, rt: &BamlRuntime, description: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let enm = self.enm(rt)?;
        let builder = enm.lock().unwrap();
        builder.with_meta("description", BamlValue::String(description.to_string()));
        Ok(())
    }

    pub fn alias(&self, rt: &BamlRuntime, alias: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let enm = self.enm(rt)?;
        let builder = enm.lock().unwrap();
        builder.with_meta("alias", BamlValue::String(alias.to_string()));
        Ok(())
    }

    pub fn r#type(&self, rt: &BamlRuntime) -> anyhow::Result<TypeIR> {
        self.mode.at_least(NodeRW::ReadOnly)?;
        let _ = self.enm(rt)?;

        Ok(TypeIR::r#enum(self.enum_name.as_str()))
    }

    pub fn list_values(&self, rt: &BamlRuntime) -> anyhow::Result<Vec<EnumValueBuilder>> {
        self.mode.at_least(NodeRW::ReadOnly)?;

        let enm = self.enm(rt)?;

        let ir_values = match rt.internal().ir().find_enum(self.enum_name.as_str()) {
            Ok(ir_enm) => ir_enm
                .item
                .elem
                .values
                .iter()
                .map(|value| value.0.elem.0.to_string())
                .collect(),
            Err(_) => vec![],
        };

        let builder = enm.lock().unwrap();
        let dynamic_values = builder
            .list_values()
            .into_iter()
            .filter(|name| !ir_values.contains(name))
            .collect::<Vec<_>>();

        let values = ir_values.into_iter().chain(dynamic_values);
        Ok(values
            .into_iter()
            .map(|name| self.create_value(name.as_str(), rt))
            .collect())
    }

    pub fn value(&self, rt: &BamlRuntime, name: &str) -> anyhow::Result<EnumValueBuilder> {
        self.mode.at_least(NodeRW::ReadOnly)?;
        let enm = self.enm(rt)?;

        let builder = enm.lock().unwrap();
        let values = builder.list_values();
        if values.contains(&name.to_string()) {
            Ok(self.create_value(name, rt))
        } else {
            // if the IR has the value, then its valid to add it again
            if let Ok(enm_ir) = rt.internal().ir().find_enum(self.enum_name.as_str()) {
                if enm_ir.find_value(name).is_some() {
                    let _ = builder.upsert_value(name);
                    Ok(self.create_value(name, rt))
                } else {
                    anyhow::bail!("Enum value not found: {} in enum {}", name, self.enum_name)
                }
            } else {
                anyhow::bail!("Enum value not found: {} in enum {}", name, self.enum_name)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnumValueBuilder {
    type_builder: RuntimeTypeBuilder,
    enum_name: String,
    value_name: String,
    mode: NodeRW,
}

impl EnumValueBuilder {
    fn new(type_builder: RuntimeTypeBuilder, enum_name: String, value_name: String) -> Self {
        Self {
            type_builder,
            enum_name,
            value_name,
            mode: NodeRW::ReadOnly,
        }
    }

    fn mode(self, mode: NodeRW) -> Self {
        Self { mode, ..self }
    }

    fn value(
        &self,
        rt: &BamlRuntime,
    ) -> anyhow::Result<std::sync::Arc<std::sync::Mutex<type_builder::EnumValueBuilder>>> {
        // if the enum is defined in the IR, then its always valid
        if let Ok(enm) = rt.internal().ir().find_enum(self.enum_name.as_str()) {
            if enm.find_value(self.value_name.as_str()).is_some() {
                let enm = self.type_builder.upsert_enum(self.enum_name.as_str());
                let builder = enm.lock().unwrap();
                let value = builder.upsert_value(&self.value_name);
                return Ok(value);
            }
        }

        let Some(enm) = self.type_builder.maybe_get_enum(self.enum_name.as_str()) else {
            return Err(anyhow::anyhow!("Enum not found: {}", self.enum_name));
        };
        let builder = enm.lock().unwrap();
        match builder.maybe_get_value(&self.value_name) {
            Some(value) => Ok(value),
            None => {
                anyhow::bail!(
                    "Enum value not found: {} in enum {}",
                    self.value_name,
                    self.enum_name
                )
            }
        }
    }

    pub fn description(&self, rt: &BamlRuntime, description: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let value = self.value(rt)?;
        let builder = value.lock().unwrap();
        builder.with_meta("description", BamlValue::String(description.to_string()));
        Ok(())
    }

    pub fn alias(&self, rt: &BamlRuntime, alias: &str) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let value = self.value(rt)?;
        let builder = value.lock().unwrap();
        builder.with_meta("alias", BamlValue::String(alias.to_string()));
        Ok(())
    }

    pub fn skip(&self, rt: &BamlRuntime) -> anyhow::Result<()> {
        self.mode.at_least(NodeRW::LLMOnly)?;

        let value = self.value(rt)?;
        let builder = value.lock().unwrap();
        builder.with_meta("skip", BamlValue::Bool(true));
        Ok(())
    }
}
