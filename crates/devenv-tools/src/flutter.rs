use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform, ProviderId,
    RegisteredRuntime, RemoteRelease, RemoteReleaseIndex, ResolvedArtifact, RuntimeRegistry,
    ToolAdapter, ToolMetadata, ToolName, Version, VersionMatcher, VersionRequirement,
    VersionScheme, VersionSource,
};
use serde::Deserialize;

pub const FLUTTER_OFFICIAL_BASE_URL: &str =
    "https://storage.googleapis.com/flutter_infra_release/releases";
pub const FLUTTER_OFFICIAL_MACOS_RELEASES_URL: &str =
    "https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json";
pub const FLUTTER_OFFICIAL_LINUX_RELEASES_URL: &str =
    "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json";
pub const FLUTTER_OFFICIAL_WINDOWS_RELEASES_URL: &str =
    "https://storage.googleapis.com/flutter_infra_release/releases/releases_windows.json";

#[derive(Debug, Clone)]
pub struct FlutterToolAdapter {
    metadata: ToolMetadata,
}

impl FlutterToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                flutter_tool_name(),
                VersionScheme::Custom("flutter".to_owned()),
                vec!["flutter".to_owned(), "dart".to_owned()],
            ),
        }
    }
}

impl Default for FlutterToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for FlutterToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_flutter_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new()
            .set_env("FLUTTER_ROOT", runtime_root.to_string_lossy().into_owned())
            .prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlutterVersionMatcher;

impl VersionMatcher for FlutterVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_flutter_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlutterRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterRuntime {
    version: Version,
    root: PathBuf,
    source: FlutterRuntimeSource,
    platform: Option<Platform>,
}

