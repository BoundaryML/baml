use std::{collections::BTreeSet, fmt::Write as _};

use baml_sdkgen_types::{
    Class, Enum, Function, FunctionArgument, Name, NamingConvention, SymbolPool, Ty, TypeAlias,
};

use crate::{
    descriptors::descriptor,
    names::{NameTable, callable_parts, constant, identifier, parameter, quote},
    types::{ruby_alias_type, ruby_input_type, ruby_type},
};

pub(crate) struct Emitter<'a> {
    pub(crate) pool: &'a SymbolPool,
    pub(crate) supported: &'a BTreeSet<Name>,
    pub(crate) naming: NamingConvention,
    pub(crate) names: NameTable,
    pub(crate) registry: String,
    pub(crate) shells: String,
    pub(crate) aliases: String,
    pub(crate) skipped: BTreeSet<String>,
}

impl Emitter<'_> {
    pub(crate) fn class(&mut self, name: &Name, class: &Class, body: &mut String) {
        let scope = self.names.qualified(name);
        self.shell(name, "Struct");
        doc_comment(
            body,
            &member_doc(
                class.docstring.as_deref(),
                "Attributes",
                class
                    .properties
                    .iter()
                    .map(|p| (p.name.as_str(), p.docstring.as_deref())),
            ),
        );
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
                ruby_input_type(&property.ty, self.supported, &self.names, self.pool).unwrap()
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
        doc_comment(
            body,
            &member_doc(
                enum_.docstring.as_deref(),
                "Members",
                enum_
                    .variants
                    .iter()
                    .map(|v| (v.name.as_str(), v.docstring.as_deref())),
            ),
        );
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

    pub(crate) fn type_alias(&mut self, name: &Name, alias: &TypeAlias, pool: &SymbolPool) {
        let ty = ruby_alias_type(
            &Ty::TypeAlias(name.clone()),
            pool,
            self.supported,
            &self.names,
        )
        .unwrap();
        writeln!(
            self.aliases,
            "{} = ::T.type_alias {{ {ty} }}",
            self.names.qualified(name)
        )
        .unwrap();
        writeln!(
            self.registry,
            "::Baml::Bridge.define_alias({}, {})",
            quote(&name.to_string()),
            descriptor(&alias.resolves_to)
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
                .any(|a| ruby_type(&a.ty, self.supported, &self.names).is_none())
        {
            self.skipped.insert(wire.to_owned());
            return;
        }
        let result = ruby_type(&function.return_type, self.supported, &self.names).unwrap();
        let argument_types: Vec<_> = function
            .arguments
            .iter()
            .map(|a| ruby_input_type(&a.ty, self.supported, &self.names, self.pool))
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
        let mut keyword_params = Vec::new();
        let mut required = Vec::new();
        let mut keywords = Vec::new();
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
            let optional = argument.default.is_some();
            let ty = argument_type.unwrap();
            let ty = if optional {
                format!("::T.any({ty}, ::Baml::Unset)")
            } else {
                ty
            };
            let signature = format!("{arg}: {ty}");
            if optional {
                keyword_params.push(signature);
            } else {
                params.push(signature);
            }
            descriptors.push(format!(
                "[{}, {}]",
                quote(argument.name.as_str()),
                descriptor(&argument.ty)
            ));
            if optional {
                keywords.push(format!("{arg}: ::Baml::UNSET"));
            } else {
                required.push(arg);
            }
        }
        params.extend(keyword_params);
        params.push("_baml: ::T.nilable(::Baml::Options)".into());
        keywords.push("_baml: nil".into());
        required.extend(keywords);
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
            let mut doc = function.docstring.clone().unwrap_or_default();
            if let Some(throws) = &function.throws {
                let mut names = BTreeSet::new();
                throws_names(throws, &mut names);
                if !names.is_empty() {
                    if !doc.is_empty() {
                        doc.push_str("\n\n");
                    }
                    write!(
                        doc,
                        "Raises:\n    {}",
                        names.into_iter().collect::<Vec<_>>().join(", ")
                    )
                    .unwrap();
                }
            }
            doc_comment(body, &doc);
            writeln!(body, "sig {{ params({}).returns({returns}) }}\ndef {target}{method}({})\n  ::Baml::Bridge.call_async({}, {{{}}}, baml: _baml){}\nend", params.join(", "), required.join(", "), quote(wire), arguments.join(", "), if asynchronous { "" } else { ".wait" }).unwrap();
        }
    }
}

fn doc_comment(body: &mut String, doc: &str) {
    for line in doc.lines() {
        if line.is_empty() {
            body.push_str("#\n");
        } else {
            writeln!(body, "# {line}").unwrap();
        }
    }
}

fn member_doc<'a>(
    summary: Option<&str>,
    section: &str,
    members: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> String {
    let mut doc = summary.unwrap_or_default().to_owned();
    let members: Vec<_> = members.collect();
    if members.iter().any(|(_, doc)| doc.is_some()) {
        if !doc.is_empty() {
            doc.push_str("\n\n");
        }
        write!(doc, "{section}:").unwrap();
        for (name, comment) in members {
            write!(doc, "\n    {name}").unwrap();
            if let Some(comment) = comment {
                let mut lines = comment.lines();
                if let Some(first) = lines.next() {
                    write!(doc, ": {first}").unwrap();
                }
                for line in lines {
                    write!(doc, "\n        {line}").unwrap();
                }
            }
        }
    }
    doc
}

fn throws_names(ty: &Ty, names: &mut BTreeSet<String>) {
    match ty {
        Ty::Union(items) => items.iter().for_each(|t| throws_names(t, names)),
        Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => {
            names.insert(name.name().to_string());
        }
        Ty::Never => {}
        _ => {
            names.insert(ty.to_string());
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
