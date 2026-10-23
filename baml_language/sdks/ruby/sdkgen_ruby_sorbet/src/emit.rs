use std::{collections::BTreeSet, fmt::Write as _};

use baml_sdkgen_types::{Class, Enum, Function, FunctionArgument, Name, NamingConvention};

use crate::{
    descriptors::descriptor,
    names::{NameTable, callable_parts, constant, identifier, parameter, quote},
    types::ruby_type,
};

pub(crate) struct Emitter<'a> {
    pub(crate) supported: &'a BTreeSet<Name>,
    pub(crate) naming: NamingConvention,
    pub(crate) names: NameTable,
    pub(crate) registry: String,
    pub(crate) shells: String,
    pub(crate) skipped: BTreeSet<String>,
}

impl Emitter<'_> {
    pub(crate) fn class(&mut self, name: &Name, class: &Class, body: &mut String) {
        let scope = self.names.qualified(name);
        self.shell(name, "Struct");
        writeln!(
            body,
            "class {} < ::T::Struct\n  extend ::T::Sig\n  include ::Baml::ValueEquality",
            scope.rsplit("::").next().unwrap()
        )
        .unwrap();
        let mut fields = Vec::new();
        for property in &class.properties {
            let field = self.names.claim(
                &scope,
                &identifier(property.name.as_str(), self.naming),
                &format!("field {name}.{}", property.name),
            );
            writeln!(
                body,
                "  const :{field}, {}",
                ruby_type(&property.ty, self.supported, &self.names).unwrap()
            )
            .unwrap();
            fields.push(format!(
                "{} => [:{field}, {}]",
                quote(property.name.as_str()),
                descriptor(&property.ty)
            ));
        }
        writeln!(
            self.registry,
            "::Baml::Bridge.define_class({}, ::{scope}, fields: {{{}}})",
            quote(&name.to_string()),
            fields.join(", ")
        )
        .unwrap();
        for (instance, methods) in [
            (false, &class.static_methods),
            (true, &class.instance_methods),
        ] {
            for method in methods {
                let mut source = String::new();
                self.function(
                    &scope,
                    &format!("{name}.{}", method.name),
                    method,
                    instance.then_some(name),
                    &mut source,
                );
                for line in source.lines() {
                    writeln!(body, "  {line}").unwrap();
                }
            }
        }
        body.push_str("end\n");
    }

    pub(crate) fn enum_(&mut self, name: &Name, enum_: &Enum, body: &mut String) {
        self.shell(name, "Enum");
        let scope = self.names.qualified(name);
        writeln!(
            body,
            "class {} < ::T::Enum\n  enums do",
            scope.rsplit("::").next().unwrap()
        )
        .unwrap();
        for variant in &enum_.variants {
            let ruby = self.names.claim(
                &scope,
                &constant(variant.name.as_str()),
                &format!("{name}.{}", variant.name),
            );
            writeln!(body, "    {ruby} = new({})", quote(&variant.value)).unwrap();
        }
        body.push_str("  end\nend\n");
        writeln!(
            self.registry,
            "::Baml::Bridge.define_enum({}, ::{})",
            quote(&name.to_string()),
            scope
        )
        .unwrap();
    }

    fn shell(&mut self, name: &Name, kind: &str) {
        writeln!(
            self.shells,
            "{}.const_set(:{}, Class.new(::T::{kind}))",
            self.names.module_name(name),
            self.names.qualified(name).rsplit("::").next().unwrap()
        )
        .unwrap();
    }

    pub(crate) fn function(
        &mut self,
        scope: &str,
        wire: &str,
        function: &Function,
        receiver: Option<&Name>,
        body: &mut String,
    ) {
        if wire.contains('$')
            || !function.generic_params.is_empty()
            || !function.watchers.is_empty()
            || ruby_type(&function.return_type, self.supported, &self.names).is_none()
            || function
                .arguments
                .iter()
                .filter(|a| !omit_injected_argument(a, self.supported, &self.names))
                .any(|a| {
                    a.default.is_some() || ruby_type(&a.ty, self.supported, &self.names).is_none()
                })
        {
            self.skipped.insert(wire.to_owned());
            return;
        }
        let result = ruby_type(&function.return_type, self.supported, &self.names).unwrap();
        let argument_types: Vec<_> = function
            .arguments
            .iter()
            .map(|a| ruby_type(&a.ty, self.supported, &self.names))
            .collect();
        let (base, suffix) = callable_parts(function.name.as_str());
        let base = identifier(base, self.naming);
        // Instance and singleton methods have separate Ruby scopes. Fields share
        // the instance scope, so an emitted method cannot overwrite a reader.
        let name_scope = if receiver.is_some() {
            scope.to_owned()
        } else {
            format!("{scope}.self")
        };
        let base = self.names.callable(&name_scope, &base, wire);
        let method = format!("{base}{suffix}");
        let mut params = Vec::new();
        let mut required = Vec::new();
        let mut arguments = Vec::new();
        let mut descriptors = Vec::new();
        let mut used = BTreeSet::from(["_baml".into(), "_types".into()]);
        if let Some(class) = receiver {
            arguments.push("\"self\" => self".into());
            descriptors.push(format!(
                "[\"self\", {}]",
                descriptor(&baml_sdkgen_types::Ty::Class(class.clone(), Box::new([])))
            ));
        }
        for (argument, argument_type) in function
            .arguments
            .iter()
            .zip(argument_types)
            .filter(|(a, _)| !omit_injected_argument(a, self.supported, &self.names))
        {
            let arg = parameter(argument.name.as_str(), self.naming, &mut used);
            arguments.push(format!("{} => {arg}", quote(argument.name.as_str())));
            let ty = argument_type.unwrap();
            params.push(format!("{arg}: {ty}"));
            descriptors.push(format!(
                "[{}, {}]",
                quote(argument.name.as_str()),
                descriptor(&argument.ty)
            ));
            required.push(arg);
        }
        writeln!(
            self.registry,
            "::Baml::Bridge.define_function({}, params: [{}])",
            quote(wire),
            descriptors.join(", ")
        )
        .unwrap();
        for asynchronous in [false, true] {
            let method = format!("{method}{}", if asynchronous { "_async" } else { "" });
            let returns = if asynchronous {
                format!("::Baml::Call[{result}]")
            } else {
                result.clone()
            };
            let target = if receiver.is_some() { "" } else { "self." };
            let signature = if params.is_empty() {
                String::new()
            } else {
                format!("params({}).", params.join(", "))
            };
            writeln!(body, "sig {{ {signature}returns({returns}) }}\ndef {target}{method}({})\n  ::Baml::Bridge.call_async({}, {{{}}}){}\nend", required.join(", "), quote(wire), arguments.join(", "), if asynchronous { "" } else { ".wait" }).unwrap();
        }
    }
}

// Leave unsupported compiler-injected options to the engine's default, without
// relaxing the supported-type requirement for user-declared arguments.
fn omit_injected_argument(
    argument: &FunctionArgument,
    supported: &BTreeSet<Name>,
    names: &NameTable,
) -> bool {
    argument.injected
        && argument.default.is_some()
        && ruby_type(&argument.ty, supported, names).is_none()
}
