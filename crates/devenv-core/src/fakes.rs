use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::{
    ActivationPlan, ArchiveExtractor, ArchiveType, Artifact, ArtifactResolver, ChecksumVerifier,
    Clock, CommandInvocation, CommandOutput, CommandRunner, ConfigRepository, CoreError,
    CoreResult, DownloadedArtifact, Downloader, EnvOperation, ExtractionManifest, InstallPlan,
    InstallStore, InstallTransaction, InstallTransactionManager, Installation,
    InstallationMetadata, LockKey, LockManager, Platform, PlatformDetector, RegisteredRuntime,
    RuntimeRegistry, ShimSpec, ShimWriter, ToolAdapter, ToolMetadata, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionScheme, VersionSource,
};

#[derive(Debug, Clone)]
pub struct FakeToolAdapter {
    metadata: ToolMetadata,
    versions: Vec<Version>,
    activation_plan: ActivationPlan,
}

impl FakeToolAdapter {
    pub fn new(name: impl AsRef<str>) -> Self {
        let tool_name = ToolName::new(name).expect("fake tool names should be valid");
        Self {
            metadata: ToolMetadata::new(
                tool_name,
                VersionScheme::Custom("fake".to_owned()),
                vec![],
            ),
            versions: Vec::new(),
            activation_plan: ActivationPlan::new(),
        }
    }

    pub fn with_versions<I, S>(mut self, versions: I) -> CoreResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.versions = versions
            .into_iter()
            .map(Version::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
    }

    pub fn with_exposed_binaries<I, S>(mut self, binaries: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.metadata = ToolMetadata::new(
            self.metadata.name().clone(),
            self.metadata.version_scheme().clone(),
            binaries.into_iter().map(Into::into).collect(),
        );
        self
    }

    pub fn with_activation_plan(mut self, activation_plan: ActivationPlan) -> Self {
        self.activation_plan = activation_plan;
        self
    }
}

impl ToolAdapter for FakeToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        StaticExactVersionMatcher.match_version(requirement, &self.versions)
    }

    fn activation_plan(&self, _runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(self.activation_plan.clone())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticVersionSource {
    versions: HashMap<ToolName, Vec<Version>>,
}

impl StaticVersionSource {
    pub fn add_versions<I, S>(&mut self, tool: ToolName, versions: I) -> CoreResult<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let versions = versions
            .into_iter()
            .map(Version::new)
            .collect::<Result<Vec<_>, _>>()?;
        self.versions.insert(tool, versions);
        Ok(())
    }
}

impl VersionSource for StaticVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        Ok(self.versions.get(tool).cloned().unwrap_or_default())
    }
}

#[derive(Debug, Clone)]
pub struct StaticExactVersionMatcher;

impl VersionMatcher for StaticExactVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        Ok(candidates
            .iter()
            .find(|version| version.raw() == requirement.raw())
            .cloned())
    }
}

#[derive(Debug, Clone)]
pub struct StaticArtifactResolver {
    artifact: Artifact,
}

impl StaticArtifactResolver {
    pub fn new(url: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            artifact: Artifact::new(url, filename, ArchiveType::PlainFile, None),
        }
    }

    pub fn with_artifact(artifact: Artifact) -> Self {
        Self { artifact }
    }
}