impl FlutterRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        source: FlutterRuntimeSource,
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

    pub fn source(&self) -> &FlutterRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlutterRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl FlutterRuntimeDiscovery {
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
    ) -> CoreResult<Vec<FlutterRuntime>> {
        let flutter = flutter_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&flutter) {
            if runtime.platform() == platform {
                runtimes.push(flutter_runtime_from_registered(runtime)?);
            }
        }

        for installation in install_store.list_installations(&flutter) {
            if installation.platform() == platform {
                runtimes.push(flutter_runtime_from_installation(installation)?);
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

pub fn validate_flutter_sdk_home(root: impl AsRef<Path>) -> CoreResult<FlutterRuntime> {
    let root = canonical_flutter_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Flutter SDK `{}`: expected a Flutter SDK directory",
            root.display()
        )));
    }

    for binary in ["flutter", "dart"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Flutter SDK `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_flutter_version(&root)?;

    Ok(FlutterRuntime::new(
        Version::new(version)?,
        root,
        FlutterRuntimeSource::CandidatePath,
        None,
    ))
}

#[derive(Debug, Clone, Default)]
pub struct FlutterInstalledRuntimeValidator;

impl InstalledRuntimeValidator for FlutterInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_flutter_sdk_home(root).map(|_| ())
    }
}

pub fn match_flutter_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [FlutterRuntime],
) -> CoreResult<Option<&'a FlutterRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_flutter_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_flutter_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = FlutterVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = FlutterVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_flutter_version(value: &str) -> CoreResult<String> {
    Ok(FlutterVersionKey::parse(value)?.normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterReleaseMetadata {
    releases: Vec<FlutterRelease>,
}

impl FlutterReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse Flutter release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid Flutter release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_flutter_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[FlutterRelease] {
        &self.releases
    }

    pub fn from_release_index(index: &RemoteReleaseIndex) -> CoreResult<Self> {
        if index.tool().as_str() != "flutter" {
            return Err(CoreError::message(format!(
                "Flutter release metadata cannot be built from `{}` index",
                index.tool()
            )));
        }

        let releases = index
            .releases()
            .iter()
            .map(flutter_release_from_remote_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    fn release_for_version(&self, version: &Version) -> CoreResult<&FlutterRelease> {
        if let Some(exact) = self
            .releases()
            .iter()
            .find(|release| release.version().raw() == version.raw())
        {
            return Ok(exact);
        }

        let versions = self
            .releases()
            .iter()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        let requirement = VersionRequirement::exact(version.raw()).map_err(CoreError::from)?;
        let Some(matched) = match_flutter_version(&requirement, &versions)? else {
            return Err(CoreError::message(format!(
                "Flutter version `{}` was not found in metadata",
                version
            )));
        };

        self.releases()
            .iter()
            .find(|release| release.version().raw() == matched.raw())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Flutter version `{}` was not found in metadata",
                    version
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterOfficialReleaseMetadata {
    index: RemoteReleaseIndex,
}

impl FlutterOfficialReleaseMetadata {
    pub fn parse_stable(platform_payloads: &BTreeMap<String, String>) -> CoreResult<Self> {
        let tool = flutter_tool_name();
        let provider =
            ProviderId::new("stable").expect("built-in Flutter provider id should be valid");
        let mut releases = BTreeMap::<String, FlutterOfficialReleaseBuilder>::new();

        for (platform_name, payload) in platform_payloads {
            let os = flutter_official_platform_os(platform_name)?;
            let payload =
                serde_json::from_str::<FlutterOfficialIndexPayload>(payload).map_err(|error| {
                    CoreError::message(format!(
                        "failed to parse Flutter official {platform_name} releases: {error}"
                    ))
                })?;
            let base_url = payload
                .base_url
                .as_deref()
                .unwrap_or(FLUTTER_OFFICIAL_BASE_URL);

            for release in payload
                .releases
                .into_iter()
                .filter(|release| release.channel == "stable")
            {
                let version_raw = normalize_flutter_version(&release.version)?;
                let version = Version::new(&version_raw)?;
                let Some(artifact) = flutter_remote_artifact_from_official_release(
                    &tool, &provider, &version, os, base_url, release,
                )?
                else {
                    continue;
                };

                releases
                    .entry(version_raw)
                    .or_insert_with(|| FlutterOfficialReleaseBuilder::new(version))
                    .artifacts
                    .push(artifact);
            }
        }

        let releases = releases
            .into_values()
            .map(|release| {
                RemoteRelease::new(release.version, release.artifacts)
                    .with_metadata_field("channel", "stable")
                    .with_metadata_field("stable", "true")
            })
            .collect::<Vec<_>>();

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

    pub fn into_release_metadata(self) -> CoreResult<FlutterReleaseMetadata> {
        FlutterReleaseMetadata::from_release_index(&self.index)
    }
}

#[derive(Debug)]
struct FlutterOfficialReleaseBuilder {
    version: Version,
    artifacts: Vec<ResolvedArtifact>,
}

impl FlutterOfficialReleaseBuilder {
    fn new(version: Version) -> Self {
        Self {
            version,
            artifacts: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct FlutterOfficialIndexPayload {
    #[serde(default)]
    base_url: Option<String>,
    releases: Vec<FlutterOfficialReleasePayload>,
}

#[derive(Debug, Deserialize)]
struct FlutterOfficialReleasePayload {
    hash: String,
    channel: String,
    version: String,
    #[serde(default)]
    dart_sdk_version: Option<String>,
    #[serde(default)]
    dart_sdk_arch: Option<String>,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    archive: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterRelease {
    version: Version,
    channel: String,
    stable: bool,
    files: Vec<FlutterReleaseFile>,
}

impl FlutterRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[FlutterReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlutterReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl FlutterReleaseFile {
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
pub struct FlutterReleaseVersionSource {
    metadata: FlutterReleaseMetadata,
}

impl FlutterReleaseVersionSource {
    pub fn new(metadata: FlutterReleaseMetadata) -> Self {
        Self { metadata }
    }
}

impl VersionSource for FlutterReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "flutter" {
            return Ok(Vec::new());
        }

        let mut versions = self
            .metadata
            .releases()
            .iter()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_flutter_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct FlutterArtifactResolver {
    metadata: FlutterReleaseMetadata,
}

impl FlutterArtifactResolver {
    pub fn new(metadata: FlutterReleaseMetadata) -> Self {
        Self { metadata }
    }

    pub fn resolve_install_version(&self, requirement: &Version) -> CoreResult<Version> {
        Ok(self
            .metadata
            .release_for_version(requirement)?
            .version()
            .clone())
    }
}

impl ArtifactResolver for FlutterArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "flutter" {
            return Err(CoreError::message(format!(
                "Flutter artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self.metadata.release_for_version(version)?;
        let os = flutter_artifact_os(platform);
        let arch = flutter_artifact_arch(platform);
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "archive" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Flutter {} does not provide an archive for {}",
                    version,
                    platform.id()
                ))
            })?;
        let archive_type = archive_type_for_flutter_file(file.filename())?;
        let url = file.url().map(ToOwned::to_owned).unwrap_or_else(|| {
            format!(
                "https://storage.googleapis.com/flutter_infra_release/releases/{}/{}/{}",
                os,
                release.channel(),
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

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<FlutterRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_flutter_sdk_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Flutter candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Flutter candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_flutter_sdk_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn flutter_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<FlutterRuntime> {
    let root =
        canonical_flutter_home(runtime.root()).unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(FlutterRuntime::new(
        runtime.version().clone(),
        root,
        FlutterRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn flutter_runtime_from_installation(installation: Installation) -> CoreResult<FlutterRuntime> {
    let root = canonical_flutter_home(installation.root())?;
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_flutter_sdk_home(&root)?.version().clone()
    };

    Ok(FlutterRuntime::new(
        version,
        root,
        FlutterRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_flutter_home(root: &Path) -> CoreResult<PathBuf> {
    if root.join("bin/flutter").is_file() {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Flutter SDK `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Flutter SDK `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.join("bin/flutter").is_file() {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn read_flutter_version(root: &Path) -> CoreResult<String> {
    for relative in ["VERSION", "version", "bin/internal/flutter.version"] {
        let path = root.join(relative);
        if path.is_file() {
            let version = std::fs::read_to_string(&path).map_err(|error| {
                CoreError::message(format!(
                    "invalid Flutter SDK `{}`: failed to read `{}` for version metadata: {error}",
                    root.display(),
                    path.display()
                ))
            })?;
            return first_version_line(root, &path, &version);
        }
    }

    if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
        if let Ok(version) = normalize_flutter_version(name) {
            return Ok(version);
        }
    }

    Err(CoreError::message(format!(
        "invalid Flutter SDK `{}`: missing version metadata. Expected VERSION, version, bin/internal/flutter.version, or a versioned SDK directory name.",
        root.display()
    )))
}

fn first_version_line(root: &Path, path: &Path, contents: &str) -> CoreResult<String> {
    let version = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Flutter SDK `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_flutter_version(version)
}

