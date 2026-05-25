use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform, ProviderId,
    RegisteredRuntime, RemoteRelease, RemoteReleaseIndex, ResolvedArtifact, RuntimeRegistry,
    ToolAdapter, ToolDistribution, ToolMetadata, ToolName, Version, VersionMatcher,
    VersionRequirement, VersionScheme, VersionSource,
};
use serde::Deserialize;

pub const JAVA_TEMURIN_METADATA_URL_HINT: &str =
    "https://api.adoptium.net/v3/assets/feature_releases/{feature}/ga";

#[derive(Debug, Clone)]
pub struct JavaToolAdapter {
    metadata: ToolMetadata,
}

impl JavaToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                java_tool_name(),
                VersionScheme::JavaFeatureInterimUpdate,
                vec![
                    "java".to_owned(),
                    "javac".to_owned(),
                    "jar".to_owned(),
                    "javadoc".to_owned(),
                ],
            ),
        }
    }
}

impl Default for JavaToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for JavaToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(requirement.raw())?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new()
            .set_env("JAVA_HOME", runtime_root.to_string_lossy().into_owned())
            .prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaVersionMatcher;

impl VersionMatcher for JavaVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_java_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    version: Version,
    root: PathBuf,
    distribution: ToolDistribution,
    source: JavaRuntimeSource,
    platform: Option<Platform>,
}

impl JavaRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        distribution: ToolDistribution,
        source: JavaRuntimeSource,
        platform: Option<Platform>,
    ) -> Self {
        Self {
            version,
            root: root.into(),
            distribution,
            source,
            platform,
        }
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn distribution(&self) -> &ToolDistribution {
        &self.distribution
    }

    pub fn source(&self) -> &JavaRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl JavaRuntimeDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_candidate_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.candidate_roots.push(root.into());
        self
    }

    pub fn with_candidate_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.candidate_roots
            .extend(roots.into_iter().map(Into::into));
        self
    }

    pub fn discover(
        &self,
        platform: Platform,
        registry: &dyn RuntimeRegistry,
        install_store: &dyn InstallStore,
    ) -> CoreResult<Vec<JavaRuntime>> {
        let java = java_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&java) {
            if runtime.platform() == platform {
                runtimes.push(java_runtime_from_registered(runtime));
            }
        }

        for metadata in install_store.list_installation_metadata(&java) {
            if metadata.installation().platform() == platform {
                runtimes.push(java_runtime_from_installation_metadata(metadata));
            }
        }

        for candidate in &self.candidate_roots {
            runtimes.extend(discover_candidate_root(candidate)?);
        }

        runtimes.sort_by(runtime_sort);
        runtimes.dedup_by(|left, right| left.root == right.root);

        Ok(runtimes)
    }
}

pub fn validate_jdk_home(root: impl AsRef<Path>) -> CoreResult<JavaRuntime> {
    let root = root.as_ref();

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Java runtime `{}`: expected a JDK home directory",
            root.display()
        )));
    }

    for binary in ["java", "javac"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Java runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let release_path = root.join("release");
    let release = std::fs::read_to_string(&release_path).map_err(|error| {
        CoreError::message(format!(
            "invalid Java runtime `{}`: failed to read `{}` for JAVA_VERSION metadata: {error}",
            root.display(),
            release_path.display()
        ))
    })?;
    let metadata = parse_jdk_release(&release);
    let version = metadata.get("JAVA_VERSION").ok_or_else(|| {
        CoreError::message(format!(
            "invalid Java runtime `{}`: missing JAVA_VERSION in `{}`",
            root.display(),
            release_path.display()
        ))
    })?;
    let distribution = metadata
        .get("IMPLEMENTOR")
        .map(ToolDistribution::named)
        .unwrap_or(ToolDistribution::Unknown);

    Ok(JavaRuntime::new(
        Version::new(version)?,
        root,
        distribution,
        JavaRuntimeSource::CandidatePath,
        None,
    ))
}

