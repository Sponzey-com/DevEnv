use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionScheme, VersionSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IacTool {
    Terraform,
    OpenTofu,
}

impl IacTool {
    pub fn tool_name(self) -> ToolName {
        ToolName::new(self.as_str()).expect("built-in IaC tool name should be valid")
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terraform => "terraform",
            Self::OpenTofu => "opentofu",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Terraform => "Terraform",
            Self::OpenTofu => "OpenTofu",
        }
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Terraform => "terraform",
            Self::OpenTofu => "tofu",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerraformToolAdapter {
    metadata: ToolMetadata,
}

impl TerraformToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                IacTool::Terraform.tool_name(),
                VersionScheme::Custom("terraform".to_owned()),
                vec![IacTool::Terraform.binary_name().to_owned()],
            ),
        }
    }
}

impl Default for TerraformToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for TerraformToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_iac_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(
            ActivationPlan::new().prepend_path(activation_path_for_iac_root(
                runtime_root,
                IacTool::Terraform,
            )),
        )
    }
}

#[derive(Debug, Clone)]
pub struct OpenTofuToolAdapter {
    metadata: ToolMetadata,
}

impl OpenTofuToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                IacTool::OpenTofu.tool_name(),
                VersionScheme::Custom("opentofu".to_owned()),
                vec![IacTool::OpenTofu.binary_name().to_owned()],
            ),
        }
    }
}

impl Default for OpenTofuToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for OpenTofuToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_iac_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(
            ActivationPlan::new().prepend_path(activation_path_for_iac_root(
                runtime_root,
                IacTool::OpenTofu,
            )),
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct IacVersionMatcher;

impl VersionMatcher for IacVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_iac_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IacRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IacRuntime {
    tool: IacTool,
    version: Version,
    root: PathBuf,
    source: IacRuntimeSource,
    platform: Option<Platform>,
}

impl IacRuntime {
    pub fn new(
        tool: IacTool,
        version: Version,
        root: impl Into<PathBuf>,
        source: IacRuntimeSource,
        platform: Option<Platform>,
    ) -> Self {
        Self {
            tool,
            version,
            root: root.into(),
            source,
            platform,
        }
    }

    pub fn tool(&self) -> IacTool {
        self.tool
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> &IacRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone)]
pub struct IacRuntimeDiscovery {
    tool: IacTool,
    candidate_roots: Vec<PathBuf>,
}

impl IacRuntimeDiscovery {
    pub fn new(tool: IacTool) -> Self {
        Self {
            tool,
            candidate_roots: Vec::new(),
        }
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
    ) -> CoreResult<Vec<IacRuntime>> {
        let tool_name = self.tool.tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&tool_name) {
            if runtime.platform() == platform {
                runtimes.push(iac_runtime_from_registered(self.tool, runtime)?);
            }
        }

        for installation in install_store.list_installations(&tool_name) {
            if installation.platform() == platform {
                runtimes.push(iac_runtime_from_installation(self.tool, installation)?);
            }
        }

        for candidate in &self.candidate_roots {
            runtimes.extend(discover_candidate_root(self.tool, candidate)?);
        }

        runtimes.sort_by(runtime_sort);
        runtimes.dedup_by(|left, right| left.root == right.root);

        Ok(runtimes)
    }
}

pub fn validate_iac_tool_home(root: impl AsRef<Path>, tool: IacTool) -> CoreResult<IacRuntime> {
    let root = canonical_iac_home(root.as_ref(), tool)?;
    validate_iac_binary_layout(&root, tool)?;
    let version = read_iac_version(&root, tool)?;

    Ok(IacRuntime::new(
        tool,
        Version::new(version)?,
        root,
        IacRuntimeSource::CandidatePath,
        None,
    ))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TerraformInstalledRuntimeValidator;

impl InstalledRuntimeValidator for TerraformInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_iac_binary_layout(root, IacTool::Terraform)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OpenTofuInstalledRuntimeValidator;

impl InstalledRuntimeValidator for OpenTofuInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_iac_binary_layout(root, IacTool::OpenTofu)
    }
}

