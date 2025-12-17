#![allow(clippy::print_stdout)]
//! GEPA Runtime - Loads and executes the GEPA BAML functions
//!
//! This module manages the separate BamlRuntime instance that runs the GEPA
//! reflection functions (ProposeImprovements, MergeVariants, etc.)

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use baml_types::BamlValue;
use serde::{Deserialize, Serialize};

use super::{
    candidate::{
        CurrentMetrics, ImprovedFunction, OptimizableFunction, OptimizationObjectives,
        ReflectiveExample,
    },
    gepa_defaults,
};
use crate::{BamlRuntime, TripWire};

/// Version tracking for GEPA implementation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GEPAVersionInfo {
    pub baml_cli_version: String,
    pub created_at: String,
    pub gepa_baml_hash: String,
}

/// Status of the GEPA version
pub enum VersionStatus {
    /// GEPA files match the bundled defaults for this version
    Current,
    /// GEPA files are from an older version
    Outdated { current: String, bundled: String },
    /// GEPA files have been modified by the user
    Modified,
}

/// Runtime for executing GEPA reflection functions
pub struct GEPARuntime {
    runtime: Arc<BamlRuntime>,
    gepa_dir: PathBuf,
    version_info: Option<GEPAVersionInfo>,
    env_vars: HashMap<String, String>,
}

