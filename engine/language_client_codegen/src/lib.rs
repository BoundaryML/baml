use anyhow::{Context, Result};
use baml_types::{Constraint, ConstraintLevel, FieldType};
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::{GeneratorDefaultClientMode, GeneratorOutputType},
    ir::repr::IntermediateRepr,
};
use std::{collections::{BTreeMap, HashSet}, path::PathBuf};
use version_check::{check_version, GeneratorType, VersionCheckMode};

mod dir_writer;
pub mod openapi;
mod python;
mod ruby;
mod typescript;
pub mod version_check;

pub struct GeneratorArgs {
    /// Output directory for the generated client, relative to baml_src
    output_dir_relative_to_baml_src: PathBuf,

    /// Path to the BAML source directory
    baml_src_dir: PathBuf,

    inlined_file_map: BTreeMap<PathBuf, String>,

    version: String,
    no_version_check: bool,

    // Default call mode for functions
    default_client_mode: GeneratorDefaultClientMode,
    on_generate: Vec<String>,
}

fn relative_path_to_baml_src(path: &PathBuf, baml_src: &PathBuf) -> Result<PathBuf> {
    pathdiff::diff_paths(path, baml_src).ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to compute relative path from {} to {}",
            path.display(),
            baml_src.display()
        )
    })
}

impl GeneratorArgs {
    pub fn new<'i>(
        output_dir_relative_to_baml_src: impl Into<PathBuf>,
        baml_src_dir: impl Into<PathBuf>,
        input_files: impl IntoIterator<Item = (&'i PathBuf, &'i String)>,
        version: String,
        no_version_check: bool,
        default_client_mode: GeneratorDefaultClientMode,
        on_generate: Vec<String>,
    ) -> Result<Self> {
        let baml_src = baml_src_dir.into();
        let input_file_map: BTreeMap<PathBuf, String> = input_files
            .into_iter()
            .map(|(k, v)| Ok((relative_path_to_baml_src(k, &baml_src)?, v.clone())))
            .collect::<Result<_>>()?;

        Ok(Self {
            output_dir_relative_to_baml_src: output_dir_relative_to_baml_src.into(),
            baml_src_dir: baml_src.clone(),
            // for the key, whhich is the name, just get the filename
            inlined_file_map: input_file_map,
            version,
            no_version_check,
            default_client_mode,
            on_generate,
        })
    }

    pub fn file_map(&self) -> Result<Vec<(String, String)>> {
        self.inlined_file_map
            .iter()
            .map(|(k, v)| {
                Ok((
                    serde_json::to_string(&k.display().to_string()).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize key {:#}", k.display()))
                    })?,
                    serde_json::to_string(v).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize contents of {:#}", k.display()))
                    })?,
                ))
            })
            .collect()
    }

    pub fn output_dir(&self) -> PathBuf {
        use sugar_path::SugarPath;
        self.baml_src_dir
            .join(&self.output_dir_relative_to_baml_src)
            .normalize()
    }

    /// Returns baml_src relative to the output_dir.
    ///
    /// We need this to be able to codegen a singleton client, so that our generated code can build
    /// baml_src relative to the path of the file in which the singleton is defined. In Python this is
    /// os.path.dirname(__file__) for globals.py; in TS this is __dirname for globals.ts.
    pub fn baml_src_relative_to_output_dir(&self) -> Result<PathBuf> {
        pathdiff::diff_paths(&self.baml_src_dir, &self.output_dir()).ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to compute baml_src ({}) relative to output_dir ({})",
                self.baml_src_dir.display(),
                self.output_dir().display()
            )
        })
    }
}

pub struct GenerateOutput {
    pub client_type: GeneratorOutputType,
    /// Relative path to the output directory (output_dir in the generator)
    pub output_dir_shorthand: PathBuf,
    /// The absolute path that the generated baml client was written to
    pub output_dir_full: PathBuf,
    pub files: IndexMap<PathBuf, String>,
}