pub fn match_iac_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [IacRuntime],
) -> CoreResult<Option<&'a IacRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_iac_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_iac_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = IacVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = IacVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_iac_version(value: &str) -> CoreResult<String> {
    Ok(IacVersionKey::parse(value)?.normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IacReleaseMetadata {
    releases: Vec<IacRelease>,
}

impl IacReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse IaC release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid IaC release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_iac_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[IacRelease] {
        &self.releases
    }

    fn release_for_version(&self, tool: IacTool, version: &Version) -> CoreResult<&IacRelease> {
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
        let Some(matched) = match_iac_version(&requirement, &versions)? else {
            return Err(CoreError::message(format!(
                "{} version `{}` was not found in metadata",
                tool.display_name(),
                version
            )));
        };

        self.releases()
            .iter()
            .find(|release| release.version().raw() == matched.raw())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "{} version `{}` was not found in metadata",
                    tool.display_name(),
                    version
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IacRelease {
    version: Version,
    stable: bool,
    files: Vec<IacReleaseFile>,
}

impl IacRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[IacReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IacReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl IacReleaseFile {
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
pub struct IacReleaseVersionSource {
    tool: IacTool,
    metadata: IacReleaseMetadata,
}

impl IacReleaseVersionSource {
    pub fn new(tool: IacTool, metadata: IacReleaseMetadata) -> Self {
        Self { tool, metadata }
    }
}

impl VersionSource for IacReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != self.tool.as_str() {
            return Ok(Vec::new());
        }

        let mut versions = self
            .metadata
            .releases()
            .iter()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_iac_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct IacArtifactResolver {
    tool: IacTool,
    metadata: IacReleaseMetadata,
}

impl IacArtifactResolver {
    pub fn new(tool: IacTool, metadata: IacReleaseMetadata) -> Self {
        Self { tool, metadata }
    }

    pub fn resolve_install_version(&self, requirement: &Version) -> CoreResult<Version> {
        Ok(self
            .metadata
            .release_for_version(self.tool, requirement)?
            .version()
            .clone())
    }
}

impl ArtifactResolver for IacArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != self.tool.as_str() {
            return Err(CoreError::message(format!(
                "{} artifact resolver cannot resolve `{tool}`",
                self.tool.display_name()
            )));
        }

        let release = self.metadata.release_for_version(self.tool, version)?;
        let os = iac_artifact_os(platform);
        let arch = iac_artifact_arch(platform);
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "binary" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "{} {} does not provide a binary for {}",
                    self.tool.display_name(),
                    version,
                    platform.id()
                ))
            })?;
        let url = file
            .url()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default_iac_artifact_url(self.tool, release.version(), file));
        let mut artifact = Artifact::new(
            url,
            self.tool.binary_name(),
            ArchiveType::PlainFile,
            file.sha256().map(ToOwned::to_owned),
        );
        if let Some(size) = file.size() {
            artifact = artifact.with_size(size);
        }

        Ok(artifact)
    }
}

fn discover_candidate_root(tool: IacTool, root: &Path) -> CoreResult<Vec<IacRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_iac_tool_home(root, tool) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan {} candidate directory `{}`: {error}",
            tool.display_name(),
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan {} candidate directory `{}`: {error}",
                tool.display_name(),
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_iac_tool_home(&path, tool) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn iac_runtime_from_registered(
    tool: IacTool,
    runtime: RegisteredRuntime,
) -> CoreResult<IacRuntime> {
    let root =
        canonical_iac_home(runtime.root(), tool).unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(IacRuntime::new(
        tool,
        runtime.version().clone(),
        root,
        IacRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn iac_runtime_from_installation(
    tool: IacTool,
    installation: Installation,
) -> CoreResult<IacRuntime> {
    let root = canonical_iac_home(installation.root(), tool)?;

    Ok(IacRuntime::new(
        tool,
        installation.version().clone(),
        root,
        IacRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_iac_home(root: &Path, tool: IacTool) -> CoreResult<PathBuf> {
    if is_tool_binary(root, tool) {
        return root.parent().map(Path::to_path_buf).ok_or_else(|| {
            CoreError::message(format!(
                "invalid {} runtime `{}`: binary path has no parent",
                tool.display_name(),
                root.display()
            ))
        });
    }

    Ok(root.to_path_buf())
}

fn validate_iac_binary_layout(root: &Path, tool: IacTool) -> CoreResult<()> {
    if !root.exists() {
        return Err(CoreError::message(format!(
            "invalid {} runtime `{}`: expected a directory or `{}` binary",
            tool.display_name(),
            root.display(),
            tool.binary_name()
        )));
    }

    if binary_path(root, tool).is_some() {
        return Ok(());
    }

    Err(CoreError::message(format!(
        "invalid {} runtime `{}`: missing `{}` or `{}`",
        tool.display_name(),
        root.display(),
        root.join(tool.binary_name()).display(),
        root.join("bin").join(tool.binary_name()).display()
    )))
}

fn read_iac_version(root: &Path, tool: IacTool) -> CoreResult<String> {
    for relative in ["VERSION", ".devenv-version", "bin/VERSION"] {
        let path = root.join(relative);
        if path.is_file() {
            let version = std::fs::read_to_string(&path).map_err(|error| {
                CoreError::message(format!(
                    "invalid {} runtime `{}`: failed to read `{}` for version metadata: {error}",
                    tool.display_name(),
                    root.display(),
                    path.display()
                ))
            })?;
            return first_version_line(root, &path, &version, tool);
        }
    }

    if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
        if let Ok(version) = normalize_iac_version(name) {
            return Ok(version);
        }
    }

    Err(CoreError::message(format!(
        "invalid {} runtime `{}`: missing version metadata. Expected VERSION, .devenv-version, bin/VERSION, or a versioned runtime directory name.",
        tool.display_name(),
        root.display()
    )))
}

fn first_version_line(
    root: &Path,
    path: &Path,
    contents: &str,
    tool: IacTool,
) -> CoreResult<String> {
    let version = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid {} runtime `{}`: missing version in `{}`",
                tool.display_name(),
                root.display(),
                path.display()
            ))
        })?;

    normalize_iac_version(version)
}

fn activation_path_for_iac_root(root: &Path, tool: IacTool) -> PathBuf {
    if has_tool_binary(root, tool) {
        root.to_path_buf()
    } else if has_tool_binary(&root.join("bin"), tool) {
        root.join("bin")
    } else {
        root.to_path_buf()
    }
}

fn binary_path(root: &Path, tool: IacTool) -> Option<PathBuf> {
    let direct = root.join(tool.binary_name());
    if direct.is_file() {
        return Some(direct);
    }

    let nested = root.join("bin").join(tool.binary_name());
    if nested.is_file() {
        return Some(nested);
    }

    let direct_exe = root.join(format!("{}.exe", tool.binary_name()));
    if direct_exe.is_file() {
        return Some(direct_exe);
    }

    let nested_exe = root.join("bin").join(format!("{}.exe", tool.binary_name()));
    if nested_exe.is_file() {
        return Some(nested_exe);
    }

    None
}

fn has_tool_binary(root: &Path, tool: IacTool) -> bool {
    root.join(tool.binary_name()).is_file()
        || root.join(format!("{}.exe", tool.binary_name())).is_file()
}

fn is_tool_binary(path: &Path, tool: IacTool) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name == tool.binary_name() || name == format!("{}.exe", tool.binary_name())
            })
}

