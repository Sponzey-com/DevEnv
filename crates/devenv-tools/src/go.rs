use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform, ProviderId,
    RegisteredRuntime, RemoteRelease, RemoteReleaseIndex, ResolvedArtifact, RuntimeRegistry,
    ToolAdapter, ToolMetadata, ToolName, Version, VersionMatcher, VersionRequirement,
    VersionScheme, VersionSource,
};
use serde::Deserialize;

pub const GO_OFFICIAL_METADATA_URL: &str = "https://go.dev/dl/?mode=json&include=all";

#[derive(Debug, Clone)]
pub struct GoToolAdapter {
    metadata: ToolMetadata,
}

impl GoToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                go_tool_name(),
                VersionScheme::GoRelease,
                vec!["go".to_owned(), "gofmt".to_owned()],
            ),
        }
    }
}

impl Default for GoToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for GoToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_go_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new()
            .set_env("GOROOT", runtime_root.to_string_lossy().into_owned())
            .prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct GoVersionMatcher;

impl VersionMatcher for GoVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_go_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoRuntime {
    version: Version,
    root: PathBuf,
    source: GoRuntimeSource,
    platform: Option<Platform>,
}

impl GoRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        source: GoRuntimeSource,
        platform: Option<Platform>,
    ) -> Self {
        Self {
            version,
            root: root.into(),
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

    pub fn source(&self) -> &GoRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct GoRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl GoRuntimeDiscovery {
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
    ) -> CoreResult<Vec<GoRuntime>> {
        let go = go_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&go) {
            if runtime.platform() == platform {
                runtimes.push(go_runtime_from_registered(runtime));
            }
        }

        for installation in install_store.list_installations(&go) {
            if installation.platform() == platform {
                runtimes.push(go_runtime_from_installation(installation)?);
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

pub fn validate_go_sdk_home(root: impl AsRef<Path>) -> CoreResult<GoRuntime> {
    let root = root.as_ref();
    let root = canonical_go_sdk_home(root);
    let root = root.as_path();

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Go runtime `{}`: expected a Go SDK root directory",
            root.display()
        )));
    }

    for binary in ["go", "gofmt"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Go runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version_path = root.join("VERSION");
    let version = std::fs::read_to_string(&version_path).map_err(|error| {
        CoreError::message(format!(
            "invalid Go runtime `{}`: failed to read `{}` for version metadata: {error}",
            root.display(),
            version_path.display()
        ))
    })?;
    let version = version
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Go runtime `{}`: missing version in `{}`",
                root.display(),
                version_path.display()
            ))
        })?;
    let version = normalize_go_version(version)?;

    Ok(GoRuntime::new(
        Version::new(version)?,
        root,
        GoRuntimeSource::CandidatePath,
        None,
    ))
}

#[derive(Debug, Clone, Default)]
pub struct GoInstalledRuntimeValidator;

impl InstalledRuntimeValidator for GoInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_go_sdk_home(root).map(|_| ())
    }
}

