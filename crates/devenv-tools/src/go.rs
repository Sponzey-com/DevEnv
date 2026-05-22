use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionScheme, VersionSource,
};

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