pub trait GenerateClient {
    fn generate_client(&self, ir: &IntermediateRepr, gen: &GeneratorArgs)
        -> Result<GenerateOutput>;
}

// Assume VSCode is the only one that uses WASM, and it does call this method but at a different time.
#[cfg(target_arch = "wasm32")]
fn version_check_with_error(
    runtime_version: &str,
    gen_version: &str,
    generator_type: GeneratorType,
    mode: VersionCheckMode,
    client_type: GeneratorOutputType,
) -> Result<()> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn version_check_with_error(
    runtime_version: &str,
    gen_version: &str,
    generator_type: GeneratorType,
    mode: VersionCheckMode,
    client_type: GeneratorOutputType,
) -> Result<()> {
    let res = check_version(
        gen_version,
        runtime_version,
        generator_type,
        mode,
        client_type,
        true,
    );
    match res {
        Some(e) => Err(anyhow::anyhow!("{}", e.msg())),
        None => Ok(()),
    }
}

impl GenerateClient for GeneratorOutputType {
    fn generate_client(
        &self,
        ir: &IntermediateRepr,
        gen: &GeneratorArgs,
    ) -> Result<GenerateOutput> {
        let runtime_version = env!("CARGO_PKG_VERSION");

        if !gen.no_version_check {
            version_check_with_error(
                runtime_version,
                &gen.version,
                GeneratorType::CLI,
                VersionCheckMode::Strict,
                self.clone(),
            )?;
        }

        let files = match self {
            GeneratorOutputType::OpenApi => openapi::generate(ir, gen),
            GeneratorOutputType::PythonPydantic => python::generate(ir, gen),
            GeneratorOutputType::RubySorbet => ruby::generate(ir, gen),
            GeneratorOutputType::Typescript => typescript::generate(ir, gen),
        }?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            for cmd in gen.on_generate.iter() {
                log::info!("Running {:?} in {}", cmd, gen.output_dir().display());
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(gen.output_dir())
                    .status()
                    .context(format!("Failed to run on_generate command {:?}", cmd))?;
                if !status.success() {
                    anyhow::bail!(
                        "on_generate command finished with {}: {:?}",
                        match status.code() {
                            Some(code) => format!("exit code {}", code),
                            None => "no exit code".to_string(),
                        },
                        cmd,
                    );
                }
            }

            if matches!(self, GeneratorOutputType::OpenApi) && gen.on_generate.is_empty() {
                // TODO: we should auto-suggest a command for the user to run here
                log::warn!("No on_generate commands were provided for OpenAPI generator - skipping OpenAPI client generation");
            }
        }

        Ok(GenerateOutput {
            client_type: self.clone(),
            output_dir_shorthand: gen.output_dir_relative_to_baml_src.clone(),
            output_dir_full: gen.output_dir(),
            files,
        })
    }
}

/// A set of names of @check attributes. This set determines the
/// way name of a Python Class or TypeScript Interface that holds
/// the results of running these checks. See TODO (Docs) for details on
/// the support types generated from checks.
#[derive(Clone, Debug, Eq)]
pub struct TypeCheckAttributes(pub HashSet<String>);

impl PartialEq for TypeCheckAttributes {
   fn eq(&self, other: &Self) -> bool {
       self.0.len() == other.0.len() && self.0.iter().all(|x| other.0.contains(x))
   }
}

impl <'a> std::hash::Hash for TypeCheckAttributes {
    fn hash<H>(&self, state: &mut H)
        where H: std::hash::Hasher
    {
        let mut strings: Vec<_> = self.0.iter().collect();
        strings.sort();
        strings.into_iter().for_each(|s| s.hash(state))
    }

}