pub fn match_java_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [JavaRuntime],
) -> CoreResult<Option<&'a JavaRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_java_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_java_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = JavaVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = JavaVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_java_version(value: &str) -> CoreResult<String> {
    let key = JavaVersionKey::parse(value)?;
    Ok(key.to_normalized_string())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JavaDistribution(String);

impl JavaDistribution {
    pub fn temurin() -> Self {
        Self("temurin".to_owned())
    }

    pub fn named(value: impl AsRef<str>) -> CoreResult<Self> {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            return Err(CoreError::message(
                "invalid Java distribution: expected a non-empty distribution name",
            ));
        }

        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JavaDistribution {
    fn default() -> Self {
        Self::temurin()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaReleaseMetadata {
    releases: Vec<JavaRelease>,
}

impl JavaReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse Java release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid Java release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_java_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[JavaRelease] {
        &self.releases
    }

    pub fn from_release_index(index: &RemoteReleaseIndex) -> CoreResult<Self> {
        if index.tool().as_str() != "java" {
            return Err(CoreError::message(format!(
                "Java release metadata cannot be built from `{}` index",
                index.tool()
            )));
        }

        let releases = index
            .releases()
            .iter()
            .map(java_release_from_remote_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    fn release_for_requirement(
        &self,
        requirement: &Version,
        distribution: &JavaDistribution,
    ) -> CoreResult<&JavaRelease> {
        let mut releases = self
            .releases()
            .iter()
            .filter(|release| release.distribution() == distribution && release.stable())
            .collect::<Vec<_>>();

        if releases.is_empty() {
            return Err(CoreError::message(format!(
                "Java distribution `{}` was not found in release metadata",
                distribution.as_str()
            )));
        }

        if let Some(exact) = releases
            .iter()
            .find(|release| release.version().raw() == requirement.raw())
        {
            return Ok(exact);
        }

        let requirement_key = JavaVersionKey::parse(requirement.raw())?;
        releases.retain(|release| {
            JavaVersionKey::parse(release.version().raw())
                .map(|candidate| candidate.matches_requirement(&requirement_key))
                .unwrap_or(false)
        });
        releases.sort_by(|left, right| compare_java_versions_desc(left.version(), right.version()));

        releases.first().copied().ok_or_else(|| {
            CoreError::message(format!(
                "Java {} ({}) was not found in release metadata",
                requirement,
                distribution.as_str()
            ))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaTemurinReleaseMetadata {
    index: RemoteReleaseIndex,
}

impl JavaTemurinReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let releases =
            serde_json::from_str::<Vec<AdoptiumReleasePayload>>(input).map_err(|error| {
                CoreError::message(format!("failed to parse Java Temurin metadata: {error}"))
            })?;
        let tool = java_tool_name();
        let provider =
            ProviderId::new("temurin").expect("built-in Java provider id should be valid");
        let releases = releases
            .into_iter()
            .map(|release| java_remote_release_from_adoptium_payload(&tool, &provider, release))
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self {
            index: RemoteReleaseIndex::new(tool, provider, releases),
        })
    }

    pub fn release_index(&self) -> &RemoteReleaseIndex {
        &self.index
    }

    pub fn into_release_index(self) -> RemoteReleaseIndex {
        self.index
    }

    pub fn into_release_metadata(self) -> CoreResult<JavaReleaseMetadata> {
        JavaReleaseMetadata::from_release_index(&self.index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRelease {
    version: Version,
    feature: u32,
    distribution: JavaDistribution,
    stable: bool,
    files: Vec<JavaReleaseFile>,
}

impl JavaRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn feature(&self) -> u32 {
        self.feature
    }

    pub fn distribution(&self) -> &JavaDistribution {
        &self.distribution
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[JavaReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl JavaReleaseFile {
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn os(&self) -> &str {
        &self.os
    }

    pub fn arch(&self) -> &str {
        &self.arch
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct JavaReleaseVersionSource {
    metadata: JavaReleaseMetadata,
    distribution: JavaDistribution,
}

impl JavaReleaseVersionSource {
    pub fn new(metadata: JavaReleaseMetadata) -> Self {
        Self::with_distribution(metadata, JavaDistribution::temurin())
    }

    pub fn with_distribution(
        metadata: JavaReleaseMetadata,
        distribution: JavaDistribution,
    ) -> Self {
        Self {
            metadata,
            distribution,
        }
    }

    pub fn distribution(&self) -> &JavaDistribution {
        &self.distribution
    }
}

impl VersionSource for JavaReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "java" {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        for release in self
            .metadata
            .releases()
            .iter()
            .filter(|release| release.stable() && release.distribution() == &self.distribution)
        {
            versions.push(Version::new(release.feature().to_string()).map_err(CoreError::from)?);
            versions.push(release.version().clone());
        }
        versions.sort_by(compare_java_versions_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct JavaArtifactResolver {
    metadata: JavaReleaseMetadata,
    distribution: JavaDistribution,
}

impl JavaArtifactResolver {
    pub fn new(metadata: JavaReleaseMetadata) -> Self {
        Self::with_distribution(metadata, JavaDistribution::temurin())
    }

    pub fn with_distribution(
        metadata: JavaReleaseMetadata,
        distribution: JavaDistribution,
    ) -> Self {
        Self {
            metadata,
            distribution,
        }
    }

    pub fn distribution(&self) -> &JavaDistribution {
        &self.distribution
    }

    pub fn resolve_install_version(&self, requirement: &Version) -> CoreResult<Version> {
        let release = self
            .metadata
            .release_for_requirement(requirement, &self.distribution)?;
        Version::new(format!(
            "{}-{}",
            release.version().raw(),
            release.distribution().as_str()
        ))
        .map_err(CoreError::from)
    }
}

impl ArtifactResolver for JavaArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "java" {
            return Err(CoreError::message(format!(
                "Java artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self
            .metadata
            .release_for_requirement(version, &self.distribution)?;
        let os = java_artifact_os(platform);
        let arch = java_artifact_arch(platform);
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "jdk" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Java {} ({}) does not provide a JDK archive for {}",
                    release.version(),
                    release.distribution().as_str(),
                    platform.id()
                ))
            })?;
        let archive_type = archive_type_for_java_file(file.filename())?;
        let url = file.url().map(ToOwned::to_owned).unwrap_or_else(|| {
            format!(
                "https://api.adoptium.net/v3/binary/version/{}/{}",
                release.version().raw(),
                file.filename()
            )
        });
        let mut artifact = Artifact::new(
            url,
            file.filename(),
            archive_type,
            file.sha256().map(ToOwned::to_owned),
        );
        if let Some(size) = file.size() {
            artifact = artifact.with_size(size);
        }

        Ok(artifact)
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaInstalledRuntimeValidator;

impl InstalledRuntimeValidator for JavaInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_jdk_home(root).map(|_| ())
    }
}

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<JavaRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_jdk_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Java candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Java candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_jdk_home(&path) {
            runtimes.push(runtime);
            continue;
        }

        let macos_home = path.join("Contents/Home");
        if let Ok(runtime) = validate_jdk_home(macos_home) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn java_runtime_from_registered(runtime: RegisteredRuntime) -> JavaRuntime {
    JavaRuntime::new(
        runtime.version().clone(),
        runtime.root(),
        ToolDistribution::Unknown,
        JavaRuntimeSource::Registered,
        Some(runtime.platform()),
    )
}

fn java_runtime_from_installation_metadata(
    metadata: devenv_core::InstallationMetadata,
) -> JavaRuntime {
    let distribution = metadata
        .metadata_field("distribution")
        .map(ToolDistribution::named)
        .unwrap_or(ToolDistribution::Unknown);
    java_runtime_from_installation_with_distribution(metadata.installation().clone(), distribution)
}

fn java_runtime_from_installation_with_distribution(
    installation: Installation,
    distribution: ToolDistribution,
) -> JavaRuntime {
    JavaRuntime::new(
        installation.version().clone(),
        installation.root(),
        distribution,
        JavaRuntimeSource::Installed,
        Some(installation.platform()),
    )
}

fn parse_java_release(value: &toml::Value) -> CoreResult<JavaRelease> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Java release metadata: release must be a table")
    })?;
    let version = normalize_java_version(required_java_string(table, "version")?)?;
    let feature = table
        .get("feature")
        .and_then(toml::Value::as_integer)
        .map(|value| {
            u32::try_from(value).map_err(|_| {
                CoreError::message(format!(
                    "invalid Java release metadata: feature for `{version}` must be non-negative"
                ))
            })
        })
        .transpose()?
        .unwrap_or(JavaVersionKey::parse(&version)?.feature());
    let distribution = table
        .get("distribution")
        .and_then(toml::Value::as_str)
        .map(JavaDistribution::named)
        .transpose()?
        .unwrap_or_default();
    let stable = table
        .get("stable")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    let files = table
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Java release metadata: release `{version}` has no [[release.file]] entries"
            ))
        })?
        .iter()
        .map(parse_java_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(JavaRelease {
        version: Version::new(version).map_err(CoreError::from)?,
        feature,
        distribution,
        stable,
        files,
    })
}

fn parse_java_release_file(value: &toml::Value) -> CoreResult<JavaReleaseFile> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Java release metadata: release file must be a table")
    })?;
    let filename = required_java_string(table, "filename")?.to_owned();
    let os = required_java_string(table, "os")?.to_owned();
    let arch = required_java_string(table, "arch")?.to_owned();
    let kind = table
        .get("kind")
        .or_else(|| table.get("package_type"))
        .and_then(toml::Value::as_str)
        .unwrap_or("jdk")
        .to_ascii_lowercase();
    let sha256 = table
        .get("sha256")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);
    let size = table
        .get("size")
        .and_then(toml::Value::as_integer)
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                CoreError::message(format!(
                    "invalid Java release metadata: size for `{filename}` must be non-negative"
                ))
            })
        })
        .transpose()?;
    let url = table
        .get("url")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);

    Ok(JavaReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256,
        size,
        url,
    })
}