impl ArtifactResolver for StaticArtifactResolver {
    fn resolve_artifact(
        &self,
        _tool: &ToolName,
        _version: &Version,
        _platform: Platform,
    ) -> CoreResult<Artifact> {
        Ok(self.artifact.clone())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryInstallStore {
    metadata: Vec<InstallationMetadata>,
}

impl InMemoryInstallStore {
    pub fn installation_metadata(&self) -> &[InstallationMetadata] {
        &self.metadata
    }
}

impl InstallStore for InMemoryInstallStore {
    fn add_installation(&mut self, installation: Installation) -> CoreResult<()> {
        self.metadata.push(InstallationMetadata::new(
            installation,
            "unknown",
            None,
            "unknown",
        ));
        Ok(())
    }

    fn list_installations(&self, tool: &ToolName) -> Vec<Installation> {
        self.metadata
            .iter()
            .map(|metadata| metadata.installation())
            .filter(|installation| installation.tool() == tool)
            .cloned()
            .collect()
    }

    fn add_installation_metadata(&mut self, metadata: InstallationMetadata) -> CoreResult<()> {
        self.metadata.push(metadata);
        Ok(())
    }

    fn list_installation_metadata(&self, tool: &ToolName) -> Vec<InstallationMetadata> {
        self.metadata
            .iter()
            .filter(|metadata| metadata.installation().tool() == tool)
            .cloned()
            .collect()
    }

    fn remove_installation_metadata(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Option<InstallationMetadata>> {
        let Some(index) = self.metadata.iter().position(|metadata| {
            let installation = metadata.installation();
            installation.tool() == tool
                && installation.version() == version
                && installation.platform() == platform
        }) else {
            return Ok(None);
        };

        Ok(Some(self.metadata.remove(index)))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FakeDownloader {
    bytes: Vec<u8>,
    downloads: Vec<PathBuf>,
}

impl FakeDownloader {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            downloads: Vec::new(),
        }
    }

    pub fn downloads(&self) -> &[PathBuf] {
        &self.downloads
    }
}

impl Downloader for FakeDownloader {
    fn download(
        &mut self,
        _artifact: &Artifact,
        destination: &Path,
    ) -> CoreResult<DownloadedArtifact> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::message(format!(
                    "failed to create download directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(destination, &self.bytes).map_err(|error| {
            CoreError::message(format!(
                "failed to write fake download `{}`: {error}",
                destination.display()
            ))
        })?;
        self.downloads.push(destination.to_path_buf());
        Ok(DownloadedArtifact::new(
            destination,
            u64::try_from(self.bytes.len()).unwrap_or(u64::MAX),
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FakeChecksumVerifier {
    failure: Option<String>,
}

impl FakeChecksumVerifier {
    pub fn passing() -> Self {
        Self::default()
    }

    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            failure: Some(message.into()),
        }
    }
}

impl ChecksumVerifier for FakeChecksumVerifier {
    fn verify(&self, artifact_path: &Path, expected_checksum: &str) -> CoreResult<()> {
        if let Some(message) = &self.failure {
            return Err(CoreError::message(format!(
                "checksum mismatch for `{}`: expected {expected_checksum}: {message}",
                artifact_path.display()
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct FakeArchiveExtractor {
    entries: Vec<PathBuf>,
    failure: Option<String>,
    extractions: Vec<PathBuf>,
}

impl FakeArchiveExtractor {
    pub fn with_entries<I, P>(entries: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            entries: entries.into_iter().map(Into::into).collect(),
            failure: None,
            extractions: Vec::new(),
        }
    }

    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            entries: Vec::new(),
            failure: Some(message.into()),
            extractions: Vec::new(),
        }
    }

    pub fn extractions(&self) -> &[PathBuf] {
        &self.extractions
    }
}

impl ArchiveExtractor for FakeArchiveExtractor {
    fn extract(
        &mut self,
        _archive_path: &Path,
        destination: &Path,
        _archive_type: ArchiveType,
    ) -> CoreResult<ExtractionManifest> {
        if let Some(message) = &self.failure {
            return Err(CoreError::message(message.clone()));
        }

        std::fs::create_dir_all(destination).map_err(|error| {
            CoreError::message(format!(
                "failed to create fake extraction root `{}`: {error}",
                destination.display()
            ))
        })?;
        self.extractions.push(destination.to_path_buf());

        Ok(ExtractionManifest::new(self.entries.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct FakeShimWriter {
    shim_dir: PathBuf,
    written: Vec<ShimSpec>,
}

impl FakeShimWriter {
    pub fn new(shim_dir: impl Into<PathBuf>) -> Self {
        Self {
            shim_dir: shim_dir.into(),
            written: Vec::new(),
        }
    }

    pub fn written(&self) -> &[ShimSpec] {
        &self.written
    }
}

impl Default for FakeShimWriter {
    fn default() -> Self {
        Self::new("/devenv/shims")
    }
}

impl ShimWriter for FakeShimWriter {
    fn shim_dir(&self) -> &Path {
        &self.shim_dir
    }

    fn write_shim(&mut self, spec: &ShimSpec) -> CoreResult<()> {
        self.written.push(spec.clone());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FakeInstallTransactionManager {
    root: PathBuf,
    begun: Vec<InstallTransaction>,
    committed: Vec<PathBuf>,
    cleaned: Vec<PathBuf>,
}

impl FakeInstallTransactionManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            begun: Vec::new(),
            committed: Vec::new(),
            cleaned: Vec::new(),
        }
    }

    pub fn begun(&self) -> &[InstallTransaction] {
        &self.begun
    }

    pub fn committed(&self) -> &[PathBuf] {
        &self.committed
    }

    pub fn cleaned(&self) -> &[PathBuf] {
        &self.cleaned
    }

    fn temp_root(&self, plan: &InstallPlan) -> PathBuf {
        self.root.join(".tmp").join(format!(
            "{}-{}-{}",
            plan.tool().as_str(),
            plan.version().raw(),
            plan.platform().id()
        ))
    }
}

impl InstallTransactionManager for FakeInstallTransactionManager {
    fn install_root(&self, tool: &ToolName, version: &Version, platform: Platform) -> PathBuf {
        self.root
            .join("installs")
            .join(tool.as_str())
            .join(version.raw())
            .join(platform.id())
    }

    fn begin(&mut self, plan: &InstallPlan) -> CoreResult<InstallTransaction> {
        let temp_root = self.temp_root(plan);
        if temp_root.exists() {
            std::fs::remove_dir_all(&temp_root).map_err(|error| {
                CoreError::message(format!(
                    "failed to reset fake install temp `{}`: {error}",
                    temp_root.display()
                ))
            })?;
        }
        let download_dir = temp_root.join("download");
        let extract_root = temp_root.join("extract");
        std::fs::create_dir_all(&download_dir).map_err(|error| {
            CoreError::message(format!(
                "failed to create fake install temp `{}`: {error}",
                download_dir.display()
            ))
        })?;
        std::fs::create_dir_all(&extract_root).map_err(|error| {
            CoreError::message(format!(
                "failed to create fake install extract root `{}`: {error}",
                extract_root.display()
            ))
        })?;
        let transaction = InstallTransaction::new(
            plan.install_root(),
            &temp_root,
            download_dir.join(plan.artifact().filename()),
            &extract_root,
        );
        self.begun.push(transaction.clone());

        Ok(transaction)
    }

    fn commit(&mut self, transaction: &InstallTransaction) -> CoreResult<()> {
        if let Some(parent) = transaction.install_root().parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::message(format!(
                    "failed to create fake install parent `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        if transaction.install_root().exists() {
            std::fs::remove_dir_all(transaction.install_root()).map_err(|error| {
                CoreError::message(format!(
                    "failed to replace fake install `{}`: {error}",
                    transaction.install_root().display()
                ))
            })?;
        }
        std::fs::rename(transaction.extract_root(), transaction.install_root()).map_err(
            |error| {
                CoreError::message(format!(
                    "failed to commit fake install `{}` to `{}`: {error}",
                    transaction.extract_root().display(),
                    transaction.install_root().display()
                ))
            },
        )?;
        self.committed
            .push(transaction.install_root().to_path_buf());

        Ok(())
    }

    fn cleanup(&mut self, transaction: &InstallTransaction) -> CoreResult<()> {
        if transaction.temp_root().exists() {
            std::fs::remove_dir_all(transaction.temp_root()).map_err(|error| {
                CoreError::message(format!(
                    "failed to clean fake install temp `{}`: {error}",
                    transaction.temp_root().display()
                ))
            })?;
        }
        self.cleaned.push(transaction.temp_root().to_path_buf());
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRuntimeRegistry {
    runtimes: Vec<RegisteredRuntime>,
}

impl RuntimeRegistry for InMemoryRuntimeRegistry {
    fn add_registered_runtime(&mut self, runtime: RegisteredRuntime) -> CoreResult<()> {
        self.runtimes.push(runtime);
        Ok(())
    }

    fn remove_registered_runtime(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
        root: Option<&Path>,
    ) -> CoreResult<Vec<RegisteredRuntime>> {
        let mut removed = Vec::new();
        self.runtimes.retain(|runtime| {
            let matches = runtime.tool() == tool
                && runtime.version() == version
                && runtime.platform() == platform
                && root.is_none_or(|root| runtime.root() == root);

            if matches {
                removed.push(runtime.clone());
            }

            !matches
        });

        Ok(removed)
    }

    fn list_registered_runtimes(&self, tool: &ToolName) -> Vec<RegisteredRuntime> {
        crate::ports::by_tool(&self.runtimes, tool, RegisteredRuntime::tool)
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryConfigRepository {
    requirements: HashMap<ToolName, VersionRequirement>,
}

impl ConfigRepository for InMemoryConfigRepository {
    fn get_requirement(&self, tool: &ToolName) -> CoreResult<Option<VersionRequirement>> {
        Ok(crate::ports::get_config(&self.requirements, tool))
    }

    fn set_requirement(
        &mut self,
        tool: ToolName,
        requirement: VersionRequirement,
    ) -> CoreResult<()> {
        self.requirements.insert(tool, requirement);
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct FakeActivationRenderer;

impl crate::ActivationRenderer for FakeActivationRenderer {
    fn render(&self, plan: &ActivationPlan) -> CoreResult<String> {
        let lines = plan
            .operations()
            .iter()
            .map(|operation| match operation {
                EnvOperation::Set { key, value } => format!("set {key}={value}"),
                EnvOperation::Unset { key } => format!("unset {key}"),
                EnvOperation::PrependPath { path } => {
                    format!("prepend PATH={}", path.display())
                }
            })
            .collect::<Vec<_>>();

        Ok(lines.join("\n"))
    }
}

#[derive(Debug, Clone)]
pub struct FakePlatformDetector {
    platform: Platform,
}

impl FakePlatformDetector {
    pub fn new(platform: Platform) -> Self {
        Self { platform }
    }
}

impl PlatformDetector for FakePlatformDetector {
    fn current_platform(&self) -> CoreResult<Platform> {
        Ok(self.platform)
    }
}

#[derive(Debug, Clone)]
pub struct FakeCommandRunner {
    invocations: Vec<CommandInvocation>,
    output: CommandOutput,
}

impl Default for FakeCommandRunner {
    fn default() -> Self {
        Self {
            invocations: Vec::new(),
            output: CommandOutput::new(0, "", ""),
        }
    }
}

impl FakeCommandRunner {
    pub fn with_output(mut self, output: CommandOutput) -> Self {
        self.output = output;
        self
    }

    pub fn invocations(&self) -> &[CommandInvocation] {
        &self.invocations
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(&mut self, invocation: CommandInvocation) -> CoreResult<CommandOutput> {
        self.invocations.push(invocation);
        Ok(self.output.clone())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryLockManager {
    acquired: HashSet<LockKey>,
}

impl LockManager for InMemoryLockManager {
    fn acquire(&mut self, key: LockKey) -> CoreResult<bool> {
        Ok(self.acquired.insert(key))
    }

    fn release(&mut self, key: &LockKey) -> CoreResult<()> {
        if self.acquired.remove(key) {
            Ok(())
        } else {
            Err(CoreError::message(format!(
                "cannot release lock `{}` because it is not acquired",
                key.as_str()
            )))
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaticClock {
    now: String,
}

impl StaticClock {
    pub fn new(now: impl Into<String>) -> Self {
        Self { now: now.into() }
    }
}

impl Clock for StaticClock {
    fn now_utc(&self) -> CoreResult<String> {
        Ok(self.now.clone())
    }
}
