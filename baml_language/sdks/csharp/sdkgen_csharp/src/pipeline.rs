//! Enforced allocation → routing → rendering → transaction preparation.

use std::{collections::BTreeSet, fmt};

use baml_codegen_types::Symbol;

use crate::{
    model::CodegenModel,
    names::CSharpNames,
    output::{GeneratedFile, GeneratedTree, GenerationMetadata},
    routing::{CSharpFileRoute, CSharpFileRoutes, FileRouteRequest},
};

pub const PROGRAM_BYTES_PLACEHOLDER: &str = "{{BAML_PROGRAM_BYTES}}";
pub const PROGRAM_FINGERPRINT_PLACEHOLDER: &str = "{{BAML_PROGRAM_FINGERPRINT}}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutedSource {
    route: FileRouteRequest,
    contents: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramCarrierTemplate {
    route: FileRouteRequest,
    template: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationInput {
    schema_version: u32,
    cli_version: String,
    required_bridge_version: String,
    program_identity: String,
    program_bytes: Vec<u8>,
    program_carrier: ProgramCarrierTemplate,
    sources: Vec<RoutedSource>,
}

impl GenerationInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        schema_version: u32,
        cli_version: impl Into<String>,
        required_bridge_version: impl Into<String>,
        program_identity: impl Into<String>,
        program_bytes: impl Into<Vec<u8>>,
        program_carrier: ProgramCarrierTemplate,
        sources: Vec<RoutedSource>,
    ) -> Self {
        Self {
            schema_version,
            cli_version: cli_version.into(),
            required_bridge_version: required_bridge_version.into(),
            program_identity: program_identity.into(),
            program_bytes: program_bytes.into(),
            program_carrier,
            sources,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparationError {
    NoAllocatedNames,
    RouteUsesForeignName(Box<FileRouteRequest>),
    DuplicateRenderedRoute(Box<FileRouteRequest>),
    MissingAllocatedRoute(Box<FileRouteRequest>),
    UnrenderedAllocatedRoute(Box<FileRouteRequest>),
    InvalidProgramCarrierTemplate(&'static str),
}

impl fmt::Display for PreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAllocatedNames => f.write_str("C# names must be allocated before rendering"),
            Self::RouteUsesForeignName(route) => {
                write!(
                    f,
                    "file route uses a name from another allocation set: {route:?}"
                )
            }
            Self::DuplicateRenderedRoute(route) => {
                write!(f, "route rendered more than once: {route:?}")
            }
            Self::MissingAllocatedRoute(route) => {
                write!(f, "rendered source has no allocated route: {route:?}")
            }
            Self::UnrenderedAllocatedRoute(route) => {
                write!(f, "allocated route was not rendered: {route:?}")
            }
            Self::InvalidProgramCarrierTemplate(reason) => {
                write!(f, "invalid program-carrier template: {reason}")
            }
        }
    }
}

impl std::error::Error for PreparationError {}

#[derive(Clone, Copy, Debug)]
pub struct CSharpRenderContext<'a> {
    names: &'a CSharpNames,
    route: &'a CSharpFileRoute,
}

impl<'a> CSharpRenderContext<'a> {
    #[must_use]
    pub fn names(&self) -> &'a CSharpNames {
        self.names
    }

    #[must_use]
    pub fn route(&self) -> &'a CSharpFileRoute {
        self.route
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CSharpRenderPlan<'a> {
    model: &'a CodegenModel,
    names: &'a CSharpNames,
    routes: &'a CSharpFileRoutes,
}

impl<'a> CSharpRenderPlan<'a> {
    pub(crate) fn new(
        model: &'a CodegenModel,
        names: &'a CSharpNames,
        routes: &'a CSharpFileRoutes,
    ) -> Result<Self, PreparationError> {
        if names.is_empty() {
            return Err(PreparationError::NoAllocatedNames);
        }
        for (request, _) in routes.iter() {
            if request
                .allocated_names()
                .any(|name| !names.contains_allocated(name))
            {
                return Err(PreparationError::RouteUsesForeignName(Box::new(
                    request.clone(),
                )));
            }
        }
        Ok(Self {
            model,
            names,
            routes,
        })
    }

