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

pub const TERRAFORM_OFFICIAL_INDEX_URL: &str =
    "https://releases.hashicorp.com/terraform/index.json";
pub const TERRAFORM_OFFICIAL_BASE_URL: &str = "https://releases.hashicorp.com/terraform";
pub const OPENTOFU_OFFICIAL_RELEASES_URL: &str =
    "https://api.github.com/repos/opentofu/opentofu/releases";
pub const OPENTOFU_OFFICIAL_BASE_URL: &str =
    "https://github.com/opentofu/opentofu/releases/download";

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

    pub fn provider_id(self) -> &'static str {
        match self {
            Self::Terraform => "hashicorp",
            Self::OpenTofu => "opentofu",
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

    pub fn from_release_index(tool: IacTool, index: &RemoteReleaseIndex) -> CoreResult<Self> {
        if index.tool().as_str() != tool.as_str() {
            return Err(CoreError::message(format!(
                "{} release metadata cannot be built from `{}` index",
                tool.display_name(),
                index.tool()
            )));
        }

        let releases = index
            .releases()
            .iter()
            .map(iac_release_from_remote_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
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
pub struct IacOfficialReleaseMetadata {
    tool: IacTool,
    index: RemoteReleaseIndex,
}

impl IacOfficialReleaseMetadata {
    pub fn parse_terraform(
        index_json: &str,
        checksums_by_version: &BTreeMap<String, String>,
    ) -> CoreResult<Self> {
        let payload =
            serde_json::from_str::<TerraformOfficialIndexPayload>(index_json).map_err(|error| {
                CoreError::message(format!("failed to parse Terraform official index: {error}"))
            })?;
        let tool = IacTool::Terraform;
        let tool_name = tool.tool_name();
        let provider = ProviderId::new(tool.provider_id())
            .expect("built-in Terraform provider id should be valid");
        let releases = payload
            .versions
            .into_iter()
            .map(|(version, release)| {
                terraform_remote_release_from_official_payload(
                    tool,
                    &tool_name,
                    &provider,
                    &version,
                    release,
                    checksums_by_version,
                )
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self {
            tool,
            index: RemoteReleaseIndex::new(tool_name, provider, releases),
        })
    }

    pub fn parse_opentofu(
        releases_json: &str,
        checksums_by_version: &BTreeMap<String, String>,
    ) -> CoreResult<Self> {
        let payload = serde_json::from_str::<Vec<OpenTofuReleasePayload>>(releases_json).map_err(
            |error| {
                CoreError::message(format!(
                    "failed to parse OpenTofu official releases: {error}"
                ))
            },
        )?;
        let tool = IacTool::OpenTofu;
        let tool_name = tool.tool_name();
        let provider = ProviderId::new(tool.provider_id())
            .expect("built-in OpenTofu provider id should be valid");
        let releases = payload
            .into_iter()
            .filter(|release| !release.draft)
            .map(|release| {
                opentofu_remote_release_from_official_payload(
                    tool,
                    &tool_name,
                    &provider,
                    release,
                    checksums_by_version,
                )
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self {
            tool,
            index: RemoteReleaseIndex::new(tool_name, provider, releases),
        })
    }

    pub fn release_index(&self) -> &RemoteReleaseIndex {
        &self.index
    }

    pub fn into_release_index(self) -> RemoteReleaseIndex {
        self.index
    }

    pub fn into_release_metadata(self) -> CoreResult<IacReleaseMetadata> {
        IacReleaseMetadata::from_release_index(self.tool, &self.index)
    }

    pub fn tool(&self) -> IacTool {
        self.tool
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IacCatalogReleaseMetadata {
    tool: IacTool,
    index: RemoteReleaseIndex,
}

impl IacCatalogReleaseMetadata {
    pub fn parse_terraform(input: &str) -> CoreResult<Self> {
        Self::parse(input, IacTool::Terraform)
    }

    fn parse(input: &str, tool: IacTool) -> CoreResult<Self> {
        let payload = serde_json::from_str::<IacCatalogPayload>(input).map_err(|error| {
            CoreError::message(format!(
                "failed to parse {} catalog metadata: {error}",
                tool.display_name()
            ))
        })?;
        if payload.schema_version != 1 {
            return Err(CoreError::message(format!(
                "unsupported {} catalog metadata schema version {}: expected 1",
                tool.display_name(),
                payload.schema_version
            )));
        }
        if payload.tool != tool.as_str() {
            return Err(CoreError::message(format!(
                "{} catalog metadata cannot parse tool `{}`",
                tool.display_name(),
                payload.tool
            )));
        }
        if payload.provider != tool.provider_id() {
            return Err(CoreError::message(format!(
                "{} catalog metadata cannot parse provider `{}`",
                tool.display_name(),
                payload.provider
            )));
        }

        let tool_name = tool.tool_name();
        let provider =
            ProviderId::new(tool.provider_id()).expect("built-in IaC provider id should be valid");
        let releases = payload
            .releases
            .into_iter()
            .map(|release| {
                iac_remote_release_from_catalog_payload(tool, &tool_name, &provider, release)
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self {
            tool,
            index: RemoteReleaseIndex::new(tool_name, provider, releases),
        })
    }

    pub fn release_index(&self) -> &RemoteReleaseIndex {
        &self.index
    }

    pub fn into_release_index(self) -> RemoteReleaseIndex {
        self.index
    }

    pub fn into_release_metadata(self) -> CoreResult<IacReleaseMetadata> {
        IacReleaseMetadata::from_release_index(self.tool, &self.index)
    }

    pub fn tool(&self) -> IacTool {
        self.tool
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
        let archive_type = archive_type_for_iac_file(file.filename());
        let artifact_filename = if archive_type == ArchiveType::PlainFile {
            self.tool.binary_name()
        } else {
            file.filename()
        };
        let url = file
            .url()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default_iac_artifact_url(self.tool, release.version(), file));
        let mut artifact = Artifact::new(
            url,
            artifact_filename,
            archive_type,
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

#[derive(Debug, Deserialize)]
struct TerraformOfficialIndexPayload {
    #[serde(default)]
    versions: BTreeMap<String, TerraformVersionPayload>,
}

#[derive(Debug, Deserialize)]
struct TerraformVersionPayload {
    #[serde(default)]
    builds: Vec<TerraformBuildPayload>,
}

#[derive(Debug, Deserialize)]
struct TerraformBuildPayload {
    arch: String,
    os: String,
    filename: String,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenTofuReleasePayload {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<OpenTofuAssetPayload>,
}

#[derive(Debug, Deserialize)]
struct OpenTofuAssetPayload {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct IacCatalogPayload {
    schema_version: u32,
    tool: String,
    provider: String,
    #[serde(default)]
    releases: Vec<IacCatalogReleasePayload>,
}

#[derive(Debug, Deserialize)]
struct IacCatalogReleasePayload {
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
    artifacts: Vec<IacCatalogArtifactPayload>,
}

#[derive(Debug, Deserialize)]
struct IacCatalogArtifactPayload {
    filename: String,
    os: String,
    arch: String,
    url: String,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default = "default_single_binary_kind")]
    kind: String,
    #[serde(default = "default_true")]
    installable: bool,
}

fn default_true() -> bool {
    true
}

fn default_single_binary_kind() -> String {
    "single-binary".to_owned()
}

pub fn parse_iac_sha256s(input: &str) -> CoreResult<BTreeMap<String, String>> {
    let mut checksums = BTreeMap::new();
    for (line_index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let checksum = parts.next().ok_or_else(|| {
            CoreError::message(format!(
                "invalid IaC checksum line {}: missing checksum",
                line_index + 1
            ))
        })?;
        let filename = parts.next().ok_or_else(|| {
            CoreError::message(format!(
                "invalid IaC checksum line {}: missing filename",
                line_index + 1
            ))
        })?;
        if parts.next().is_some() {
            return Err(CoreError::message(format!(
                "invalid IaC checksum line {}: expected `<sha256>  <filename>`",
                line_index + 1
            )));
        }
        if checksum.len() != 64
            || !checksum
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(CoreError::message(format!(
                "invalid IaC checksum line {}: checksum for `{filename}` is not a sha256 hex digest",
                line_index + 1
            )));
        }
        checksums.insert(filename.to_owned(), format!("sha256:{checksum}"));
    }

    Ok(checksums)
}

fn terraform_remote_release_from_official_payload(
    tool: IacTool,
    tool_name: &ToolName,
    provider: &ProviderId,
    version: &str,
    release: TerraformVersionPayload,
    checksums_by_version: &BTreeMap<String, String>,
) -> CoreResult<RemoteRelease> {
    let normalized = normalize_iac_version(version)?;
    let version = Version::new(&normalized)?;
    let checksums = checksums_for_iac_release(tool, &normalized, checksums_by_version)?;
    let artifacts = release
        .builds
        .into_iter()
        .filter_map(terraform_archive_from_build)
        .map(|archive| {
            iac_remote_artifact_from_official_archive(
                tool_name, provider, &version, archive, &checksums,
            )
        })
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(RemoteRelease::new(version, artifacts)
        .with_metadata_field("upstream_version", normalized)
        .with_metadata_field("stable", "true"))
}

fn opentofu_remote_release_from_official_payload(
    tool: IacTool,
    tool_name: &ToolName,
    provider: &ProviderId,
    release: OpenTofuReleasePayload,
    checksums_by_version: &BTreeMap<String, String>,
) -> CoreResult<RemoteRelease> {
    let normalized = normalize_iac_version(&release.tag_name)?;
    let version = Version::new(&normalized)?;
    let checksums = checksums_for_iac_release(tool, &normalized, checksums_by_version)?;
    let artifacts = release
        .assets
        .into_iter()
        .filter_map(opentofu_archive_from_asset)
        .map(|archive| {
            iac_remote_artifact_from_official_archive(
                tool_name, provider, &version, archive, &checksums,
            )
        })
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(RemoteRelease::new(version, artifacts)
        .with_metadata_field("upstream_version", release.tag_name)
        .with_metadata_field("stable", (!release.prerelease).to_string()))
}

fn iac_remote_release_from_catalog_payload(
    tool: IacTool,
    tool_name: &ToolName,
    provider: &ProviderId,
    release: IacCatalogReleasePayload,
) -> CoreResult<RemoteRelease> {
    let normalized = normalize_iac_version(&release.version)?;
    let version = Version::new(&normalized)?;
    let stable = release.stable.to_string();
    let yanked = release.yanked.to_string();
    let artifacts = release
        .artifacts
        .into_iter()
        .filter_map(|artifact| {
            iac_remote_artifact_from_catalog_payload(tool, tool_name, provider, &version, artifact)
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let upstream_version = release
        .upstream_version
        .unwrap_or_else(|| version.raw().to_owned());
    let mut remote_release = RemoteRelease::new(version, artifacts)
        .with_metadata_field("upstream_version", upstream_version)
        .with_metadata_field("stable", stable)
        .with_metadata_field("yanked", yanked);
    if let Some(reason) = release.yanked_reason {
        remote_release = remote_release.with_metadata_field("yanked_reason", reason);
    }

    Ok(remote_release)
}

fn checksums_for_iac_release(
    tool: IacTool,
    version: &str,
    checksums_by_version: &BTreeMap<String, String>,
) -> CoreResult<BTreeMap<String, String>> {
    let checksums = checksums_by_version.get(version).ok_or_else(|| {
        CoreError::message(format!(
            "invalid {} official metadata: missing checksum payload for {}",
            tool.display_name(),
            version
        ))
    })?;
    parse_iac_sha256s(checksums)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IacOfficialArchive {
    filename: String,
    url: String,
    os: &'static str,
    arch: &'static str,
    platform: Platform,
}

fn terraform_archive_from_build(build: TerraformBuildPayload) -> Option<IacOfficialArchive> {
    let (os, platform_os) = official_iac_os(&build.os)?;
    let (arch, platform_arch) = official_iac_arch(&build.arch)?;
    let url = build
        .url
        .unwrap_or_else(|| format!("{TERRAFORM_OFFICIAL_BASE_URL}/unknown/{}", build.filename));

    Some(IacOfficialArchive {
        filename: build.filename,
        url,
        os,
        arch,
        platform: Platform::new(platform_os, platform_arch),
    })
}

fn opentofu_archive_from_asset(asset: OpenTofuAssetPayload) -> Option<IacOfficialArchive> {
    if !asset.name.ends_with(".zip") || !asset.name.starts_with("tofu_") {
        return None;
    }
    let filename = asset.name;
    let stem = filename.strip_suffix(".zip")?;
    let mut parts = stem.split('_').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    let arch_raw = parts.pop()?;
    let os_raw = parts.pop()?;
    let (os, platform_os) = official_iac_os(os_raw)?;
    let (arch, platform_arch) = official_iac_arch(arch_raw)?;

    Some(IacOfficialArchive {
        filename,
        url: asset.browser_download_url,
        os,
        arch,
        platform: Platform::new(platform_os, platform_arch),
    })
}

fn iac_remote_artifact_from_official_archive(
    tool: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    archive: IacOfficialArchive,
    checksums: &BTreeMap<String, String>,
) -> CoreResult<ResolvedArtifact> {
    let checksum = checksums.get(&archive.filename).ok_or_else(|| {
        CoreError::message(format!(
            "invalid IaC official metadata: archive `{}` is missing checksum",
            archive.filename
        ))
    })?;
    let archive_type = archive_type_for_iac_file(&archive.filename);
    let artifact = Artifact::new(
        archive.url.clone(),
        archive.filename.clone(),
        archive_type,
        Some(checksum.clone()),
    );

    Ok(ResolvedArtifact::new(
        tool.clone(),
        provider.clone(),
        release_version.clone(),
        archive.platform,
        artifact,
    )
    .with_metadata_field("filename", archive.filename)
    .with_metadata_field("kind", "binary")
    .with_metadata_field("iac_os", archive.os)
    .with_metadata_field("iac_arch", archive.arch))
}

fn iac_remote_artifact_from_catalog_payload(
    tool: IacTool,
    tool_name: &ToolName,
    provider: &ProviderId,
    release_version: &Version,
    artifact: IacCatalogArtifactPayload,
) -> Option<CoreResult<ResolvedArtifact>> {
    if !artifact.installable || !iac_catalog_kind_is_binary(&artifact.kind) {
        return None;
    }
    let checksum = artifact
        .checksum
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_owned();

    let result = (|| {
        let platform =
            iac_platform_from_catalog_fields(&artifact.os, &artifact.arch).ok_or_else(|| {
                CoreError::message(format!(
                    "invalid {} catalog metadata: unsupported platform {}-{} for `{}`",
                    tool.display_name(),
                    artifact.os,
                    artifact.arch,
                    artifact.filename
                ))
            })?;
        if !checksum.starts_with("sha256:") {
            return Err(CoreError::message(format!(
                "invalid {} catalog metadata: artifact `{}` checksum must use sha256:<hex>",
                tool.display_name(),
                artifact.filename
            )));
        }
        let archive_type = archive_type_for_iac_file(&artifact.filename);
        let mut resolved_artifact = Artifact::new(
            artifact.url,
            artifact.filename.clone(),
            archive_type,
            Some(checksum),
        );
        if let Some(size) = artifact.size {
            resolved_artifact = resolved_artifact.with_size(size);
        }

        Ok(ResolvedArtifact::new(
            tool_name.clone(),
            provider.clone(),
            release_version.clone(),
            platform,
            resolved_artifact,
        )
        .with_metadata_field("filename", artifact.filename)
        .with_metadata_field("kind", "binary")
        .with_metadata_field("catalog_kind", artifact.kind)
        .with_metadata_field("iac_os", normalized_iac_catalog_os(&artifact.os))
        .with_metadata_field("iac_arch", normalized_iac_catalog_arch(&artifact.arch)))
    })();

    Some(result)
}

fn official_iac_os(value: &str) -> Option<(&'static str, OperatingSystem)> {
    match value {
        "darwin" => Some(("darwin", OperatingSystem::Macos)),
        "linux" => Some(("linux", OperatingSystem::Linux)),
        "windows" => Some(("windows", OperatingSystem::Windows)),
        _ => None,
    }
}

fn official_iac_arch(value: &str) -> Option<(&'static str, Architecture)> {
    match value {
        "amd64" => Some(("amd64", Architecture::X64)),
        "arm64" => Some(("arm64", Architecture::Arm64)),
        _ => None,
    }
}

fn iac_release_from_remote_release(release: &RemoteRelease) -> CoreResult<IacRelease> {
    let stable = release.metadata_field("stable") != Some("false")
        && release.metadata_field("yanked") != Some("true");
    let files = release
        .artifacts()
        .iter()
        .map(iac_release_file_from_remote_artifact)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(IacRelease {
        version: release.version().clone(),
        stable,
        files,
    })
}

fn iac_release_file_from_remote_artifact(
    resolved: &ResolvedArtifact,
) -> CoreResult<IacReleaseFile> {
    let artifact = resolved.artifact();
    let filename = resolved
        .metadata_field("filename")
        .unwrap_or_else(|| artifact.filename())
        .to_owned();
    let os = resolved
        .metadata_field("iac_os")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| iac_artifact_os(resolved.platform()).to_owned());
    let arch = resolved
        .metadata_field("iac_arch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| iac_artifact_arch(resolved.platform()).to_owned());
    let kind = resolved
        .metadata_field("kind")
        .unwrap_or("binary")
        .to_owned();

    Ok(IacReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256: artifact.checksum().map(ToOwned::to_owned),
        size: artifact.size(),
        url: Some(artifact.url().to_owned()),
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

fn iac_catalog_kind_is_binary(kind: &str) -> bool {
    matches!(kind, "binary" | "single-binary")
}

fn iac_platform_from_catalog_fields(os: &str, arch: &str) -> Option<Platform> {
    let os = match os {
        "darwin" | "macos" => OperatingSystem::Macos,
        "linux" => OperatingSystem::Linux,
        "windows" | "win" => OperatingSystem::Windows,
        _ => return None,
    };
    let arch = match arch {
        "amd64" | "x64" => Architecture::X64,
        "arm64" => Architecture::Arm64,
        _ => return None,
    };

    Some(Platform::new(os, arch))
}

fn normalized_iac_catalog_os(os: &str) -> String {
    match os {
        "macos" => "darwin".to_owned(),
        "win" => "windows".to_owned(),
        value => value.to_owned(),
    }
}

fn normalized_iac_catalog_arch(arch: &str) -> String {
    match arch {
        "x64" => "amd64".to_owned(),
        value => value.to_owned(),
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

fn archive_type_for_iac_file(filename: &str) -> ArchiveType {
    if filename.ends_with(".zip") {
        ArchiveType::Zip
    } else {
        ArchiveType::PlainFile
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