fn parse_flutter_release(value: &toml::Value) -> CoreResult<FlutterRelease> {
    let version = required_string(value, "version", "Flutter release")?;
    let files = value
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| CoreError::message("invalid Flutter release metadata: missing files"))?
        .iter()
        .map(parse_flutter_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(FlutterRelease {
        version: Version::new(normalize_flutter_version(version)?)?,
        channel: optional_string(value, "channel")
            .unwrap_or("stable")
            .to_owned(),
        stable: value
            .get("stable")
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
        files,
    })
}

fn parse_flutter_release_file(value: &toml::Value) -> CoreResult<FlutterReleaseFile> {
    let filename = required_string(value, "filename", "Flutter release file")?.to_owned();
    Ok(FlutterReleaseFile {
        filename,
        os: required_string(value, "os", "Flutter release file")?.to_owned(),
        arch: required_string(value, "arch", "Flutter release file")?.to_owned(),
        kind: optional_string(value, "kind")
            .unwrap_or("archive")
            .to_owned(),
        sha256: optional_string(value, "sha256").map(ToOwned::to_owned),
        size: value
            .get("size")
            .and_then(toml::Value::as_integer)
            .map(|size| size as u64),
        url: optional_string(value, "url").map(ToOwned::to_owned),
    })
}

fn flutter_remote_artifact_from_official_release(
    tool: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    os: OperatingSystem,
    base_url: &str,
    release: FlutterOfficialReleasePayload,
) -> CoreResult<Option<ResolvedArtifact>> {
    let Some(architecture) = flutter_official_architecture(
        &release.archive,
        release.dart_sdk_arch.as_deref().or(release.arch.as_deref()),
    ) else {
        return Ok(None);
    };
    let platform = Platform::new(os, architecture);
    let filename = release
        .archive
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Flutter official metadata: archive path `{}` has no filename",
                release.archive
            ))
        })?
        .to_owned();
    let archive_type = archive_type_for_flutter_file(&filename)?;
    let checksum = normalize_flutter_official_sha256(&filename, &release.sha256)?;
    let url = flutter_official_artifact_url(base_url, &release.archive);
    let mut artifact = Artifact::new(url, filename.clone(), archive_type, Some(checksum));
    if let Some(size) = release.size {
        artifact = artifact.with_size(size);
    }

    let mut resolved = ResolvedArtifact::new(
        tool.clone(),
        provider.clone(),
        release_version.clone(),
        platform,
        artifact,
    )
    .with_metadata_field("filename", filename)
    .with_metadata_field("kind", "archive")
    .with_metadata_field("channel", release.channel)
    .with_metadata_field("flutter_os", flutter_artifact_os(platform))
    .with_metadata_field("flutter_arch", flutter_artifact_arch(platform))
    .with_metadata_field("hash", release.hash)
    .with_metadata_field("archive", release.archive);
    if let Some(dart_sdk_version) = release.dart_sdk_version {
        resolved = resolved.with_metadata_field("dart_sdk_version", dart_sdk_version);
    }
    if let Some(release_date) = release.release_date {
        resolved = resolved.with_metadata_field("release_date", release_date);
    }

    Ok(Some(resolved))
}