#[derive(Debug, Deserialize)]
struct AdoptiumReleasePayload {
    #[serde(default)]
    release_name: Option<String>,
    #[serde(default)]
    release_type: Option<String>,
    #[serde(default)]
    version_data: Option<AdoptiumVersionData>,
    #[serde(default)]
    binaries: Vec<AdoptiumBinaryPayload>,
}

#[derive(Debug, Deserialize)]
struct AdoptiumVersionData {
    #[serde(default)]
    major: Option<u32>,
    #[serde(default)]
    openjdk_version: Option<String>,
    #[serde(default)]
    semver: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinaryPayload {
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    image_type: Option<String>,
    #[serde(default)]
    package: Option<AdoptiumPackagePayload>,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackagePayload {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

fn java_remote_release_from_adoptium_payload(
    tool: &ToolName,
    provider: &ProviderId,
    release: AdoptiumReleasePayload,
) -> CoreResult<RemoteRelease> {
    let version_raw = java_adoptium_release_version(&release)?;
    let normalized = normalize_java_version(&version_raw)?;
    let version = Version::new(&normalized)?;
    let feature = release
        .version_data
        .as_ref()
        .and_then(|data| data.major)
        .unwrap_or(JavaVersionKey::parse(&normalized)?.feature());
    let stable = release
        .release_type
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case("ga"))
        .unwrap_or(true);
    let artifacts = release
        .binaries
        .into_iter()
        .filter_map(|binary| {
            java_remote_artifact_from_adoptium_binary(tool, provider, &version, binary)
        })
        .collect::<CoreResult<Vec<_>>>()?;

    let mut remote_release = RemoteRelease::new(version, artifacts)
        .with_metadata_field("feature", feature.to_string())
        .with_metadata_field("distribution", "temurin")
        .with_metadata_field("stable", stable.to_string())
        .with_metadata_field("image_type", "jdk")
        .with_metadata_field("package_type", "archive")
        .with_metadata_field("upstream_version", version_raw);
    if let Some(release_name) = release.release_name {
        remote_release = remote_release.with_metadata_field("release_name", release_name);
    }

    Ok(remote_release)
}

fn java_adoptium_release_version(release: &AdoptiumReleasePayload) -> CoreResult<String> {
    if let Some(version_data) = release.version_data.as_ref() {
        for candidate in [
            version_data.openjdk_version.as_deref(),
            version_data.semver.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !candidate.trim().is_empty() {
                return Ok(candidate.trim().to_owned());
            }
        }
    }

    release
        .release_name
        .as_deref()
        .and_then(|name| name.trim().strip_prefix("jdk-").or(Some(name.trim())))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CoreError::message(
                "invalid Java Temurin metadata: release is missing version_data.openjdk_version",
            )
        })
}