impl GEPARuntime {
    /// Initialize the GEPA runtime
    ///
    /// - Creates .baml_optimize/gepa/baml_src/ if it doesn't exist
    /// - Writes default GEPA BAML files on first run
    /// - Loads the GEPA runtime for executing reflection functions
    pub fn new(
        gepa_dir: &Path,
        env_vars: HashMap<String, String>,
        reset_defaults: bool,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<Self> {
        let baml_src_dir = gepa_dir.join("baml_src");
        let version_file = baml_src_dir.join(".gepa_version");

        // Check if we need to create/reset the GEPA files
        let should_create = !baml_src_dir.exists() || reset_defaults;

        if should_create {
            Self::create_default_gepa_files(&baml_src_dir, &version_file)?;
        }

        // Load version info
        let version_info = Self::load_version_info(&version_file)?;

        // Load the GEPA runtime
        let runtime = BamlRuntime::from_directory(&baml_src_dir, env_vars.clone(), feature_flags)
            .with_context(|| {
            format!(
                "Failed to load GEPA BAML runtime from {}.\n\
                 This may indicate invalid BAML syntax in the GEPA files.\n\
                 Try running: baml-cli optimize --reset-gepa-prompts\n\
                 Or check the files in: {}",
                baml_src_dir.display(),
                gepa_dir.display()
            )
        })?;

        Ok(Self {
            runtime: Arc::new(runtime),
            gepa_dir: gepa_dir.to_path_buf(),
            version_info,
            env_vars,
        })
    }

    /// Create the default GEPA files
    fn create_default_gepa_files(baml_src_dir: &Path, version_file: &Path) -> Result<()> {
        std::fs::create_dir_all(baml_src_dir)
            .context("Failed to create GEPA baml_src directory")?;

        // Write gepa.baml
        std::fs::write(baml_src_dir.join("gepa.baml"), gepa_defaults::GEPA_BAML)
            .context("Failed to write gepa.baml")?;

        // Write clients.baml
        std::fs::write(
            baml_src_dir.join("clients.baml"),
            gepa_defaults::CLIENTS_BAML,
        )
        .context("Failed to write clients.baml")?;

        // Write version info
        let version_info = GEPAVersionInfo {
            baml_cli_version: gepa_defaults::GEPA_VERSION.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            gepa_baml_hash: gepa_defaults::default_gepa_hash(),
        };

        let version_json = serde_json::to_string_pretty(&version_info)
            .context("Failed to serialize version info")?;

        std::fs::write(version_file, version_json).context("Failed to write .gepa_version")?;

        println!(
            "Created GEPA files in {} with defaults from baml-cli {}",
            baml_src_dir.display(),
            gepa_defaults::GEPA_VERSION
        );

        Ok(())
    }

    /// Load version info from .gepa_version file
    fn load_version_info(version_file: &Path) -> Result<Option<GEPAVersionInfo>> {
        if !version_file.exists() {
            return Ok(None);
        }

        let content =
            std::fs::read_to_string(version_file).context("Failed to read .gepa_version")?;

        let info: GEPAVersionInfo =
            serde_json::from_str(&content).context("Failed to parse .gepa_version")?;

        Ok(Some(info))
    }

    /// Compute hash of current GEPA files
    fn compute_current_hash(&self) -> Result<String> {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let baml_src_dir = self.gepa_dir.join("baml_src");

        let gepa_content =
            std::fs::read_to_string(baml_src_dir.join("gepa.baml")).unwrap_or_default();
        let clients_content =
            std::fs::read_to_string(baml_src_dir.join("clients.baml")).unwrap_or_default();

        let mut hasher = DefaultHasher::new();
        gepa_content.hash(&mut hasher);
        clients_content.hash(&mut hasher);

        Ok(format!("{:x}", hasher.finish()))
    }

    /// Check if the GEPA version is current
    pub fn check_version(&self) -> VersionStatus {
        let Some(ref info) = self.version_info else {
            return VersionStatus::Modified;
        };

        // Check if files have been modified
        let current_hash = self.compute_current_hash().unwrap_or_default();
        let default_hash = gepa_defaults::default_gepa_hash();

        if current_hash != info.gepa_baml_hash && current_hash != default_hash {
            return VersionStatus::Modified;
        }

        // Check version
        if info.baml_cli_version != gepa_defaults::GEPA_VERSION {
            return VersionStatus::Outdated {
                current: info.baml_cli_version.clone(),
                bundled: gepa_defaults::GEPA_VERSION.to_string(),
            };
        }

        VersionStatus::Current
    }

    /// Call the ProposeImprovements function
    pub async fn propose_improvements(
        &self,
        current: &OptimizableFunction,
        failures: &[ReflectiveExample],
        successes: Option<&[ReflectiveExample]>,
        objectives: Option<&OptimizationObjectives>,
        metrics: Option<&CurrentMetrics>,
    ) -> Result<ImprovedFunction> {
        // Convert inputs to BamlValue
        let current_val = serde_json::to_value(current)?;
        let current_baml = json_to_baml_value(current_val)?;

        let failures_val = serde_json::to_value(failures)?;
        let failures_baml = json_to_baml_value(failures_val)?;

        let successes_baml = match successes {
            Some(s) => {
                let val = serde_json::to_value(s)?;
                json_to_baml_value(val)?
            }
            None => BamlValue::Null,
        };

        let objectives_baml = match objectives {
            Some(o) => {
                let val = serde_json::to_value(o)?;
                json_to_baml_value(val)?
            }
            None => BamlValue::Null,
        };

        let metrics_baml = match metrics {
            Some(m) => {
                let val = serde_json::to_value(m)?;
                json_to_baml_value(val)?
            }
            None => BamlValue::Null,
        };

        // Build arguments
        let args: baml_types::BamlMap<String, BamlValue> = [
            ("current_function".to_string(), current_baml),
            ("failed_examples".to_string(), failures_baml),
            ("successful_examples".to_string(), successes_baml),
            ("optimization_objectives".to_string(), objectives_baml),
            ("current_metrics".to_string(), metrics_baml),
        ]
        .into_iter()
        .collect();

        // Create context manager
        let ctx_manager = self
            .runtime
            .create_ctx_manager(BamlValue::String("optimize".to_string()), None);

        // Call the function
        let (result, _call_id) = self
            .runtime
            .call_function(
                "ProposeImprovements".to_string(),
                &args,
                &ctx_manager,
                None,
                None,
                None,
                self.env_vars.clone(),
                None,
                TripWire::new(None),
            )
            .await;

        let result = result.context("Failed to call ProposeImprovements")?;

        // Parse result
        let parsed_result = result
            .result_with_constraints_content()
            .context("Failed to get result from ProposeImprovements")?;
        let baml_value: BamlValue = (&parsed_result.0).into();
        let result_json = baml_value_to_json(&baml_value)?;
        let improved: ImprovedFunction = serde_json::from_value(result_json)
            .context("Failed to parse ImprovedFunction result")?;

        Ok(improved)
    }

    /// Call the MergeVariants function
    pub async fn merge_variants(
        &self,
        variant_a: &OptimizableFunction,
        variant_b: &OptimizableFunction,
        a_strengths: &[String],
        b_strengths: &[String],
    ) -> Result<ImprovedFunction> {
        let variant_a_val = serde_json::to_value(variant_a)?;
        let variant_b_val = serde_json::to_value(variant_b)?;

        let args: baml_types::BamlMap<String, BamlValue> = [
            ("variant_a".to_string(), json_to_baml_value(variant_a_val)?),
            ("variant_b".to_string(), json_to_baml_value(variant_b_val)?),
            (
                "variant_a_strengths".to_string(),
                BamlValue::List(
                    a_strengths
                        .iter()
                        .map(|s| BamlValue::String(s.clone()))
                        .collect(),
                ),
            ),
            (
                "variant_b_strengths".to_string(),
                BamlValue::List(
                    b_strengths
                        .iter()
                        .map(|s| BamlValue::String(s.clone()))
                        .collect(),
                ),
            ),
        ]
        .into_iter()
        .collect();

        let ctx_manager = self
            .runtime
            .create_ctx_manager(BamlValue::String("optimize".to_string()), None);

        let (result, _call_id) = self
            .runtime
            .call_function(
                "MergeVariants".to_string(),
                &args,
                &ctx_manager,
                None,
                None,
                None,
                self.env_vars.clone(),
                None,
                TripWire::new(None),
            )
            .await;

        let result = result.context("Failed to call MergeVariants")?;

        let parsed_result = result
            .result_with_constraints_content()
            .context("Failed to get result from MergeVariants")?;
        let baml_value: BamlValue = (&parsed_result.0).into();
        let result_json = baml_value_to_json(&baml_value)?;
        let improved: ImprovedFunction = serde_json::from_value(result_json)
            .context("Failed to parse ImprovedFunction result")?;

        Ok(improved)
    }
}

/// Reset GEPA prompts to the bundled defaults
///
/// This is a standalone function that can be called without initializing a full GEPARuntime.
/// It simply overwrites the GEPA BAML files with the defaults bundled in this version of baml-cli.
pub fn reset_gepa_prompts(gepa_dir: &Path) -> Result<()> {
    let baml_src_dir = gepa_dir.join("baml_src");
    let version_file = baml_src_dir.join(".gepa_version");

    std::fs::create_dir_all(&baml_src_dir).context("Failed to create GEPA baml_src directory")?;

    // Write gepa.baml
    std::fs::write(baml_src_dir.join("gepa.baml"), gepa_defaults::GEPA_BAML)
        .context("Failed to write gepa.baml")?;

    // Write clients.baml
    std::fs::write(
        baml_src_dir.join("clients.baml"),
        gepa_defaults::CLIENTS_BAML,
    )
    .context("Failed to write clients.baml")?;

    // Write version info
    let version_info = GEPAVersionInfo {
        baml_cli_version: gepa_defaults::GEPA_VERSION.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        gepa_baml_hash: gepa_defaults::default_gepa_hash(),
    };

    let version_json =
        serde_json::to_string_pretty(&version_info).context("Failed to serialize version info")?;

    std::fs::write(version_file, version_json).context("Failed to write .gepa_version")?;

    Ok(())
}

/// Convert serde_json::Value to BamlValue
fn json_to_baml_value(val: serde_json::Value) -> Result<BamlValue> {
    match val {
        serde_json::Value::Null => Ok(BamlValue::Null),
        serde_json::Value::Bool(b) => Ok(BamlValue::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(BamlValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(BamlValue::Float(f))
            } else {
                anyhow::bail!("Invalid number: {n}")
            }
        }
        serde_json::Value::String(s) => Ok(BamlValue::String(s)),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>> = arr.into_iter().map(json_to_baml_value).collect();
            Ok(BamlValue::List(items?))
        }
        serde_json::Value::Object(obj) => {
            let map: Result<indexmap::IndexMap<_, _>> = obj
                .into_iter()
                .map(|(k, v)| json_to_baml_value(v).map(|bv| (k, bv)))
                .collect();
            Ok(BamlValue::Map(map?))
        }
    }
}

/// Convert BamlValue to serde_json::Value
fn baml_value_to_json(val: &BamlValue) -> Result<serde_json::Value> {
    match val {
        BamlValue::Null => Ok(serde_json::Value::Null),
        BamlValue::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        BamlValue::Int(i) => Ok(serde_json::Value::Number((*i).into())),
        BamlValue::Float(f) => Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()),
        )),
        BamlValue::String(s) => Ok(serde_json::Value::String(s.clone())),
        BamlValue::List(arr) => {
            let items: Result<Vec<_>> = arr.iter().map(baml_value_to_json).collect();
            Ok(serde_json::Value::Array(items?))
        }
        BamlValue::Map(map) => {
            let obj: Result<serde_json::Map<_, _>> = map
                .iter()
                .map(|(k, v)| baml_value_to_json(v).map(|jv| (k.clone(), jv)))
                .collect();
            Ok(serde_json::Value::Object(obj?))
        }
        BamlValue::Class(_, fields) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in fields {
                obj.insert(k.clone(), baml_value_to_json(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        BamlValue::Enum(_, val) => Ok(serde_json::Value::String(val.clone())),
        BamlValue::Media(_) => {
            anyhow::bail!("Cannot convert media types to JSON")
        }
    }
}
