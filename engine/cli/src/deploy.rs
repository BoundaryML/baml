use anyhow::{Context, Result};
use axum::{extract::Path, extract::Query, routing::get, Router};
use baml_runtime::{baml_src_files, BamlRuntime};
use base64::{engine::general_purpose, Engine as _};
use console::style;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Confirm;
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use indoc::indoc;
use internal_baml_core::configuration::CloudProject;
use reqwest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::{RefCell, RefMut};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use web_time::SystemTime;

use crate::api_client::{ApiClient, GetOrCreateProjectRequest, Project};
use crate::colordiff::print_diff;
use crate::propelauth::PersistedTokenData;
use crate::tui::FutureWithProgress;

// Constants (replace with actual values as needed)
#[derive(clap::Args, Debug)]
pub struct DeployArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub(super) from: PathBuf,

    #[arg(
        long,
        env = "NEXT_PUBLIC_BOUNDARY_API_URL",
        default_value = "https://api2.boundaryml.com",
        hide = true
    )]
    pub(super) api_url: String,
}

#[derive(Debug, Serialize)]
struct CreateDeploymentRequest {
    /// Map from file path (within baml_src) to the contents of the file
    ///
    /// For example, baml_src/clients.baml becomes { "clients.baml": "<contents>" }
    baml_src: IndexMap<String, String>,
}

const DEPLOYMENT_ID: &str = "prod";

#[derive(Debug, Deserialize)]
struct CreateBamlDeploymentResponse {
    #[allow(dead_code)]
    deployment_id: String,
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

impl DeployArgs {
    /// Implementation notes:
    ///
    ///   - selected dialoguer / indicatif based on https://fadeevab.com/comparison-of-rust-cli-prompts/
    pub async fn run_async(&self) -> Result<()> {
        let runtime = BamlRuntime::from_directory(&self.from, std::env::vars().collect())
            .context("Failed to build BAML runtime")?;

        let d = Deployer {
            from: self.from.clone(),
            runtime,
            api_url: self.api_url.clone(),
            token_data: RefCell::new(PersistedTokenData::read_from_storage()?),
        };

        d.run_async().await
    }
}

struct Deployer {
    from: PathBuf,
    runtime: BamlRuntime,

    api_url: String,
    token_data: RefCell<PersistedTokenData>,
}

impl Deployer {
    async fn run_async(&self) -> Result<()> {
        let cloud_projects = self.runtime.cloud_projects();

        if cloud_projects.is_empty() {
            self.deploy_new_project().await?;
        } else {
            for cloud_project in cloud_projects {
                self.deploy_project_no_progress_spinner(&cloud_project.project_id, IndexMap::new())
                    .with_progress_spinner(
                        format!("Deploying to {}", cloud_project.project_id),
                        |_| "done!".to_string(),
                        "something went wrong.",
                    )
                    .await?;
            }
        }

        Ok(())
    }

    /// Get the project ID for the user to deploy to.
    ///
    /// If the user's org has 0 projects, this will create one.
    /// If the user's org has 1 project, this will return that project.
    /// If the user's org has more than 1 project, this currently fails (in the
    /// future, we can prompt the user and ask them to choose one).
    async fn get_or_create_project(&self) -> Result<Project> {
        let api_client = ApiClient {
            base_url: self.api_url.clone(),
            token: self
                .token_data
                .borrow_mut()
                .access_token()
                .await?
                .to_string(),
        };

        let project_resp = api_client
            .get_or_create_project(GetOrCreateProjectRequest {})
            .await
            .context("Failed while requesting projects from API")?;

        match project_resp.single_project {
            Some(project) => {
                Ok(project)
            },
            // TODO: make this an interactive prompt
            None => Err(anyhow::anyhow!(
                "You have {} projects configured in Boundary Cloud; please specify which one you want to deploy to.",
                project_resp.total_project_count
            )),
        }
    }

