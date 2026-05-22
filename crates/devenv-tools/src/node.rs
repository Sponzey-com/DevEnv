use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionScheme, VersionSource,
};

#[derive(Debug, Clone)]
pub struct NodeToolAdapter {
    metadata: ToolMetadata,
}

impl NodeToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                node_tool_name(),
                VersionScheme::Custom("node".to_owned()),
                vec![
                    "node".to_owned(),
                    "npm".to_owned(),
                    "npx".to_owned(),
                    "corepack".to_owned(),
                ],
            ),
        }
    }
}

impl Default for NodeToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for NodeToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_node_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct NodeVersionMatcher;

impl VersionMatcher for NodeVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_node_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRuntime {
    version: Version,
    root: PathBuf,
    source: NodeRuntimeSource,
    platform: Option<Platform>,
}

impl NodeRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        source: NodeRuntimeSource,
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

    pub fn source(&self) -> &NodeRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct NodeRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl NodeRuntimeDiscovery {
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
    ) -> CoreResult<Vec<NodeRuntime>> {
        let node = node_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&node) {
            if runtime.platform() == platform {
                runtimes.push(node_runtime_from_registered(runtime)?);
            }
        }

        for installation in install_store.list_installations(&node) {
            if installation.platform() == platform {
                runtimes.push(node_runtime_from_installation(installation)?);
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

pub fn validate_node_home(root: impl AsRef<Path>) -> CoreResult<NodeRuntime> {
    let root = canonical_node_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Node.js runtime `{}`: expected a Node.js runtime directory",
            root.display()
        )));
    }

    let node = root.join("bin").join("node");
    if !node.is_file() {
        return Err(CoreError::message(format!(
            "invalid Node.js runtime `{}`: missing `{}`",
            root.display(),
            node.display()
        )));
    }

    for binary in ["npm", "npx"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Node.js runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_node_version(&root)?;

    Ok(NodeRuntime::new(
        Version::new(version)?,
        root,
        NodeRuntimeSource::CandidatePath,
        None,
    ))
}

#[derive(Debug, Clone, Default)]
pub struct NodeInstalledRuntimeValidator;

impl InstalledRuntimeValidator for NodeInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_node_home(root).map(|_| ())
    }
}

