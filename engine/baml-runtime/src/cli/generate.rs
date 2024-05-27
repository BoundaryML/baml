use crate::{
    runtime::{self, runtime_interface::baml_src_files},
    BamlRuntime,
};
use anyhow::Result;
use internal_baml_core::configuration::GeneratorOutputType;
use std::path::PathBuf;

use super::LanguageClientType;

#[derive(clap::Args, Debug)]
pub struct GenerateArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    from: String,
}

impl GenerateArgs {
    pub fn run(&self, caller_type: super::CallerType) -> Result<()> {
        let src_dir = PathBuf::from(&self.from);
        let runtime = BamlRuntime::from_directory(&src_dir, std::env::vars().collect())?;

        // Safe to unwrap as the files are guaranteed to exist if the runtime was created successfully.
        let src_files = baml_src_files(&src_dir)?;
        let all_files = src_files
            .iter()
            .map(|k| {
                (
                    k.to_string_lossy().into(),
                    std::fs::read_to_string(&k).unwrap(),
                )
            })
            .collect();

        let generated = runtime.run_generators(&all_files)?;

        if generated.is_empty() {
            let client_type = caller_type.into();
            // Normally `baml_client` is added via the generator, but since we're not running the generator, we need to add it manually.
            let output_dir = src_dir.join("..").join("baml_client");
            let generate_output = runtime.generate_client(
                &client_type,
                &internal_baml_codegen::GeneratorArgs::new(&output_dir, &self.from, &all_files),
            )?;
            println!("Generated 1 baml_client at {}.", output_dir.display());

            println!(
                r#"
You can automatically generate a client by adding the following to any one of your BAML files:

generator my_client {{
    output_type "{}"
    output_dir "../"
}}"#,
                generate_output.client_type.to_string()
            );
        } else {
            println!("Generated {} baml_client's", generated.len());
        }

        Ok(())
    }
}