fn java_remote_artifact_from_adoptium_binary(
    tool: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    binary: AdoptiumBinaryPayload,
) -> Option<CoreResult<ResolvedArtifact>> {
    let image_type = binary
        .image_type
        .as_deref()
        .unwrap_or("jdk")
        .to_ascii_lowercase();
    if image_type != "jdk" {
        return None;
    }

    let os = binary.os.as_deref()?;
    let arch = binary.architecture.as_deref()?;
    let (java_os, platform_os) = java_adoptium_os(os)?;
    let (java_arch, platform_arch) = java_adoptium_arch(arch)?;
    let platform = Platform::new(platform_os, platform_arch);
    let package = binary.package?;

    let result = (|| {
        let filename = package
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::message("invalid Java Temurin metadata: JDK package is missing name")
            })?;
        let url = package
            .link
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "invalid Java Temurin metadata: archive `{filename}` is missing link"
                ))
            })?;
        let checksum = package
            .checksum
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "invalid Java Temurin metadata: archive `{filename}` is missing checksum"
                ))
            })?;
        let archive_type = archive_type_for_java_file(filename)?;
        let mut artifact = Artifact::new(
            url.to_owned(),
            filename.to_owned(),
            archive_type,
            Some(normalized_sha256_checksum(checksum)),
        );
        if let Some(size) = package.size {
            artifact = artifact.with_size(size);
        }

        Ok(ResolvedArtifact::new(
            tool.clone(),
            provider.clone(),
            release_version.clone(),
            platform,
            artifact,
        )
        .with_metadata_field("filename", filename.to_owned())
        .with_metadata_field("kind", "jdk")
        .with_metadata_field("java_os", java_os)
        .with_metadata_field("java_arch", java_arch)
        .with_metadata_field("distribution", "temurin")
        .with_metadata_field("image_type", "jdk")
        .with_metadata_field("package_type", "archive"))
    })();

    Some(result)
}