impl TypeCheckAttributes {
    /// Extend one set of attributes with the contents of another.
    pub fn extend(&mut self, other: &TypeCheckAttributes) {
        self.0.extend(other.0.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Search the IR for all types with checks, combining the checks on each type
/// into a `TypeCheckAttributes` (a HashSet of the check names). Return a HashSet
/// of these HashSets.
///
/// For example, consider this IR defining two classes:
///
/// ``` baml
/// class Foo {
///   int @check("a") @check("b")
///   string @check("a")
/// }
///
/// class Bar {
///   bool @check("a")
/// }
/// ````
///
/// It contains two distinct `TypeCheckAttributes`:
/// - ["a"]
/// - ["a", "b"]
///
/// We will need to construct two district support types:
/// `Classes_a` and `Classes_a_b`.
pub fn type_check_attributes(
    ir: &IntermediateRepr
) -> HashSet<TypeCheckAttributes> {


    let mut all_types_in_ir: Vec<&FieldType> = Vec::new();
    for class in ir.walk_classes() {
        for field in class.item.elem.static_fields.iter() {
            let field_type = &field.elem.r#type.elem;
            all_types_in_ir.push(field_type);
        }
    }
    for function in ir.walk_functions() {
        for (_param_name, parameter) in function.item.elem.inputs.iter() {
            all_types_in_ir.push(parameter);
        }
        let return_type = &function.item.elem.output;
        all_types_in_ir.push(return_type);
    }

    all_types_in_ir.into_iter().filter_map(field_type_attributes).collect()

}

/// The set of Check names associated with a type.
fn field_type_attributes<'a>(field_type: &FieldType) -> Option<TypeCheckAttributes> {
    match field_type {
        FieldType::Constrained {base, constraints} => {
            let direct_sub_attributes = field_type_attributes(base);
            let mut check_names =
                TypeCheckAttributes(
                    constraints
                        .iter()
                        .filter_map(|Constraint {label, level, ..}|
                                    if matches!(level, ConstraintLevel::Check) {
                                        Some(label.clone().expect("TODO"))
                                    } else { None }
                        ).collect::<HashSet<String>>());
            if let Some(ref sub_attrs) = direct_sub_attributes {
                check_names.extend(&sub_attrs);
            }
            if !check_names.is_empty() {
                Some(check_names)
            } else {
                None
            }
        },
        _ => None
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::repr::make_test_ir;
    use super::*;


    /// Utility function for creating test fixtures.
    fn mk_tc_attrs(names: &[&str]) -> TypeCheckAttributes {
        TypeCheckAttributes(names.into_iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn type_check_attributes_eq() {
        assert_eq!(mk_tc_attrs(&["a", "b"]), mk_tc_attrs(&["b", "a"]));

        let attrs: HashSet<TypeCheckAttributes> = vec![mk_tc_attrs(&["a", "b"])].into_iter().collect();
        assert!(attrs.contains( &mk_tc_attrs(&["a", "b"]) ));
        assert!(attrs.contains( &mk_tc_attrs(&["b", "a"]) ));

    }

    #[test]
    fn find_type_check_attributes() {
        let ir = make_test_ir(
            r##"
client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

function Go(a: int @assert({{ this < 0 }}, c)) -> Foo {
  client GPT4
  prompt #""#
}

class Foo {
  ab int @check({{this}}, a) @check({{this}}, b)
  a int @check({{this}}, a)
}

class Bar {
  cb int @check({{this}}, c) @check({{this}}, b)
  nil int @description("no checks") @assert({{this}}, a) @assert({{this}}, d)
}

        "##).expect("Valid source");

        let attrs = type_check_attributes(&ir);
        dbg!(&attrs);
        assert_eq!(attrs.len(), 3);
        assert!(attrs.contains( &mk_tc_attrs(&["a","b"]) ));
        assert!(attrs.contains( &mk_tc_attrs(&["a"]) ));
        assert!(attrs.contains( &mk_tc_attrs(&["b", "c"]) ));
        assert!(!attrs.contains( &mk_tc_attrs(&["a", "d"]) ));
    }
}
