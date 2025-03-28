use anyhow::{Context, Result};
use baml_runtime::{baml_src_files, BamlRuntime};
use bstd::ProjectFqn;
use console::style;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Confirm;
use futures::join;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

use crate::api_client::{
    ApiClient, CreateDeploymentRequest, CreateDeploymentResponse, CreateProjectRequest,
    ListProjectsRequest, Project, DEPLOYMENT_ID,
};
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

fn relative_path_to_baml_src(path: &Path, baml_src: &Path) -> Result<PathBuf> {
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

        let version_check_errors = cloud_projects
            .iter()
            .filter_map(|cloud_project| {
                internal_baml_codegen::version_check::check_version(
                    &cloud_project.version,
                    env!("CARGO_PKG_VERSION"),
                    internal_baml_codegen::version_check::GeneratorType::CLI,
                    internal_baml_codegen::version_check::VersionCheckMode::Strict,
                    baml_types::GeneratorOutputType::OpenApi,
                    false,
                )
            })
            .collect::<Vec<_>>();
        if !version_check_errors.is_empty() {
            let mut err = anyhow::anyhow!("Version check failed");
            for error in version_check_errors.iter() {
                err = err.context(error.msg());
            }
            return Err(err);
        }

        if cloud_projects.is_empty() {
            self.deploy_new_project().await?;
        } else {
            for cloud_project in cloud_projects {
                self.deploy_project_no_progress_spinner(
                    &cloud_project.project_fqn,
                    IndexMap::new(),
                )
                .with_progress_spinner(
                    format!("Deploying to {}", cloud_project.project_fqn),
                    |_| "done!".to_string(),
                    "something went wrong.",
                )
                .await?;
            }
        }

        Ok(())
    }
}

fn choose_project_shortname() -> Result<String> {
    dialoguer::Input::with_theme(&ColorfulTheme::default())
        .with_prompt("What's your project's name?")
        .with_initial_text(bstd::random_word_id())
        .validate_with(|input: &String| -> Result<(), String> {
            ProjectFqn::is_valid_project_shortname(input)
        })
        .interact_text()
        .context("Failed to wait for user input")
}

enum GetOrCreateProjectResult {
    Existing(Project),
    ToBeCreated(String),
}