    async fn deploy_new_project(&self) -> Result<CreateBamlDeploymentResponse> {
        let project = self
            .get_or_create_project()
            .with_progress_spinner(
                "Looking up your projects",
                |p| {
                    if p.auto_created {
                        format!("found none; created a new project!")
                    } else {
                        format!("found your project!")
                    }
                },
                "something went wrong.",
            )
            .await?;

        let project_dbid = if project.auto_created {
            project.dbid
        } else {
            let should_deploy = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Deploy to project {}?", project.dbid))
                .interact()
                .context("Failed to wait for user interaction")?;

            if !should_deploy {
                return Err(anyhow::anyhow!("Deployment cancelled"));
            }

            project.dbid
        };

        let new_generator_block = format!(
            r#"
generator cloud {{
  output_type cloud
  project_id "{project_dbid}"
  version "{}"
}}
            "#,
            env!("CARGO_PKG_VERSION")
        );
        let (path, prev_generators, new_generators) = match self.runtime.generator_path() {
            Some(path) => {
                let path = relative_path_to_baml_src(&path, &self.from)?;

                let current_generators =
                    std::fs::read_to_string(std::path::Path::new(&self.from).join(&path))
                        .context(format!("Failed to read generators in {}", path.display()))?;

                let new_generators = format!("{}{}", current_generators, new_generator_block);
                (path, current_generators, new_generators)
            }
            None => {
                let path = std::path::PathBuf::from("generators.baml");

                (path, String::new(), new_generator_block)
            }
        };

        let resp = self
            .deploy_project_no_progress_spinner(
                &project_dbid,
                vec![(path.to_string_lossy().to_string(), new_generators.clone())]
                    .into_iter()
                    .collect(),
            )
            .with_progress_spinner(
                "Deploying",
                |_| "done!".to_string(),
                "something went wrong.",
            )
            .await?;

        println!();
        println!();
        println!("Your project has been deployed with the following configuration:");
        print_diff(&prev_generators, &new_generators);
        println!();

        let should_append = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Save deployment configuration to {}?",
                path.display()
            ))
            .show_default(true)
            .interact()
            .context("Failed to wait for user interaction")?;

        if should_append {
            let generator_abspath = std::path::Path::new(&self.from).join(&path);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .open(&generator_abspath)
                .context(format!("Failed to open {}", generator_abspath.display()))?;
            writeln!(file, "{}", new_generators).context(format!(
                "Failed to write to {}",
                generator_abspath.display()
            ))?;
            println!("  Updated {}", path.display());
        } else {
            println!(
                "  No changes applied - future deployments may deploy to a different project."
            );
        }

        let function_names = self
            .runtime
            .function_names()
            .map(|f| format!("{}/v3/functions/{DEPLOYMENT_ID}/{}", self.api_url, f))
            .collect::<Vec<_>>();

        println!();
        match function_names.len() {
            0 => println!(
                "{}: there were zero functions defined in your project.",
                style("Warning").yellow()
            ),
            1 => println!("Your function is deployed at:\n  {}", function_names[0]),
            _ => {
                println!("Your functions are deployed at:");
                for name in function_names.iter().take(2) {
                    println!("  - {}", name);
                }
                if function_names.len() > 2 {
                    println!("  ... and {} others", function_names.len() - 2);
                }
            }
        }
        println!();
        println!(
            "{}",
            indoc! {
                r#"
        Next steps:

          1. Set environment variables for your deployed project:
          https://dashboard.boundaryml.com/projects/{project_id}/cloud

          2. Create an API key to call your deployed functions:
          https://dashboard.boundaryml.com/projects/{project_id}/api-keys

          3. Call your functions!

        Read the docs to learn more: https://docs.boundaryml.com/cloud
        "#
            }
        );

        Ok(resp)
    }

    async fn deploy_project_no_progress_spinner(
        &self,
        project_dbid: &str,
        baml_src_overrides: IndexMap<String, String>,
    ) -> Result<CreateBamlDeploymentResponse> {
        let mut baml_src = baml_src_files(&self.from)
            .context("Failed while searching for .baml files in baml_src/")?
            .into_iter()
            .map(|f| {
                Ok((
                    relative_path_to_baml_src(&f, &self.from)?
                        .to_string_lossy()
                        .to_string(),
                    std::fs::read_to_string(std::path::Path::new(&self.from).join(f))?,
                ))
            })
            .chain(baml_src_overrides.into_iter().map(|(k, v)| Ok((k, v))))
            .collect::<Result<IndexMap<_, _>>>()?;

        baml_src.shift_remove("generators.baml");

        let api_client = ApiClient {
            base_url: self.api_url.clone(),
            token: self
                .token_data
                .borrow_mut()
                .access_token()
                .await?
                .to_string(),
        };

        let client = reqwest::Client::new();
        let req = client
            .post(format!("{}/v3/functions/{DEPLOYMENT_ID}", self.api_url))
            .bearer_auth(self.token_data.borrow_mut().access_token().await?)
            .header("x-boundary-project-id", project_dbid);

        let response = req
            .json(&CreateDeploymentRequest { baml_src })
            .send()
            .await
            .context("Failed to send deployment request")?;

        if response.status().is_success() {
            let resp_body: CreateBamlDeploymentResponse = response.json().await?;
            Ok(resp_body)
        } else {
            Err(response
                .error_for_status()
                .context("Deployment failed")
                .unwrap_err())
        }
    }
}