fn java_adoptium_os(value: &str) -> Option<(&'static str, OperatingSystem)> {
    match value.to_ascii_lowercase().as_str() {
        "mac" | "macos" | "darwin" => Some(("macos", OperatingSystem::Macos)),
        "linux" => Some(("linux", OperatingSystem::Linux)),
        "windows" => Some(("windows", OperatingSystem::Windows)),
        _ => None,
    }
}

fn java_adoptium_arch(value: &str) -> Option<(&'static str, Architecture)> {
    match value.to_ascii_lowercase().as_str() {
        "x64" | "amd64" => Some(("x64", Architecture::X64)),
        "aarch64" | "arm64" => Some(("arm64", Architecture::Arm64)),
        _ => None,
    }
}

fn normalized_sha256_checksum(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("sha256:") {
        trimmed.to_owned()
    } else {
        format!("sha256:{trimmed}")
    }
}

fn java_release_from_remote_release(release: &RemoteRelease) -> CoreResult<JavaRelease> {
    let feature = release
        .metadata_field("feature")
        .map(|value| {
            value.parse::<u32>().map_err(|error| {
                CoreError::message(format!(
                    "invalid Java Temurin metadata: feature `{value}` is not numeric: {error}"
                ))
            })
        })
        .transpose()?
        .unwrap_or(JavaVersionKey::parse(release.version().raw())?.feature());
    let distribution = release
        .metadata_field("distribution")
        .map(JavaDistribution::named)
        .transpose()?
        .unwrap_or_default();
    let stable = release.metadata_field("stable") != Some("false");
    let files = release
        .artifacts()
        .iter()
        .map(java_release_file_from_remote_artifact)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(JavaRelease {
        version: release.version().clone(),
        feature,
        distribution,
        stable,
        files,
    })
}