fn parse_iac_release(value: &toml::Value) -> CoreResult<IacRelease> {
    let version = required_string(value, "version", "IaC release")?;
    let files = value
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| CoreError::message("invalid IaC release metadata: missing files"))?
        .iter()
        .map(parse_iac_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(IacRelease {
        version: Version::new(normalize_iac_version(version)?)?,
        stable: value
            .get("stable")
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
        files,
    })
}

fn parse_iac_release_file(value: &toml::Value) -> CoreResult<IacReleaseFile> {
    Ok(IacReleaseFile {
        filename: required_string(value, "filename", "IaC release file")?.to_owned(),
        os: required_string(value, "os", "IaC release file")?.to_owned(),
        arch: required_string(value, "arch", "IaC release file")?.to_owned(),
        kind: optional_string(value, "kind")
            .unwrap_or("binary")
            .to_owned(),
        sha256: optional_string(value, "sha256").map(ToOwned::to_owned),
        size: value
            .get("size")
            .and_then(toml::Value::as_integer)
            .map(|size| size as u64),
        url: optional_string(value, "url").map(ToOwned::to_owned),
    })
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

fn iac_artifact_os(platform: Platform) -> &'static str {
    match platform.os() {
        OperatingSystem::Macos => "darwin",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Windows => "windows",
    }
}

fn iac_artifact_arch(platform: Platform) -> &'static str {
    match platform.architecture() {
        Architecture::X64 => "amd64",
        Architecture::Arm64 => "arm64",
    }
}

fn default_iac_artifact_url(tool: IacTool, version: &Version, file: &IacReleaseFile) -> String {
    match tool {
        IacTool::Terraform => format!(
            "https://releases.hashicorp.com/terraform/{}/{}",
            version.raw(),
            file.filename()
        ),
        IacTool::OpenTofu => format!(
            "https://github.com/opentofu/opentofu/releases/download/v{}/{}",
            version.raw(),
            file.filename()
        ),
    }
}

fn compare_iac_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = IacVersionKey::parse(left.raw());
    let right_key = IacVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left), Ok(right)) => right.cmp(&left),
        _ => right.raw().cmp(left.raw()),
    }
}

fn runtime_sort(left: &IacRuntime, right: &IacRuntime) -> Ordering {
    compare_iac_version_desc(left.version(), right.version())
        .then_with(|| left.root().cmp(right.root()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IacVersionKey {
    normalized: String,
    parts: Vec<u64>,
}

impl IacVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let normalized = version_token(value).ok_or_else(|| {
            CoreError::message(format!(
                "invalid IaC tool version `{}`: expected a numeric version such as 1.8.5",
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
                    CoreError::message(format!("invalid IaC tool version `{normalized}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        if parts.is_empty() {
            return Err(CoreError::message(format!(
                "invalid IaC tool version `{normalized}`: expected a numeric version"
            )));
        }

        Ok(Self { normalized, parts })
    }

    fn matches_requirement(&self, requirement: &IacVersionKey) -> bool {
        self.normalized == requirement.normalized || self.parts.starts_with(&requirement.parts)
    }
}

impl Ord for IacVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts
            .cmp(&other.parts)
            .then_with(|| self.normalized.cmp(&other.normalized))
    }
}

impl PartialOrd for IacVersionKey {
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
