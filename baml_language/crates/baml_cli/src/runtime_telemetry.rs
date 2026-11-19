//! Runtime recording configuration, separate from CLI usage analytics.
use std::{path::Path, sync::Arc};

use bex_engine::{BexEngine, EngineError, TelemetryRecording};
use sys_native::SysOpsExt;

/// Use the CLI's resolved project root, including on bytecode-cache hits.
/// Engine telemetry-off mode ignores the destination without creating files.
pub(crate) fn create_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
    sources: Vec<(String, String)>,
    build: Option<bex_engine::BuildProvider>,
) -> Result<BexEngine, EngineError> {
    create_engine_with_context(
        program,
        argv,
        project_root,
        sources,
        build,
        Default::default(),
    )
}

pub(crate) fn create_engine_with_context(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
    sources: Vec<(String, String)>,
    build: Option<bex_engine::BuildProvider>,
    launch_context: btel_types::context::Context,
) -> Result<BexEngine, EngineError> {
    // `--hot` compares each save with this.
    let hotswap_baseline = baml_env::hotswap_enabled().then(|| {
        bex_project::hotswap_fingerprint::declaration_fingerprints(
            sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str())),
        )
    });
    // Disabled recording must not validate or discover an unused destination.
    if btel_settings::mode::from_env()
        .map_err(|error| EngineError::Other(error.to_string()))?
        .is_none()
    {
        return BexEngine::new_with_config(
            program,
            Arc::new(sys_native::SysOps::native()),
            argv,
            bex_engine::EngineConfig {
                launch_context,
                runtime_compiler: Some(bex_project::runtime_compiler()),
                hotswap_baseline,
                ..bex_engine::EngineConfig::default()
            },
        );
    }
    let boundary = crate::cloud_config::Boundary::read(project_root)
        .map_err(|error| EngineError::Other(error.to_string()))?;
    let recording = recording(project_root, &boundary, sources, build);
    BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        bex_engine::EngineConfig {
            launch_context,
            runtime_compiler: Some(bex_project::runtime_compiler()),
            hotswap_baseline,
            recording: Some(recording),
            ..bex_engine::EngineConfig::default()
        },
    )
}

/// `baml run`'s hot swapping flags (BEP-087).
pub(crate) struct HotswapFlags {
    /// `--no-hotswap`.
    pub(crate) disabled: bool,
    /// `--hotswap-startup-wait`, over `[hotswap] startup_wait`.
    pub(crate) startup_wait: Option<std::time::Duration>,
}

/// [`create_engine_with_context`], swappable from Boundary Cloud when the
/// project marks functions, `[hotswap]` and `flags` leave it on, and
/// recordings go to Boundary Cloud. `baml run --hot` swaps from saves instead.
pub(crate) fn create_engine_with_hotswap(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
    sources: Vec<(String, String)>,
    build: Option<bex_engine::BuildProvider>,
    launch_context: btel_types::context::Context,
    flags: HotswapFlags,
) -> anyhow::Result<(BexEngine, Option<bex_hotswap::Prepared>)> {
    use anyhow::Context as _;
    // Swaps come from Boundary Cloud, so a run that records nothing swaps
    // nothing.
    if baml_env::hotswap_enabled() || flags.disabled || btel_settings::mode::from_env()?.is_none() {
        let engine =
            create_engine_with_context(program, argv, project_root, sources, build, launch_context)
                .context("failed to create engine")?;
        return Ok((engine, None));
    }
    let mut config = hotswap_config(project_root)?;
    if let Some(wait) = flags.startup_wait {
        config.startup_wait = wait;
    }
    let mut hotswap =
        bex_hotswap::HotswapBuild::new(sources.iter().cloned().collect(), None, false, config);
    // Reading git is not free: only a build that can be swapped needs it.
    if hotswap.config.enabled && !hotswap.marked().is_empty() {
        (hotswap.commit, hotswap.dirty) = crate::build_record::hotswap_commit(project_root);
    }
    let boundary = crate::cloud_config::Boundary::read(project_root)?;
    let recording = recording(project_root, &boundary, sources, build);
    let build = hotswap;
    let prepared =
        bex_hotswap::Prepared::new(build, false, &recording).map_err(anyhow::Error::msg)?;
    let mut config = bex_engine::EngineConfig {
        launch_context,
        runtime_compiler: Some(bex_project::runtime_compiler()),
        recording: Some(recording),
        ..bex_engine::EngineConfig::default()
    };
    if let Some(prepared) = &prepared {
        prepared.configure(&mut config);
    }
    let engine = BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        config,
    )
    .context("failed to create engine")?;
    Ok((engine, prepared))
}

/// `[hotswap]` in the project's `baml.toml`, or its defaults without one.
pub(crate) fn hotswap_config(project_root: &Path) -> anyhow::Result<bex_hotswap::HotswapConfig> {
    let path = project_root.join("baml.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(bex_hotswap::HotswapConfig::default());
    };
    let manifest = baml_db::manifest::parse(&text)
        .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
    bex_hotswap::HotswapConfig::from_manifest(&manifest.hotswap)
        .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))
}

fn recording(
    project_root: &Path,
    boundary: &crate::cloud_config::Boundary,
    sources: Vec<(String, String)>,
    build: Option<bex_engine::BuildProvider>,
) -> TelemetryRecording {
    let recording = TelemetryRecording::from_boundary_defaults(
        boundary.project.as_deref(),
        boundary.api_url.as_deref(),
    )
    .unwrap_or_else(|| {
        TelemetryRecording::local_files(
            project_root,
            btel_settings::publisher::RecordingConfig::default(),
        )
    })
    .with_host("baml")
    .with_sources(sources);
    match build {
        Some(build) => recording.with_build(build),
        None => recording,
    }
}

/// The session's `.baml` sources keyed by path relative to the project root,
/// stored with each recording.
pub(crate) fn session_sources(
    session: &crate::project_session::ProjectSession,
) -> Vec<(String, String)> {
    let root = session.root();
    session
        .resolved
        .files
        .iter()
        .map(|(path, content)| {
            let path = path.strip_prefix(root).unwrap_or(path);
            (path.to_string_lossy().into_owned(), content.clone())
        })
        .collect()
}