fn java_release_file_from_remote_artifact(
    resolved: &ResolvedArtifact,
) -> CoreResult<JavaReleaseFile> {
    let artifact = resolved.artifact();
    let filename = resolved
        .metadata_field("filename")
        .unwrap_or_else(|| artifact.filename())
        .to_owned();
    let os = resolved
        .metadata_field("java_os")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| java_artifact_os(resolved.platform()).to_owned());
    let arch = resolved
        .metadata_field("java_arch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| java_artifact_arch(resolved.platform()).to_owned());
    let kind = resolved.metadata_field("kind").unwrap_or("jdk").to_owned();

    Ok(JavaReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256: artifact.checksum().map(ToOwned::to_owned),
        size: artifact.size(),
        url: Some(artifact.url().to_owned()),
    })
}

fn required_java_string<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
) -> CoreResult<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CoreError::message(format!("invalid Java release metadata: missing `{key}`"))
        })
}

fn java_artifact_os(platform: Platform) -> &'static str {
    match platform.os() {
        OperatingSystem::Macos => "macos",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Windows => "windows",
    }
}

fn java_artifact_arch(platform: Platform) -> &'static str {
    match platform.architecture() {
        Architecture::X64 => "x64",
        Architecture::Arm64 => "arm64",
    }
}

fn archive_type_for_java_file(filename: &str) -> CoreResult<ArchiveType> {
    if filename.ends_with(".tar.gz") {
        Ok(ArchiveType::TarGz)
    } else if filename.ends_with(".zip") {
        Ok(ArchiveType::Zip)
    } else {
        Err(CoreError::message(format!(
            "unsupported Java archive `{filename}`: expected .tar.gz or .zip"
        )))
    }
}

fn compare_java_versions_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = JavaVersionKey::parse(left.raw());
    let right_key = JavaVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left_key), Ok(right_key)) => right_key.cmp(&left_key),
        _ => right.raw().cmp(left.raw()),
    }
}

fn parse_jdk_release(input: &str) -> BTreeMap<String, String> {
    input
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_owned(), unquote_release_value(value.trim())))
        })
        .collect()
}

fn unquote_release_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JavaVersionKey {
    components: Vec<u32>,
}

impl JavaVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let mut components = Vec::new();
        let mut current = String::new();

        for character in value.trim().chars() {
            if character == '+'
                || (character == '-' && (!current.is_empty() || !components.is_empty()))
            {
                if !current.is_empty() {
                    components.push(current.parse::<u32>().map_err(|error| {
                        CoreError::message(format!("invalid Java version `{value}`: {error}"))
                    })?);
                    current.clear();
                }
                break;
            }
            if character.is_ascii_digit() {
                current.push(character);
            } else if !current.is_empty() {
                components.push(current.parse::<u32>().map_err(|error| {
                    CoreError::message(format!("invalid Java version `{value}`: {error}"))
                })?);
                current.clear();
            }
        }

        if !current.is_empty() {
            components.push(current.parse::<u32>().map_err(|error| {
                CoreError::message(format!("invalid Java version `{value}`: {error}"))
            })?);
        }

        if components.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Java version `{value}`: expected a numeric version"
            )));
        }

        if components.first() == Some(&1) && components.get(1) == Some(&8) {
            components.remove(0);
        }

        Ok(Self { components })
    }

    fn matches_requirement(&self, requirement: &JavaVersionKey) -> bool {
        self.components.starts_with(&requirement.components)
    }

    fn feature(&self) -> u32 {
        self.components[0]
    }

    fn to_normalized_string(&self) -> String {
        self.components
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl Ord for JavaVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components.cmp(&other.components)
    }
}

impl PartialOrd for JavaVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn runtime_sort(left: &JavaRuntime, right: &JavaRuntime) -> Ordering {
    left.root.cmp(&right.root)
}

fn java_tool_name() -> ToolName {
    ToolName::new("java").expect("built-in Java tool name should be valid")
}