fn flutter_release_from_remote_release(release: &RemoteRelease) -> CoreResult<FlutterRelease> {
    let channel = release
        .metadata_field("channel")
        .unwrap_or("stable")
        .to_owned();
    let stable = release.metadata_field("stable") != Some("false") && channel == "stable";
    let files = release
        .artifacts()
        .iter()
        .map(flutter_release_file_from_remote_artifact)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(FlutterRelease {
        version: release.version().clone(),
        channel,
        stable,
        files,
    })
}

fn flutter_release_file_from_remote_artifact(
    resolved: &ResolvedArtifact,
) -> CoreResult<FlutterReleaseFile> {
    let artifact = resolved.artifact();
    let filename = resolved
        .metadata_field("filename")
        .unwrap_or_else(|| artifact.filename())
        .to_owned();
    let os = resolved
        .metadata_field("flutter_os")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| flutter_artifact_os(resolved.platform()).to_owned());
    let arch = resolved
        .metadata_field("flutter_arch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| flutter_artifact_arch(resolved.platform()).to_owned());
    let kind = resolved
        .metadata_field("kind")
        .unwrap_or("archive")
        .to_owned();

    Ok(FlutterReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256: artifact.checksum().map(ToOwned::to_owned),
        size: artifact.size(),
        url: Some(artifact.url().to_owned()),
    })
}

fn flutter_official_platform_os(platform_name: &str) -> CoreResult<OperatingSystem> {
    match platform_name {
        "macos" | "darwin" => Ok(OperatingSystem::Macos),
        "linux" => Ok(OperatingSystem::Linux),
        "windows" | "win" => Ok(OperatingSystem::Windows),
        other => Err(CoreError::message(format!(
            "unsupported Flutter official releases platform `{other}`"
        ))),
    }
}

fn flutter_official_architecture(
    archive_path: &str,
    declared_arch: Option<&str>,
) -> Option<Architecture> {
    if let Some(architecture) = declared_arch.and_then(normalize_flutter_official_architecture) {
        return Some(architecture);
    }

    let archive_path = archive_path.to_ascii_lowercase();
    if archive_path.contains("_arm64_")
        || archive_path.contains("-arm64")
        || archive_path.contains("/arm64")
    {
        return Some(Architecture::Arm64);
    }
    if archive_path.contains("_x64_")
        || archive_path.contains("-x64")
        || archive_path.contains("_x64.")
    {
        return Some(Architecture::X64);
    }

    Some(Architecture::X64)
}