pub fn match_go_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [GoRuntime],
) -> CoreResult<Option<&'a GoRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_go_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_go_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = GoVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = GoVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_go_version(value: &str) -> CoreResult<String> {
    let key = GoVersionKey::parse(value)?;
    Ok(key.to_normalized_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoReleaseMetadata {
    releases: Vec<GoRelease>,
}

impl GoReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse Go release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid Go release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_go_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[GoRelease] {
        &self.releases
    }

    pub fn from_release_index(index: &RemoteReleaseIndex) -> CoreResult<Self> {
        if index.tool().as_str() != "go" {
            return Err(CoreError::message(format!(
                "Go release metadata cannot be built from `{}` index",
                index.tool()
            )));
        }

        let releases = index
            .releases()
            .iter()
            .map(go_release_from_remote_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    fn release_for_version(&self, version: &Version) -> CoreResult<&GoRelease> {
        let normalized = normalize_go_version(version.raw())?;
        self.releases
            .iter()
            .find(|release| release.version().raw() == normalized)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Go version `{}` was not found in metadata",
                    version
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoOfficialReleaseMetadata {
    index: RemoteReleaseIndex,
}

impl GoOfficialReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let releases =
            serde_json::from_str::<Vec<GoOfficialReleasePayload>>(input).map_err(|error| {
                CoreError::message(format!("failed to parse Go official metadata: {error}"))
            })?;
        let tool = go_tool_name();
        let provider =
            ProviderId::new("official").expect("built-in Go provider id should be valid");
        let releases = releases
            .into_iter()
            .filter(|release| normalize_go_version(&release.version).is_ok())
            .map(|release| go_remote_release_from_official_payload(&tool, &provider, release))
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

    pub fn into_release_metadata(self) -> CoreResult<GoReleaseMetadata> {
        GoReleaseMetadata::from_release_index(&self.index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoCatalogReleaseMetadata {
    index: RemoteReleaseIndex,
}

impl GoCatalogReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let payload = serde_json::from_str::<GoCatalogPayload>(input).map_err(|error| {
            CoreError::message(format!("failed to parse Go catalog metadata: {error}"))
        })?;
        if payload.schema_version != 1 {
            return Err(CoreError::message(format!(
                "unsupported Go catalog metadata schema version {}: expected 1",
                payload.schema_version
            )));
        }
        if payload.tool != "go" {
            return Err(CoreError::message(format!(
                "Go catalog metadata cannot parse tool `{}`",
                payload.tool
            )));
        }
        if payload.provider != "official" {
            return Err(CoreError::message(format!(
                "Go catalog metadata cannot parse provider `{}`",
                payload.provider
            )));
        }

        let tool = go_tool_name();
        let provider =
            ProviderId::new("official").expect("built-in Go provider id should be valid");
        let releases = payload
            .releases
            .into_iter()
            .map(|release| go_remote_release_from_catalog_payload(&tool, &provider, release))
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

    pub fn into_release_metadata(self) -> CoreResult<GoReleaseMetadata> {
        GoReleaseMetadata::from_release_index(&self.index)
    }
}

#[derive(Debug, Clone)]
pub struct GoRemoteReleaseVersionSource {
    index: RemoteReleaseIndex,
}

impl GoRemoteReleaseVersionSource {
    pub fn new(index: RemoteReleaseIndex) -> Self {
        Self { index }
    }
}

impl VersionSource for GoRemoteReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "go" {
            return Ok(Vec::new());
        }

        let mut versions = self
            .index
            .releases()
            .iter()
            .filter(|release| !release.artifacts().is_empty())
            .filter(|release| release.metadata_field("stable") != Some("false"))
            .filter(|release| release.metadata_field("yanked") != Some("true"))
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_go_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct GoRemoteArtifactResolver {
    index: RemoteReleaseIndex,
}

impl GoRemoteArtifactResolver {
    pub fn new(index: RemoteReleaseIndex) -> Self {
        Self { index }
    }
}

impl ArtifactResolver for GoRemoteArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "go" {
            return Err(CoreError::message(format!(
                "Go artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self.index.release_for_version(version).ok_or_else(|| {
            CoreError::message(format!("Go version `{version}` was not found in metadata"))
        })?;
        let resolved = release
            .artifacts()
            .iter()
            .find(|artifact| artifact.platform() == platform)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Go {} does not provide an archive for {}",
                    version,
                    platform.id()
                ))
            })?;

        Ok(resolved.artifact().clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoRelease {
    version: Version,
    stable: bool,
    files: Vec<GoReleaseFile>,
}

impl GoRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[GoReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl GoReleaseFile {
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
pub struct GoReleaseVersionSource {
    metadata: GoReleaseMetadata,
}

impl GoReleaseVersionSource {
    pub fn new(metadata: GoReleaseMetadata) -> Self {
        Self { metadata }
    }
}

impl VersionSource for GoReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "go" {
            return Ok(Vec::new());
        }

        let mut versions = self
            .metadata
            .releases()
            .iter()
            .filter(|release| release.files().iter().any(|file| file.kind() == "archive"))
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_go_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct GoArtifactResolver {
    metadata: GoReleaseMetadata,
}

impl GoArtifactResolver {
    pub fn new(metadata: GoReleaseMetadata) -> Self {
        Self { metadata }
    }
}

impl ArtifactResolver for GoArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "go" {
            return Err(CoreError::message(format!(
                "Go artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self.metadata.release_for_version(version)?;
        let os = go_artifact_os(platform)?;
        let arch = go_artifact_arch(platform)?;
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "archive" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Go {} does not provide an archive for {}",
                    version,
                    platform.id()
                ))
            })?;
        let archive_type = archive_type_for_go_file(file.filename())?;
        let url = file
            .url()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("https://go.dev/dl/{}", file.filename()));
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

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<GoRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_go_sdk_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Go candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Go candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_go_sdk_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn go_runtime_from_registered(runtime: RegisteredRuntime) -> GoRuntime {
    GoRuntime::new(
        runtime.version().clone(),
        runtime.root(),
        GoRuntimeSource::Registered,
        Some(runtime.platform()),
    )
}

fn go_runtime_from_installation(installation: Installation) -> CoreResult<GoRuntime> {
    let root = canonical_go_sdk_home(installation.root());
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_go_sdk_home(&root)?.version().clone()
    };

    Ok(GoRuntime::new(
        version,
        root,
        GoRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_go_sdk_home(root: &Path) -> PathBuf {
    let nested = root.join("go");
    if !root.join("bin/go").is_file() && nested.join("bin/go").is_file() {
        nested
    } else {
        root.to_path_buf()
    }
}

fn parse_go_release(value: &toml::Value) -> CoreResult<GoRelease> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Go release metadata: release must be a table")
    })?;
    let version = normalize_go_version(required_string(table, "version")?)?;
    let stable = table
        .get("stable")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    let files = table
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Go release metadata: release `{version}` has no [[release.file]] entries"
            ))
        })?
        .iter()
        .map(parse_go_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(GoRelease {
        version: Version::new(version)?,
        stable,
        files,
    })
}

fn parse_go_release_file(value: &toml::Value) -> CoreResult<GoReleaseFile> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Go release metadata: release file must be a table")
    })?;
    let filename = required_string(table, "filename")?.to_owned();
    let os = required_string(table, "os")?.to_owned();
    let arch = required_string(table, "arch")?.to_owned();
    let kind = table
        .get("kind")
        .and_then(toml::Value::as_str)
        .unwrap_or("archive")
        .to_owned();
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
                    "invalid Go release metadata: size for `{filename}` must be non-negative"
                ))
            })
        })
        .transpose()?;
    let url = table
        .get("url")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);

    Ok(GoReleaseFile {
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
struct GoOfficialReleasePayload {
    version: String,
    stable: bool,
    #[serde(default)]
    files: Vec<GoOfficialReleaseFilePayload>,
}

#[derive(Debug, Deserialize)]
struct GoOfficialReleaseFilePayload {
    filename: String,
    os: String,
    arch: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoCatalogPayload {
    schema_version: u32,
    tool: String,
    provider: String,
    #[serde(default)]
    releases: Vec<GoCatalogReleasePayload>,
}

#[derive(Debug, Deserialize)]
struct GoCatalogReleasePayload {
    version: String,
    #[serde(default = "default_true")]
    stable: bool,
    #[serde(default)]
    yanked: bool,
    #[serde(default)]
    yanked_reason: Option<String>,
    #[serde(default)]
    upstream_version: Option<String>,
    #[serde(default)]
    artifacts: Vec<GoCatalogArtifactPayload>,
}

#[derive(Debug, Deserialize)]
struct GoCatalogArtifactPayload {
    filename: String,
    os: String,
    arch: String,
    url: String,
    checksum: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default = "default_archive_kind")]
    kind: String,
    #[serde(default = "default_true")]
    installable: bool,
}

fn default_true() -> bool {
    true
}

fn default_archive_kind() -> String {
    "archive".to_owned()
}

fn go_remote_release_from_official_payload(
    tool: &ToolName,
    provider: &ProviderId,
    release: GoOfficialReleasePayload,
) -> CoreResult<RemoteRelease> {
    let normalized = normalize_go_version(&release.version)?;
    let version = Version::new(&normalized)?;
    let stable = release.stable.to_string();
    let artifacts = release
        .files
        .into_iter()
        .filter(|file| file.kind.as_deref().unwrap_or("archive") == "archive")
        .filter_map(|file| go_remote_artifact_from_official_file(tool, provider, &version, file))
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(RemoteRelease::new(version, artifacts)
        .with_metadata_field("upstream_version", release.version)
        .with_metadata_field("stable", stable))
}

fn go_remote_release_from_catalog_payload(
    tool: &ToolName,
    provider: &ProviderId,
    release: GoCatalogReleasePayload,
) -> CoreResult<RemoteRelease> {
    let normalized = normalize_go_version(&release.version)?;
    let version = Version::new(&normalized)?;
    let stable = release.stable.to_string();
    let yanked = release.yanked.to_string();
    let artifacts = release
        .artifacts
        .into_iter()
        .filter(|artifact| artifact.installable)
        .filter(|artifact| artifact.kind == "archive")
        .map(|artifact| go_remote_artifact_from_catalog_payload(tool, provider, &version, artifact))
        .collect::<CoreResult<Vec<_>>>()?;
    let upstream_version = release
        .upstream_version
        .unwrap_or_else(|| format!("go{}", version.raw()));
    let mut remote_release = RemoteRelease::new(version, artifacts)
        .with_metadata_field("upstream_version", upstream_version)
        .with_metadata_field("stable", stable)
        .with_metadata_field("yanked", yanked);
    if let Some(reason) = release.yanked_reason {
        remote_release = remote_release.with_metadata_field("yanked_reason", reason);
    }

    Ok(remote_release)
}

fn go_remote_artifact_from_official_file(
    tool: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    file: GoOfficialReleaseFilePayload,
) -> Option<CoreResult<ResolvedArtifact>> {
    let platform = go_platform_from_official_file(&file.os, &file.arch)?;

    let file_version = file
        .version
        .as_deref()
        .map(normalize_go_version)
        .transpose();
    let file_version = match file_version {
        Ok(file_version) => file_version,
        Err(error) => return Some(Err(error)),
    };
    if let Some(file_version) = file_version {
        if file_version != release_version.raw() {
            return Some(Err(CoreError::message(format!(
                "invalid Go official metadata: file `{}` has version `{}` but belongs to release `{}`",
                file.filename, file_version, release_version
            ))));
        }
    }

    let sha256 = file
        .sha256
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_owned();

    let result = (|| {
        let archive_type = archive_type_for_go_file(&file.filename)?;
        let url = file
            .url
            .unwrap_or_else(|| format!("https://go.dev/dl/{}", file.filename));
        let mut artifact = Artifact::new(
            url,
            file.filename.clone(),
            archive_type,
            Some(format!("sha256:{sha256}")),
        );
        if let Some(size) = file.size {
            artifact = artifact.with_size(size);
        }

        Ok(ResolvedArtifact::new(
            tool.clone(),
            provider.clone(),
            release_version.clone(),
            platform,
            artifact,
        )
        .with_metadata_field("filename", file.filename)
        .with_metadata_field("kind", file.kind.unwrap_or_else(|| "archive".to_owned()))
        .with_metadata_field("go_os", file.os)
        .with_metadata_field("go_arch", file.arch))
    })();

    Some(result)
}

fn go_remote_artifact_from_catalog_payload(
    tool: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    artifact: GoCatalogArtifactPayload,
) -> CoreResult<ResolvedArtifact> {
    let platform =
        go_platform_from_official_file(&artifact.os, &artifact.arch).ok_or_else(|| {
            CoreError::message(format!(
                "invalid Go catalog metadata: unsupported platform {}-{} for `{}`",
                artifact.os, artifact.arch, artifact.filename
            ))
        })?;
    let checksum = artifact.checksum.trim();
    if !checksum.starts_with("sha256:") {
        return Err(CoreError::message(format!(
            "invalid Go catalog metadata: archive `{}` checksum must use sha256:<hex>",
            artifact.filename
        )));
    }
    let archive_type = archive_type_for_go_file(&artifact.filename)?;
    let mut resolved_artifact = Artifact::new(
        artifact.url,
        artifact.filename.clone(),
        archive_type,
        Some(checksum.to_owned()),
    );
    if let Some(size) = artifact.size {
        resolved_artifact = resolved_artifact.with_size(size);
    }

    Ok(ResolvedArtifact::new(
        tool.clone(),
        provider.clone(),
        release_version.clone(),
        platform,
        resolved_artifact,
    )
    .with_metadata_field("filename", artifact.filename)
    .with_metadata_field("kind", artifact.kind)
    .with_metadata_field("go_os", artifact.os)
    .with_metadata_field("go_arch", artifact.arch))
}

fn go_platform_from_official_file(os: &str, arch: &str) -> Option<Platform> {
    let os = match os {
        "darwin" => OperatingSystem::Macos,
        "linux" => OperatingSystem::Linux,
        "windows" => OperatingSystem::Windows,
        _ => return None,
    };
    let arch = match arch {
        "amd64" => Architecture::X64,
        "arm64" => Architecture::Arm64,
        _ => return None,
    };

    Some(Platform::new(os, arch))
}

fn go_release_from_remote_release(release: &RemoteRelease) -> CoreResult<GoRelease> {
    let stable = release.metadata_field("stable") != Some("false")
        && release.metadata_field("yanked") != Some("true");
    let files = release
        .artifacts()
        .iter()
        .map(go_release_file_from_remote_artifact)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(GoRelease {
        version: release.version().clone(),
        stable,
        files,
    })
}

fn go_release_file_from_remote_artifact(resolved: &ResolvedArtifact) -> CoreResult<GoReleaseFile> {
    let artifact = resolved.artifact();
    let filename = resolved
        .metadata_field("filename")
        .unwrap_or_else(|| artifact.filename())
        .to_owned();
    let os = resolved
        .metadata_field("go_os")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            go_artifact_os(resolved.platform())
                .unwrap_or("unknown")
                .to_owned()
        });
    let arch = resolved
        .metadata_field("go_arch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            go_artifact_arch(resolved.platform())
                .unwrap_or("unknown")
                .to_owned()
        });
    let kind = resolved
        .metadata_field("kind")
        .unwrap_or("archive")
        .to_owned();

    Ok(GoReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256: artifact.checksum().map(ToOwned::to_owned),
        size: artifact.size(),
        url: Some(artifact.url().to_owned()),
    })
}