pub fn match_node_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [NodeRuntime],
) -> CoreResult<Option<&'a NodeRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_node_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_node_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = NodeVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = NodeVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_node_version(value: &str) -> CoreResult<String> {
    let key = NodeVersionKey::parse(value)?;
    Ok(key.to_normalized_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReleaseMetadata {
    releases: Vec<NodeRelease>,
}

impl NodeReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse Node.js release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid Node.js release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_node_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[NodeRelease] {
        &self.releases
    }

    fn release_for_version(&self, version: &Version) -> CoreResult<&NodeRelease> {
        if let Some(exact) = self
            .releases
            .iter()
            .find(|release| release.version().raw() == version.raw())
        {
            return Ok(exact);
        }

        let versions = self
            .releases
            .iter()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        let requirement = VersionRequirement::exact(version.raw()).map_err(CoreError::from)?;
        let Some(matched) = match_node_version(&requirement, &versions)? else {
            return Err(CoreError::message(format!(
                "Node.js version `{}` was not found in metadata",
                version
            )));
        };

        self.releases
            .iter()
            .find(|release| release.version().raw() == matched.raw())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Node.js version `{}` was not found in metadata",
                    version
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRelease {
    version: Version,
    stable: bool,
    files: Vec<NodeReleaseFile>,
}

impl NodeRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[NodeReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl NodeReleaseFile {
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
pub struct NodeReleaseVersionSource {
    metadata: NodeReleaseMetadata,
}

impl NodeReleaseVersionSource {
    pub fn new(metadata: NodeReleaseMetadata) -> Self {
        Self { metadata }
    }
}

impl VersionSource for NodeReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "node" {
            return Ok(Vec::new());
        }

        let mut versions = self
            .metadata
            .releases()
            .iter()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_node_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct NodeArtifactResolver {
    metadata: NodeReleaseMetadata,
}

impl NodeArtifactResolver {
    pub fn new(metadata: NodeReleaseMetadata) -> Self {
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

impl ArtifactResolver for NodeArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "node" {
            return Err(CoreError::message(format!(
                "Node.js artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self.metadata.release_for_version(version)?;
        let os = node_artifact_os(platform);
        let arch = node_artifact_arch(platform);
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "archive" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "Node.js {} does not provide an archive for {}",
                    version,
                    platform.id()
                ))
            })?;
        let archive_type = archive_type_for_node_file(file.filename())?;
        let url = file.url().map(ToOwned::to_owned).unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{}/{}",
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

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<NodeRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_node_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Node.js candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Node.js candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_node_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn node_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<NodeRuntime> {
    let root = canonical_node_home(runtime.root()).unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(NodeRuntime::new(
        runtime.version().clone(),
        root,
        NodeRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn node_runtime_from_installation(installation: Installation) -> CoreResult<NodeRuntime> {
    let root = canonical_node_home(installation.root())?;
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_node_home(&root)?.version().clone()
    };

    Ok(NodeRuntime::new(
        version,
        root,
        NodeRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_node_home(root: &Path) -> CoreResult<PathBuf> {
    if root.join("bin/node").is_file() {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Node.js runtime `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Node.js runtime `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.join("bin/node").is_file() {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn read_node_version(root: &Path) -> CoreResult<String> {
    let version_path = root.join("VERSION");
    if version_path.is_file() {
        let version = std::fs::read_to_string(&version_path).map_err(|error| {
            CoreError::message(format!(
                "invalid Node.js runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                version_path.display()
            ))
        })?;
        return first_version_line(root, &version_path, &version);
    }

    let header_path = root.join("include/node/node_version.h");
    if header_path.is_file() {
        let header = std::fs::read_to_string(&header_path).map_err(|error| {
            CoreError::message(format!(
                "invalid Node.js runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                header_path.display()
            ))
        })?;
        for line in header.lines() {
            if line.contains("NODE_VERSION") {
                let Some((_, value)) = line.split_once('"') else {
                    continue;
                };
                let Some((version, _)) = value.split_once('"') else {
                    continue;
                };
                return normalize_node_version(version);
            }
        }
    }

    Err(CoreError::message(format!(
        "invalid Node.js runtime `{}`: missing version metadata. Expected `VERSION` or `include/node/node_version.h`.",
        root.display()
    )))
}

fn first_version_line(root: &Path, path: &Path, input: &str) -> CoreResult<String> {
    let version = input
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Node.js runtime `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_node_version(version)
}

fn parse_node_release(value: &toml::Value) -> CoreResult<NodeRelease> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Node.js release metadata: release must be a table")
    })?;
    let version = normalize_node_version(required_string(table, "version")?)?;
    let stable = table
        .get("stable")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    let files = table
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Node.js release metadata: release `{version}` has no [[release.file]] entries"
            ))
        })?
        .iter()
        .map(parse_node_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(NodeRelease {
        version: Version::new(version)?,
        stable,
        files,
    })
}

fn parse_node_release_file(value: &toml::Value) -> CoreResult<NodeReleaseFile> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Node.js release metadata: release file must be a table")
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
                    "invalid Node.js release metadata: size for `{filename}` must be non-negative"
                ))
            })
        })
        .transpose()?;
    let url = table
        .get("url")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);

    Ok(NodeReleaseFile {
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
        .ok_or_else(|| {
            CoreError::message(format!("invalid Node.js release metadata: missing `{key}`"))
        })
}

fn node_artifact_os(platform: Platform) -> &'static str {
    match platform.os() {
        OperatingSystem::Macos => "darwin",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Windows => "win",
    }
}

fn node_artifact_arch(platform: Platform) -> &'static str {
    match platform.architecture() {
        Architecture::X64 => "x64",
        Architecture::Arm64 => "arm64",
    }
}

fn archive_type_for_node_file(filename: &str) -> CoreResult<ArchiveType> {
    if filename.ends_with(".tar.gz") {
        Ok(ArchiveType::TarGz)
    } else if filename.ends_with(".zip") {
        Ok(ArchiveType::Zip)
    } else {
        Err(CoreError::message(format!(
            "unsupported Node.js archive `{filename}`: expected .tar.gz or .zip"
        )))
    }
}

fn compare_node_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = NodeVersionKey::parse(left.raw());
    let right_key = NodeVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left_key), Ok(right_key)) => right_key.cmp(&left_key),
        _ => right.raw().cmp(left.raw()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeVersionKey {
    components: Vec<u32>,
}

impl NodeVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let mut value = value.trim();
        if let Some(stripped) = value.strip_prefix('v') {
            value = stripped;
        }

        let mut components = Vec::new();
        let mut current = String::new();
        for character in value.chars() {
            if character.is_ascii_digit() {
                current.push(character);
            } else if !current.is_empty() {
                components.push(current.parse::<u32>().map_err(|error| {
                    CoreError::message(format!("invalid Node.js version `{value}`: {error}"))
                })?);
                current.clear();
                if character == '-' || character == '+' {
                    break;
                }
            }
        }

        if !current.is_empty() {
            components.push(current.parse::<u32>().map_err(|error| {
                CoreError::message(format!("invalid Node.js version `{value}`: {error}"))
            })?);
        }

        if components.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Node.js version `{value}`: expected a numeric version"
            )));
        }

        Ok(Self { components })
    }

    fn matches_requirement(&self, requirement: &NodeVersionKey) -> bool {
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

impl Ord for NodeVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components.cmp(&other.components)
    }
}

impl PartialOrd for NodeVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn runtime_sort(left: &NodeRuntime, right: &NodeRuntime) -> Ordering {
    left.root.cmp(&right.root)
}

fn node_tool_name() -> ToolName {
    ToolName::new("node").expect("built-in Node.js tool name should be valid")
}