fn normalize_flutter_official_architecture(value: &str) -> Option<Architecture> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x64" | "amd64" | "x86_64" | "x86-64" => Some(Architecture::X64),
        "arm64" | "aarch64" => Some(Architecture::Arm64),
        _ => None,
    }
}

fn normalize_flutter_official_sha256(filename: &str, value: &str) -> CoreResult<String> {
    let value = value.trim();
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(CoreError::message(format!(
            "invalid Flutter official metadata: archive `{filename}` has an invalid sha256 checksum"
        )));
    }

    Ok(format!("sha256:{value}"))
}

fn flutter_official_artifact_url(base_url: &str, archive_path: &str) -> String {
    if archive_path.starts_with("http://") || archive_path.starts_with("https://") {
        archive_path.to_owned()
    } else {
        format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            archive_path.trim_start_matches('/')
        )
    }
}

fn required_string<'a>(value: &'a toml::Value, key: &str, label: &str) -> CoreResult<&'a str> {
    value.get(key).and_then(toml::Value::as_str).ok_or_else(|| {
        CoreError::message(format!(
            "invalid {label} metadata: expected `{key}` to be a string"
        ))
    })
}

fn optional_string<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(toml::Value::as_str)
}

fn flutter_artifact_os(platform: Platform) -> &'static str {
    match platform.os() {
        OperatingSystem::Macos => "macos",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Windows => "windows",
    }
}

fn flutter_artifact_arch(platform: Platform) -> &'static str {
    match platform.architecture() {
        Architecture::X64 => "x64",
        Architecture::Arm64 => "arm64",
    }
}

fn archive_type_for_flutter_file(filename: &str) -> CoreResult<ArchiveType> {
    if filename.ends_with(".tar.gz") {
        Ok(ArchiveType::TarGz)
    } else if filename.ends_with(".tar.xz") {
        Ok(ArchiveType::TarXz)
    } else if filename.ends_with(".zip") {
        Ok(ArchiveType::Zip)
    } else {
        Err(CoreError::message(format!(
            "unsupported Flutter archive `{filename}`: expected .tar.gz, .tar.xz, or .zip"
        )))
    }
}

fn compare_flutter_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = FlutterVersionKey::parse(left.raw());
    let right_key = FlutterVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left), Ok(right)) => right.cmp(&left),
        _ => right.raw().cmp(left.raw()),
    }
}

fn runtime_sort(left: &FlutterRuntime, right: &FlutterRuntime) -> Ordering {
    compare_flutter_version_desc(left.version(), right.version())
        .then_with(|| left.root().cmp(right.root()))
}

fn flutter_tool_name() -> ToolName {
    ToolName::new("flutter").expect("built-in Flutter tool name should be valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlutterVersionKey {
    normalized: String,
    parts: Vec<u64>,
}

impl FlutterVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let normalized = version_token(value).ok_or_else(|| {
            CoreError::message(format!(
                "invalid Flutter version `{}`: expected a numeric version such as 3.24.0",
                value.trim()
            ))
        })?;
        let numeric = normalized
            .split(['-', '+'])
            .next()
            .unwrap_or(normalized.as_str());
        let parts = numeric
            .split('.')
            .map(|part| {
                part.parse::<u64>().map_err(|error| {
                    CoreError::message(format!("invalid Flutter version `{normalized}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        if parts.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Flutter version `{normalized}`: expected a numeric version"
            )));
        }

        Ok(Self { normalized, parts })
    }

    fn matches_requirement(&self, requirement: &FlutterVersionKey) -> bool {
        self.normalized == requirement.normalized || self.parts.starts_with(&requirement.parts)
    }
}

impl Ord for FlutterVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts
            .cmp(&other.parts)
            .then_with(|| self.normalized.cmp(&other.normalized))
    }
}

impl PartialOrd for FlutterVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn version_token(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('v');
    let start = value
        .char_indices()
        .find_map(|(index, character)| character.is_ascii_digit().then_some(index))?;
    let token = value[start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric()
                || *character == '.'
                || *character == '-'
                || *character == '+'
        })
        .collect::<String>();

    (!token.is_empty()).then_some(token)
}
