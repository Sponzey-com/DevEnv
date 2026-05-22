use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    ActivationPlan, ArchiveType, Artifact, CoreResult, DownloadedArtifact, EnvDelta,
    ExtractionManifest, InstallPlan, InstallTransaction, Installation, InstallationMetadata,
    Platform, RegisteredRuntime, ShimSpec, ToolName, Version, VersionRequirement, VersionScheme,
};

/// Built-in or plugin-provided behavior for one supported tool.
pub trait ToolAdapter {
    fn metadata(&self) -> &ToolMetadata;
    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>>;
    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan>;

    fn exposed_binaries(&self) -> &[String] {
        self.metadata().exposed_binaries()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetadata {
    name: ToolName,
    version_scheme: VersionScheme,
    exposed_binaries: Vec<String>,
}

impl ToolMetadata {
    pub fn new(
        name: ToolName,
        version_scheme: VersionScheme,
        exposed_binaries: Vec<String>,
    ) -> Self {
        Self {
            name,
            version_scheme,
            exposed_binaries,
        }
    }

    pub fn name(&self) -> &ToolName {
        &self.name
    }

    pub fn version_scheme(&self) -> &VersionScheme {
        &self.version_scheme
    }

    pub fn exposed_binaries(&self) -> &[String] {
        &self.exposed_binaries
    }
}

/// Source of remote or synthetic versions for a tool.
pub trait VersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>>;
}

/// Tool-specific version matching. The core does not assume semver.
pub trait VersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>>;
}

/// Converts a tool/version/platform request into a downloadable artifact.
pub trait ArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact>;
}

/// Downloads an artifact into a caller-provided path.
pub trait Downloader {
    fn download(
        &mut self,
        artifact: &Artifact,
        destination: &Path,
    ) -> CoreResult<DownloadedArtifact>;
}

/// Verifies an already downloaded artifact against an expected checksum.
pub trait ChecksumVerifier {
    fn verify(&self, artifact_path: &Path, expected_checksum: &str) -> CoreResult<()>;
}

/// Extracts an archive into a caller-provided directory and reports written entries.
pub trait ArchiveExtractor {
    fn extract(
        &mut self,
        archive_path: &Path,
        destination: &Path,
        archive_type: ArchiveType,
    ) -> CoreResult<ExtractionManifest>;
}

/// Validates an extracted runtime before it is committed into the owned store.
pub trait InstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()>;
}

/// Owns the temp directories and final atomic placement for one install.
pub trait InstallTransactionManager {
    fn install_root(&self, tool: &ToolName, version: &Version, platform: Platform) -> PathBuf;
    fn begin(&mut self, plan: &InstallPlan) -> CoreResult<InstallTransaction>;
    fn commit(&mut self, transaction: &InstallTransaction) -> CoreResult<()>;
    fn cleanup(&mut self, transaction: &InstallTransaction) -> CoreResult<()>;
}

/// Store for DevEnv-owned runtime installations.
pub trait InstallStore {
    fn add_installation(&mut self, installation: Installation) -> CoreResult<()>;
    fn list_installations(&self, tool: &ToolName) -> Vec<Installation>;
    fn remove_installation_metadata(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Option<InstallationMetadata>>;

    fn add_installation_metadata(&mut self, metadata: InstallationMetadata) -> CoreResult<()> {
        self.add_installation(metadata.installation().clone())
    }

    fn list_installation_metadata(&self, tool: &ToolName) -> Vec<InstallationMetadata> {
        self.list_installations(tool)
            .into_iter()
            .map(|installation| InstallationMetadata::new(installation, "unknown", None, "unknown"))
            .collect()
    }
}

/// Writes executable shims for tool binaries.
pub trait ShimWriter {
    fn shim_dir(&self) -> &Path;
    fn write_shim(&mut self, spec: &ShimSpec) -> CoreResult<()>;
}

/// Registry for external runtimes that DevEnv references but does not own.
pub trait RuntimeRegistry {
    fn add_registered_runtime(&mut self, runtime: RegisteredRuntime) -> CoreResult<()>;
    fn remove_registered_runtime(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
        root: Option<&Path>,
    ) -> CoreResult<Vec<RegisteredRuntime>>;
    fn list_registered_runtimes(&self, tool: &ToolName) -> Vec<RegisteredRuntime>;
}

/// Reads and writes selected tool requirements from config-like storage.
pub trait ConfigRepository {
    fn get_requirement(&self, tool: &ToolName) -> CoreResult<Option<VersionRequirement>>;
    fn set_requirement(
        &mut self,
        tool: ToolName,
        requirement: VersionRequirement,
    ) -> CoreResult<()>;
}

/// Renders an activation plan for a specific shell or integration surface.
pub trait ActivationRenderer {
    fn render(&self, plan: &ActivationPlan) -> CoreResult<String>;
}

/// Detects the platform used for artifact and runtime selection.
pub trait PlatformDetector {
    fn current_platform(&self) -> CoreResult<Platform>;
}

/// Runs commands through an adapter boundary so tests avoid real process execution.
pub trait CommandRunner {
    fn run(&mut self, invocation: CommandInvocation) -> CoreResult<CommandOutput>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    command: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    activation: ActivationPlan,
    env_delta: EnvDelta,
}

impl CommandInvocation {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            cwd: None,
            activation: ActivationPlan::new(),
            env_delta: EnvDelta::new(),
        }
    }

    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_activation(mut self, activation: ActivationPlan) -> Self {
        self.activation = activation;
        self
    }

    pub fn with_env_delta(mut self, env_delta: EnvDelta) -> Self {
        self.env_delta = env_delta;
        self
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    pub fn activation(&self) -> &ActivationPlan {
        &self.activation
    }

    pub fn env_delta(&self) -> &EnvDelta {
        &self.env_delta
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    pub fn new(status_code: i32, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            status_code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    pub fn status_code(&self) -> i32 {
        self.status_code
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

/// Coordinates concurrent mutation of one logical install target.
pub trait LockManager {
    fn acquire(&mut self, key: LockKey) -> CoreResult<bool>;
    fn release(&mut self, key: &LockKey) -> CoreResult<()>;
}

/// Provides timestamps for metadata without coupling use cases to the system clock.
pub trait Clock {
    fn now_utc(&self) -> CoreResult<String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockKey {
    value: String,
}

impl LockKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

pub(crate) fn by_tool<T, F>(items: &[T], tool: &ToolName, get_tool: F) -> Vec<T>
where
    T: Clone,
    F: Fn(&T) -> &ToolName,
{
    items
        .iter()
        .filter(|item| get_tool(item) == tool)
        .cloned()
        .collect()
}

pub(crate) fn get_config(
    requirements: &HashMap<ToolName, VersionRequirement>,
    tool: &ToolName,
) -> Option<VersionRequirement> {
    requirements.get(tool).cloned()
}