impl Deployer {
    async fn get_or_create_project(&self) -> Result<GetOrCreateProjectResult> {
        let propel_auth_client = super::propelauth::PropelAuthClient::new()?;
        let user_info = propel_auth_client
            .get_user_info(self.token_data.borrow_mut().access_token().await?)
            .await?;

        let org_options = user_info.org_id_to_org_info.values().collect::<Vec<_>>();
        let selected_org_idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Which organization do you want to deploy to?")
            .items(
                &org_options
                    .iter()
                    .map(|o| &o.url_safe_org_name)
                    .collect::<Vec<_>>(),
            )
            .default(0)
            .interact()?;
        let org = org_options[selected_org_idx];
        let org_slug = &org.url_safe_org_name;

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
            .list_projects(ListProjectsRequest {
                org_slug: org_slug.clone(),
            })
            .await
            .context("Failed while requesting projects from API")?;

        match project_resp.projects.len() {
            0 => {
                let project_shortname = choose_project_shortname()?;
                Ok(GetOrCreateProjectResult::ToBeCreated(format!(
                    "{}/{}",
                    org_slug, project_shortname
                )))
            }
            _ => {
                let project_idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Would you like to create a new project or deploy to {}?",
                        bstd::pluralize(
                            project_resp.projects.len(),
                            "your existing project",
                            "one of your existing projects"
                        )
                    ))
                    .item("Create new project")
                    .items(
                        &project_resp
                            .projects
                            .iter()
                            .map(|p| format!("Deploy to existing project {}", &p.project_fqn))
                            .collect::<Vec<_>>(),
                    )
                    .default(0)
                    .interact()
                    .context("Failed to wait for user input")?;

                if project_idx == 0 {
                    let project_shortname = choose_project_shortname()?;

                    Ok(GetOrCreateProjectResult::ToBeCreated(format!(
                        "{}/{}",
                        org_slug, project_shortname
                    )))
                } else {
                    Ok(GetOrCreateProjectResult::Existing(
                        project_resp.projects[project_idx - 1].clone(),
                    ))
                }
            }
        }
    }

    async fn deploy_new_project(&self) -> Result<CreateDeploymentResponse> {
        let api_client = ApiClient {
            base_url: self.api_url.clone(),
            token: self
                .token_data
                .borrow_mut()
                .access_token()
                .await?
                .to_string(),
        };

        let get_or_create = self.get_or_create_project().await?;
        let project_fqn = ProjectFqn::parse(match &get_or_create {
            GetOrCreateProjectResult::Existing(project) => &project.project_fqn,
            GetOrCreateProjectResult::ToBeCreated(project_fqn) => project_fqn,
        })?;

        let new_generator_block = format!(
            r#"
generator cloud {{
  output_type boundary-cloud
  project "{}"
  version "{}"
}}
            "#,
            project_fqn,
            env!("CARGO_PKG_VERSION")
        );
        let (path, prev_generators, new_generators) = match self.runtime.generator_path() {
            Some(path) => {
                let current_generators =
                    std::fs::read_to_string(std::path::Path::new(&self.from).join(&path))
                        .context(format!("Failed to read generators in {}", path.display()))?;

                let new_generators = format!("{}{}", current_generators, new_generator_block);
                (path, current_generators, new_generators)
            }
            None => ("generators.baml".into(), String::new(), new_generator_block),
        };

        println!();
        println!("Your project will be deployed with the following configuration:");
        print_diff(&prev_generators, &new_generators);
        println!();

        let should_append = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Save configuration to {} and deploy your project?",
                path.display()
            ))
            .show_default(true)
            .interact()
            .context("Failed to wait for user interaction")?;

        if !should_append {
            anyhow::bail!("Exiting.");
        }

        let generator_abspath = std::path::Path::new(&self.from).join(&path);
        log::debug!("Will write generators to {}", generator_abspath.display());
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&generator_abspath)
            .context(format!("Failed to open {}", generator_abspath.display()))?;
        writeln!(file, "{}", new_generators).context(format!(
            "Failed to write to {}",
            generator_abspath.display()
        ))?;

        let project_id = match get_or_create {
            GetOrCreateProjectResult::Existing(project) => project.project_id,
            GetOrCreateProjectResult::ToBeCreated(project_fqn) => {
                async {
                    let (resp, _) = join!(
                        api_client.create_project(CreateProjectRequest {
                            project_fqn: project_fqn.to_string(),
                        }),
                        sleep(Duration::from_millis(1500)),
                    );
                    resp
                }
                .with_progress_spinner(
                    format!("Creating project {}", project_fqn),
                    |_| "done!".to_string(),
                    "uh-oh, something went wrong.",
                )
                .await
                .context("Failed while creating project in API")?
                .project
                .project_id
            }
        };

        let resp = async {
            let (resp, _) = join!(
                self.deploy_project_no_progress_spinner(
                    &project_fqn,
                    vec![(path.to_string_lossy().to_string(), new_generators.clone())]
                        .into_iter()
                        .collect(),
                ),
                sleep(Duration::from_millis(1500)),
            );
            resp
        }
        .with_progress_spinner(
            format!("Deploying to {}", project_fqn),
            |_| "done!".to_string(),
            "uh-oh, something went wrong.",
        )
        .await?;

        let function_names = self
            .runtime
            .function_names()
            .map(|f| format!("{}/v3/functions/{DEPLOYMENT_ID}/{}", self.api_url, f))
            .collect::<Vec<_>>();

        println!();
        println!();
        match function_names.len() {
            0 => println!(
                "{}: deploy succeeded, but there are zero functions defined in your project.",
                style("Warning").yellow()
            ),
            1 => println!("1 function deployed at:\n  {}", function_names[0]),
            _ => {
                println!("{} functions deployed at:", function_names.len());
                for name in function_names.iter().take(2) {
                    println!("  - {}", name);
                }
                if function_names.len() > 2 {
                    println!("  ... and {} others", function_names.len() - 2);
                }
            }
        }
        println!(
            r#"
Next steps:

    1. Set environment variables for your deployed project:
    https://dashboard.boundaryml.com/projects/{project_id}/cloud

    2. Create an API key to call your deployed functions:
    https://dashboard.boundaryml.com/projects/{project_id}/api-keys

    3. Call your functions!

Read the docs to learn more: https://docs.boundaryml.com/cloud
        "#
        );

        Ok(resp)
    }

    async fn deploy_project_no_progress_spinner(
        &self,
        project_fqn: &ProjectFqn,
        baml_src_overrides: IndexMap<String, String>,
    ) -> Result<CreateDeploymentResponse> {
        let baml_src = baml_src_files(&self.from)
            .context("Failed while searching for .baml files in baml_src/")?
            .into_iter()
            .map(|f| {
                Ok((
                    relative_path_to_baml_src(&f, &self.from)?
                        .to_string_lossy()
                        .to_string(),
                    std::fs::read_to_string(&f)
                        .context(format!("Failed to read {}", f.display()))?,
                ))
            })
            .chain(baml_src_overrides.into_iter().map(|(k, v)| Ok((k, v))))
            .collect::<Result<IndexMap<_, _>>>()?;

        let api_client = ApiClient {
            base_url: self.api_url.clone(),
            token: self
                .token_data
                .borrow_mut()
                .access_token()
                .await?
                .to_string(),
        };

        api_client
            .create_deployment(CreateDeploymentRequest {
                baml_src,
                project_fqn: project_fqn.to_string(),
            })
            .await
            .context("Failed while creating deployment")
    }
}