fn required_string<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
) -> CoreResult<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::message(format!("invalid Go release metadata: missing `{key}`")))
}

fn go_artifact_os(platform: Platform) -> CoreResult<&'static str> {
    match platform.os() {
        OperatingSystem::Macos => Ok("darwin"),
        OperatingSystem::Linux => Ok("linux"),
        OperatingSystem::Windows => Ok("windows"),
    }
}

fn go_artifact_arch(platform: Platform) -> CoreResult<&'static str> {
    match platform.architecture() {
        Architecture::X64 => Ok("amd64"),
        Architecture::Arm64 => Ok("arm64"),
    }
}

fn archive_type_for_go_file(filename: &str) -> CoreResult<ArchiveType> {
    if filename.ends_with(".tar.gz") {
        Ok(ArchiveType::TarGz)
    } else if filename.ends_with(".zip") {
        Ok(ArchiveType::Zip)
    } else {
        Err(CoreError::message(format!(
            "unsupported Go archive `{filename}`: expected .tar.gz or .zip"
        )))
    }
}

fn compare_go_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = GoVersionKey::parse(left.raw());
    let right_key = GoVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left), Ok(right)) => right.cmp(&left),
        _ => right.raw().cmp(left.raw()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GoVersionKey {
    components: Vec<u32>,
}

impl GoVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let raw = value.trim();
        let value = raw.strip_prefix("go").unwrap_or(raw);
        let components = value
            .split('.')
            .map(|component| {
                if component.is_empty()
                    || !component
                        .chars()
                        .all(|character| character.is_ascii_digit())
                {
                    return Err(CoreError::message(format!(
                        "invalid Go version `{raw}`: expected numeric dot-separated version"
                    )));
                }

                component.parse::<u32>().map_err(|error| {
                    CoreError::message(format!("invalid Go version `{raw}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;

        if components.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Go version `{raw}`: expected numeric dot-separated version"
            )));
        }

        Ok(Self { components })
    }

    fn matches_requirement(&self, requirement: &GoVersionKey) -> bool {
        self.components.starts_with(&requirement.components)
    }

    fn to_normalized_string(&self) -> String {
        self.components
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl Ord for GoVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components.cmp(&other.components)
    }
}

impl PartialOrd for GoVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn runtime_sort(left: &GoRuntime, right: &GoRuntime) -> Ordering {
    left.root.cmp(&right.root)
}

fn go_tool_name() -> ToolName {
    ToolName::new("go").expect("built-in Go tool name should be valid")
}
