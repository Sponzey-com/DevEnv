use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    InvalidToolName { value: String },
    InvalidVersion { value: String },
    InvalidToolSpec { value: String },
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToolName { value } => {
                write!(
                    formatter,
                    "invalid tool name `{value}`: expected a non-empty tool name"
                )
            }
            Self::InvalidVersion { value } => {
                write!(
                    formatter,
                    "invalid version `{value}`: expected a non-empty version"
                )
            }
            Self::InvalidToolSpec { value } => {
                write!(
                    formatter,
                    "invalid tool spec `{value}`: expected <tool>@<version>, for example java@17"
                )
            }
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            return Err(DomainError::InvalidToolName {
                value: value.as_ref().to_owned(),
            });
        }

        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(String);

impl Version {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            return Err(DomainError::InvalidVersion {
                value: value.as_ref().to_owned(),
            });
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn raw(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.raw())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedVersion(String);

impl NormalizedVersion {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            return Err(DomainError::InvalidVersion {
                value: value.as_ref().to_owned(),
            });
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolDistribution {
    Unknown,
    Named(String),
}

impl ToolDistribution {
    pub fn named(value: impl AsRef<str>) -> Self {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            Self::Unknown
        } else {
            Self::Named(trimmed.to_ascii_lowercase())
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Unknown => "unknown",
            Self::Named(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionRequirement {
    Exact(Version),
    Prefix(Version),
    Alias(String),
    Latest,
    Lts,
    DistributionAware {
        version: Version,
        distribution: ToolDistribution,
    },
}

impl VersionRequirement {
    pub fn exact(value: impl AsRef<str>) -> Result<Self, DomainError> {
        Ok(Self::Exact(Version::new(value)?))
    }

    pub fn raw(&self) -> &str {
        match self {
            Self::Exact(version) | Self::Prefix(version) => version.raw(),
            Self::Alias(alias) => alias.as_str(),
            Self::Latest => "latest",
            Self::Lts => "lts",
            Self::DistributionAware { version, .. } => version.raw(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionScheme {
    Semver,
    JavaFeatureInterimUpdate,
    GoRelease,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveType {
    TarGz,
    Zip,
    PlainFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    url: String,
    filename: String,
    archive_type: ArchiveType,
    checksum: Option<String>,
    size: Option<u64>,
}

impl Artifact {
    pub fn new(
        url: impl Into<String>,
        filename: impl Into<String>,
        archive_type: ArchiveType,
        checksum: Option<String>,
    ) -> Self {
        Self {
            url: url.into(),
            filename: filename.into(),
            archive_type,
            checksum,
            size: None,
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn archive_type(&self) -> ArchiveType {
        self.archive_type
    }

    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    tool: ToolName,
    requirement: VersionRequirement,
}

impl ToolSpec {
    pub fn new(tool: ToolName, requirement: VersionRequirement) -> Self {
        Self { tool, requirement }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }
}

impl FromStr for ToolSpec {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let Some((tool, version)) = trimmed.split_once('@') else {
            return Err(DomainError::InvalidToolSpec {
                value: value.to_owned(),
            });
        };

        if tool.is_empty() || version.is_empty() || version.contains('@') {
            return Err(DomainError::InvalidToolSpec {
                value: value.to_owned(),
            });
        }

        Ok(Self::new(
            ToolName::new(tool)?,
            VersionRequirement::exact(version)?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShimSpec {
    tool: ToolName,
    binary_name: String,
}

impl ShimSpec {
    pub fn new(tool: ToolName, binary_name: impl Into<String>) -> Self {
        Self {
            tool,
            binary_name: binary_name.into(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn binary_name(&self) -> &str {
        &self.binary_name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Macos,
    Linux,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    X64,
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Platform {
    os: OperatingSystem,
    architecture: Architecture,
}

impl Platform {
    pub fn new(os: OperatingSystem, architecture: Architecture) -> Self {
        Self { os, architecture }
    }

    pub fn os(&self) -> OperatingSystem {
        self.os
    }

    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    pub fn id(&self) -> String {
        format!("{}-{}", self.os.as_str(), self.architecture.as_str())
    }
}

impl OperatingSystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

impl Architecture {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    tool: ToolName,
    version: Version,
    platform: Platform,
    root: PathBuf,
}

impl Installation {
    pub fn new(
        tool: ToolName,
        version: Version,
        platform: Platform,
        root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            tool,
            version,
            platform,
            root: root.into(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationMetadata {
    installation: Installation,
    source: String,
    checksum: Option<String>,
    installed_at: String,
    metadata_fields: BTreeMap<String, String>,
}

impl InstallationMetadata {
    pub fn new(
        installation: Installation,
        source: impl Into<String>,
        checksum: Option<String>,
        installed_at: impl Into<String>,
    ) -> Self {
        Self {
            installation,
            source: source.into(),
            checksum,
            installed_at: installed_at.into(),
            metadata_fields: BTreeMap::new(),
        }
    }

    pub fn installation(&self) -> &Installation {
        &self.installation
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    pub fn installed_at(&self) -> &str {
        &self.installed_at
    }

    pub fn metadata_fields(&self) -> &BTreeMap<String, String> {
        &self.metadata_fields
    }

    pub fn metadata_field(&self, key: &str) -> Option<&str> {
        self.metadata_fields.get(key).map(String::as_str)
    }

    pub fn with_metadata_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata_fields.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    tool: ToolName,
    version: Version,
    platform: Platform,
    artifact: Artifact,
    install_root: PathBuf,
}

impl InstallPlan {
    pub fn new(
        tool: ToolName,
        version: Version,
        platform: Platform,
        artifact: Artifact,
        install_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            tool,
            version,
            platform,
            artifact,
            install_root: install_root.into(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTransaction {
    install_root: PathBuf,
    temp_root: PathBuf,
    download_path: PathBuf,
    extract_root: PathBuf,
}

impl InstallTransaction {
    pub fn new(
        install_root: impl Into<PathBuf>,
        temp_root: impl Into<PathBuf>,
        download_path: impl Into<PathBuf>,
        extract_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            install_root: install_root.into(),
            temp_root: temp_root.into(),
            download_path: download_path.into(),
            extract_root: extract_root.into(),
        }
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    pub fn temp_root(&self) -> &Path {
        &self.temp_root
    }

    pub fn download_path(&self) -> &Path {
        &self.download_path
    }

    pub fn extract_root(&self) -> &Path {
        &self.extract_root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedArtifact {
    path: PathBuf,
    size: u64,
}

impl DownloadedArtifact {
    pub fn new(path: impl Into<PathBuf>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtractionManifest {
    entries: Vec<PathBuf>,
}

impl ExtractionManifest {
    pub fn new<I, P>(entries: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            entries: entries.into_iter().map(Into::into).collect(),
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredRuntime {
    tool: ToolName,
    version: Version,
    platform: Platform,
    root: PathBuf,
}

impl RegisteredRuntime {
    pub fn new(
        tool: ToolName,
        version: Version,
        platform: Platform,
        root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            tool,
            version,
            platform,
            root: root.into(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvOperation {
    Set { key: String, value: String },
    Unset { key: String },
    PrependPath { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvDelta {
    sets: BTreeMap<String, String>,
    unsets: BTreeSet<String>,
}

impl EnvDelta {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.unsets.remove(&key);
        self.sets.insert(key, value.into());
        self
    }

    pub fn unset(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.sets.remove(&key);
        self.unsets.insert(key);
        self
    }

    pub fn sets(&self) -> &BTreeMap<String, String> {
        &self.sets
    }

    pub fn unsets(&self) -> &BTreeSet<String> {
        &self.unsets
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty() && self.unsets.is_empty()
    }

    pub fn apply_to(&self, environment: &mut BTreeMap<String, String>) {
        for key in &self.unsets {
            environment.remove(key);
        }

        for (key, value) in &self.sets {
            environment.insert(key.clone(), value.clone());
        }
    }

    fn between(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> EnvDelta {
        let mut delta = EnvDelta::new();

        for key in before.keys() {
            if !after.contains_key(key) {
                delta = delta.unset(key.clone());
            }
        }

        for (key, value) in after {
            if before.get(key) != Some(value) {
                delta = delta.set(key.clone(), value.clone());
            }
        }

        delta
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivationPlan {
    operations: Vec<EnvOperation>,
}

impl ActivationPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_operation(mut self, operation: EnvOperation) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn extend(mut self, other: ActivationPlan) -> Self {
        self.operations.extend(other.operations);
        self
    }

    pub fn set_env(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.with_operation(EnvOperation::Set {
            key: key.into(),
            value: value.into(),
        })
    }

    pub fn unset_env(self, key: impl Into<String>) -> Self {
        self.with_operation(EnvOperation::Unset { key: key.into() })
    }

    pub fn prepend_path(self, path: impl Into<PathBuf>) -> Self {
        self.with_operation(EnvOperation::PrependPath { path: path.into() })
    }

    pub fn operations(&self) -> &[EnvOperation] {
        &self.operations
    }

    pub fn env_delta(&self, environment: &BTreeMap<String, String>) -> EnvDelta {
        let mut applied = environment.clone();
        let mut path_prefixes = Vec::new();

        for operation in &self.operations {
            match operation {
                EnvOperation::Set { key, value } => {
                    applied.insert(key.clone(), value.clone());
                }
                EnvOperation::Unset { key } => {
                    applied.remove(key);
                }
                EnvOperation::PrependPath { path } => {
                    path_prefixes.push(path.clone());
                }
            }
        }

        if !path_prefixes.is_empty() {
            let existing_path = applied.get("PATH").map_or("", String::as_str);
            applied.insert(
                "PATH".to_owned(),
                prepend_path_entries(&path_prefixes, existing_path),
            );
        }

        EnvDelta::between(environment, &applied)
    }

    pub fn apply_to(&self, environment: &mut BTreeMap<String, String>) -> EnvDelta {
        let delta = self.env_delta(environment);
        delta.apply_to(environment);
        delta
    }
}

fn prepend_path_entries(prefixes: &[PathBuf], existing_path: &str) -> String {
    let mut entries = Vec::new();

    for path in prefixes {
        push_unique_path_entry(&mut entries, path.to_string_lossy().as_ref());
    }

    for entry in split_path_entries(existing_path) {
        push_unique_path_entry(&mut entries, entry);
    }

    entries.join(path_separator())
}

fn push_unique_path_entry(entries: &mut Vec<String>, entry: &str) {
    if entry.is_empty() {
        return;
    }

    if !entries.iter().any(|existing| existing == entry) {
        entries.push(entry.to_owned());
    }
}

fn split_path_entries(path: &str) -> impl Iterator<Item = &str> {
    path.split(path_separator())
        .filter(|entry| !entry.is_empty())
}

fn path_separator() -> &'static str {
    if cfg!(windows) { ";" } else { ":" }
}