    pub(crate) fn render_source(
        &self,
        request: &FileRouteRequest,
        render: impl FnOnce(CSharpRenderContext<'_>) -> Vec<u8>,
    ) -> Result<RoutedSource, PreparationError> {
        let route = self
            .routes
            .get(request)
            .ok_or_else(|| PreparationError::MissingAllocatedRoute(Box::new(request.clone())))?;
        Ok(RoutedSource {
            route: request.clone(),
            contents: render(CSharpRenderContext {
                names: self.names,
                route,
            }),
        })
    }

    pub(crate) fn program_carrier(
        &self,
        request: &FileRouteRequest,
        template: impl Into<String>,
    ) -> Result<ProgramCarrierTemplate, PreparationError> {
        if self.routes.get(request).is_none() {
            return Err(PreparationError::MissingAllocatedRoute(Box::new(
                request.clone(),
            )));
        }
        Ok(ProgramCarrierTemplate {
            route: request.clone(),
            template: template.into(),
        })
    }

    pub(crate) fn prepare(
        &self,
        input: GenerationInput,
    ) -> Result<GeneratedTree, PreparationError> {
        prepare_generated_tree(self.model, self.routes, input)
    }
}

fn prepare_generated_tree(
    model: &CodegenModel,
    routes: &CSharpFileRoutes,
    input: GenerationInput,
) -> Result<GeneratedTree, PreparationError> {
    let fingerprint = sha256(&input.program_bytes);
    let carrier_contents = render_program_carrier(
        &input.program_carrier.template,
        &input.program_bytes,
        &fingerprint,
    )?;
    let mut rendered = input.sources;
    rendered.push(RoutedSource {
        route: input.program_carrier.route,
        contents: carrier_contents.into_bytes(),
    });

    let mut seen = BTreeSet::new();
    let mut files = Vec::with_capacity(rendered.len());
    for source in rendered {
        if !seen.insert(source.route.clone()) {
            return Err(PreparationError::DuplicateRenderedRoute(Box::new(
                source.route,
            )));
        }
        let route = routes.get(&source.route).ok_or_else(|| {
            PreparationError::MissingAllocatedRoute(Box::new(source.route.clone()))
        })?;
        files.push(GeneratedFile::new(route.relative_path(), source.contents));
    }
    for (request, _) in routes.iter() {
        if !seen.contains(request) {
            return Err(PreparationError::UnrenderedAllocatedRoute(Box::new(
                request.clone(),
            )));
        }
    }

    let mut recursive_aliases = model
        .symbols
        .iter()
        .filter_map(|(name, symbol)| match symbol {
            Symbol::TypeAlias(alias) if alias.recursive && name.package().as_str() == "user" => {
                Some(name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    recursive_aliases.sort();

    Ok(GeneratedTree {
        metadata: GenerationMetadata {
            schema_version: input.schema_version,
            cli_version: input.cli_version,
            required_bridge_version: input.required_bridge_version,
            program_identity: input.program_identity,
            program_fingerprint: fingerprint,
        },
        program_bytes: input.program_bytes,
        files,
        recursive_aliases,
    })
}

fn render_program_carrier(
    template: &str,
    program_bytes: &[u8],
    fingerprint: &str,
) -> Result<String, PreparationError> {
    if template.matches(PROGRAM_BYTES_PLACEHOLDER).count() != 1 {
        return Err(PreparationError::InvalidProgramCarrierTemplate(
            "byte placeholder must appear exactly once",
        ));
    }
    if template.matches(PROGRAM_FINGERPRINT_PLACEHOLDER).count() != 1 {
        return Err(PreparationError::InvalidProgramCarrierTemplate(
            "fingerprint placeholder must appear exactly once",
        ));
    }

    let mut byte_literals = String::new();
    for chunk in program_bytes.chunks(16) {
        byte_literals.push_str("        ");
        for (index, byte) in chunk.iter().enumerate() {
            use std::fmt::Write as _;
            if index != 0 {
                byte_literals.push(' ');
            }
            write!(&mut byte_literals, "0x{byte:02x},").expect("writing to String cannot fail");
        }
        byte_literals.push('\n');
    }
    Ok(template
        .replace(PROGRAM_BYTES_PLACEHOLDER, &byte_literals)
        .replace(PROGRAM_FINGERPRINT_PLACEHOLDER, fingerprint))
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    use sha2::Digest as _;

    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use baml_base::Name as BaseName;
    use baml_codegen_types::{Name, SymbolPool, write_generated_output};
    use tempfile::TempDir;

    use super::*;
    use crate::{
        names::{
            BamlFqn, BamlWireName, CSharpNameKind, CSharpNameOrigin, CSharpNameRequest,
            CSharpScope, CSharpVisibility,
        },
        output::validate_and_collect,
    };

    const HEADER: &str = "// <auto-generated />\n#nullable enable\n";

    fn symbol(name: &str) -> Name {
        Name::new(BaseName::new("user"), vec![], BaseName::new(name))
    }

    fn setup() -> (
        CodegenModel,
        CSharpNames,
        CSharpFileRoutes,
        FileRouteRequest,
        FileRouteRequest,
    ) {
        let program_symbol = symbol("BamlProgram");
        let functions_symbol = symbol("Functions");
        let scope = CSharpScope::Namespace {
            package: BaseName::new("user"),
            path: vec![],
        };
        let holder_request = CSharpNameRequest::new(
            BamlFqn::symbol(&functions_symbol),
            BamlWireName::Symbol(functions_symbol.clone()),
            "Functions",
            CSharpNameKind::FunctionsHolder,
            CSharpVisibility::Public,
            CSharpNameOrigin::CompilerGenerated,
            scope.clone(),
        );
        let carrier_request = CSharpNameRequest::new(
            BamlFqn::symbol(&program_symbol),
            BamlWireName::Generated,
            "BamlProgram",
            CSharpNameKind::FileStem,
            CSharpVisibility::Internal,
            CSharpNameOrigin::CompilerGenerated,
            scope.clone(),
        );
        let functions_request = CSharpNameRequest::new(
            BamlFqn::symbol(&functions_symbol),
            BamlWireName::Generated,
            "Functions",
            CSharpNameKind::FileStem,
            CSharpVisibility::Internal,
            CSharpNameOrigin::CompilerGenerated,
            scope.clone(),
        );
        let namespace_symbol = symbol("Acme");
        let namespace_request = CSharpNameRequest::new(
            BamlFqn::symbol(&namespace_symbol),
            BamlWireName::Generated,
            "Acme",
            CSharpNameKind::NamespaceSegment,
            CSharpVisibility::Internal,
            CSharpNameOrigin::CompilerGenerated,
            scope,
        );
        let names = CSharpNames::allocate([
            holder_request,
            carrier_request.clone(),
            functions_request.clone(),
            namespace_request.clone(),
        ]);
        let carrier = FileRouteRequest::new(
            BamlFqn::symbol(&program_symbol),
            std::iter::empty(),
            names.get(&carrier_request).unwrap().clone(),
        )
        .unwrap();
        let functions = FileRouteRequest::new(
            BamlFqn::symbol(&functions_symbol),
            [names.get(&namespace_request).unwrap().clone()],
            names.get(&functions_request).unwrap().clone(),
        )
        .unwrap();
        let routes = CSharpFileRoutes::allocate([carrier.clone(), functions.clone()]).unwrap();
        let model = CodegenModel {
            symbols: SymbolPool::new(),
            callables: HashMap::new(),
        };
        (model, names, routes, carrier, functions)
    }

    #[test]
    fn preparation_consumes_every_route_and_owns_bytecode_projection() {
        let (model, names, routes, carrier, functions) = setup();
        let plan = CSharpRenderPlan::new(&model, &names, &routes).unwrap();
        let bytes = vec![0, 1, 254, 255];
        let carrier_template = plan
            .program_carrier(
                &carrier,
                format!(
                    "{HEADER}internal static class BamlProgram {{\n    const string Fingerprint = \"{PROGRAM_FINGERPRINT_PLACEHOLDER}\";\n    static readonly byte[] Bytes = [\n{PROGRAM_BYTES_PLACEHOLDER}    ];\n}}\n"
                ),
            )
            .unwrap();
        let functions_source = plan
            .render_source(&functions, |context| {
                assert!(!context.names().is_empty());
                assert_eq!(
                    context.route().relative_path(),
                    routes.get(&functions).unwrap().relative_path()
                );
                format!("{HEADER}public static class Functions {{}}\n").into_bytes()
            })
            .unwrap();
        let tree = plan
            .prepare(GenerationInput::new(
                1,
                "1.2.3",
                "1.2.3",
                "program",
                bytes.clone(),
                carrier_template,
                vec![functions_source],
            ))
            .unwrap();
        assert_eq!(tree.program_bytes, bytes);
        assert_eq!(tree.files.len(), 2);
        let carrier = tree
            .files
            .iter()
            .find(|file| file.relative_path.ends_with("BamlProgram.g.cs"))
            .unwrap();
        let source = std::str::from_utf8(&carrier.contents).unwrap();
        assert!(source.contains("0x00, 0x01, 0xfe, 0xff,"));
        assert!(
            source.lines().all(|line| line.trim_end() == line),
            "generated carriers must not contain trailing whitespace"
        );
        assert!(source.contains(&tree.metadata.program_fingerprint));
        assert!(!source.contains("{{BAML_"));

        let root = TempDir::new().unwrap();
        let (_, files) = validate_and_collect(&tree).unwrap();
        write_generated_output(
            &root.path().join("baml_client"),
            files,
            &baml_codegen_types::OutputOptions {
                provenance: baml_codegen_types::OutputProvenance {
                    input_fingerprint: "fingerprint".to_string(),
                    toolchain_version: "0.0.0-test".to_string(),
                    generator_name: "client1".to_string(),
                },
                vcs: baml_codegen_types::VcsPolicy::Ignore,
            },
        )
        .unwrap();
        assert!(root.path().join("baml_client/BamlProgram.g.cs").is_file());
        assert!(!root.path().join("baml_client/program.baml").exists());
    }

    #[test]
    fn preparation_rejects_route_gaps_and_bad_carrier_templates() {
        let (model, names, routes, carrier, _functions) = setup();
        let plan = CSharpRenderPlan::new(&model, &names, &routes).unwrap();
        let program_carrier = plan
            .program_carrier(
                &carrier,
                format!("{HEADER}{PROGRAM_BYTES_PLACEHOLDER}{PROGRAM_FINGERPRINT_PLACEHOLDER}"),
            )
            .unwrap();
        let input = GenerationInput::new(
            1,
            "1.2.3",
            "1.2.3",
            "program",
            vec![1],
            program_carrier,
            Vec::new(),
        );
        assert!(matches!(
            plan.prepare(input),
            Err(PreparationError::UnrenderedAllocatedRoute(_))
        ));

        let invalid_carrier = plan.program_carrier(&carrier, HEADER).unwrap();
        assert!(matches!(
            plan.prepare(GenerationInput::new(
                1,
                "1.2.3",
                "1.2.3",
                "program",
                vec![1],
                invalid_carrier,
                Vec::new(),
            )),
            Err(PreparationError::InvalidProgramCarrierTemplate(_))
        ));
    }

    #[test]
    fn render_plan_rejects_routes_from_an_unrelated_name_allocation() {
        let (model, names, _, _, _) = setup();
        let foreign_symbol = symbol("Foreign");
        let foreign_request = CSharpNameRequest::new(
            BamlFqn::symbol(&foreign_symbol),
            BamlWireName::Generated,
            "Foreign",
            CSharpNameKind::FileStem,
            CSharpVisibility::Internal,
            CSharpNameOrigin::CompilerGenerated,
            CSharpScope::Namespace {
                package: BaseName::new("user"),
                path: vec![],
            },
        );
        let foreign_names = CSharpNames::allocate([foreign_request.clone()]);
        let route = FileRouteRequest::new(
            BamlFqn::symbol(&foreign_symbol),
            std::iter::empty(),
            foreign_names.get(&foreign_request).unwrap().clone(),
        )
        .unwrap();
        let routes = CSharpFileRoutes::allocate([route.clone()]).unwrap();
        assert!(matches!(
            CSharpRenderPlan::new(&model, &names, &routes),
            Err(PreparationError::RouteUsesForeignName(found)) if *found == route
        ));
    }
}
