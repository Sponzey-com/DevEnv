use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    InvalidToolName { value: String },
    InvalidProviderId { value: String },
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
            Self::InvalidProviderId { value } => {
                write!(
                    formatter,
                    "invalid provider id `{value}`: expected a non-empty provider id"
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
                    "invalid tool spec `{value}`: expected a compact tool selector such as java@17"
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            return Err(DomainError::InvalidProviderId {
                value: value.as_ref().to_owned(),
            });
        }

        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
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
    TarXz,
    Zip,
    PlainFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportLevel {
    Direct,
    Delegated,
    LocalOnly,
}

impl SupportLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Delegated => "delegated",
            Self::LocalOnly => "local-only",
        }
    }

    pub fn supports_direct_install(&self) -> bool {
        matches!(self, Self::Direct)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumPolicy {
    Required,
    Optional,
    Unavailable,
}

impl ChecksumPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSourceKind {
    OfficialApi,
    StaticIndex,
    ChecksumFile,
    DelegatedCommand,
    LocalFixture,
    Catalog,
}

impl ProviderSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OfficialApi => "official-api",
            Self::StaticIndex => "static-index",
            Self::ChecksumFile => "checksum-file",
            Self::DelegatedCommand => "delegated-command",
            Self::LocalFixture => "local-fixture",
            Self::Catalog => "catalog",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSelectorDimension {
    Distribution,
    Channel,
    Implementation,
    PackageType,
    ImageType,
}

impl ProviderSelectorDimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Distribution => "distribution",
            Self::Channel => "channel",
            Self::Implementation => "implementation",
            Self::PackageType => "package-type",
            Self::ImageType => "image-type",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSupport {
    platforms: Vec<Platform>,
}

impl PlatformSupport {
    pub fn new(platforms: impl IntoIterator<Item = Platform>) -> Self {
        let mut unique = Vec::new();
        for platform in platforms {
            if !unique.contains(&platform) {
                unique.push(platform);
            }
        }
        Self { platforms: unique }
    }

    pub fn empty() -> Self {
        Self {
            platforms: Vec::new(),
        }
    }

    pub fn platforms(&self) -> &[Platform] {
        &self.platforms
    }

    pub fn supports(&self, platform: Platform) -> bool {
        self.platforms.contains(&platform)
    }

    pub fn is_empty(&self) -> bool {
        self.platforms.is_empty()
    }
}

impl Default for PlatformSupport {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapability {
    tool: ToolName,
    provider: ProviderId,
    display_name: String,
    support_level: SupportLevel,
    source_kind: ProviderSourceKind,
    checksum_policy: ChecksumPolicy,
    selector_dimensions: Vec<ProviderSelectorDimension>,
    platform_support: PlatformSupport,
    unavailable_reason: Option<String>,
    next_action: Option<String>,
}

impl ProviderCapability {
    pub fn new(
        tool: ToolName,
        provider: ProviderId,
        display_name: impl Into<String>,
        support_level: SupportLevel,
        source_kind: ProviderSourceKind,
        checksum_policy: ChecksumPolicy,
    ) -> Self {
        Self {
            tool,
            provider,
            display_name: display_name.into(),
            support_level,
            source_kind,
            checksum_policy,
            selector_dimensions: Vec::new(),
            platform_support: PlatformSupport::empty(),
            unavailable_reason: None,
            next_action: None,
        }
    }

    pub fn with_selector_dimension(mut self, dimension: ProviderSelectorDimension) -> Self {
        if !self.selector_dimensions.contains(&dimension) {
            self.selector_dimensions.push(dimension);
        }
        self
    }

    pub fn with_supported_platforms(
        mut self,
        platforms: impl IntoIterator<Item = Platform>,
    ) -> Self {
        self.platform_support = PlatformSupport::new(platforms);
        self
    }

    pub fn with_unavailable_reason(mut self, reason: impl Into<String>) -> Self {
        self.unavailable_reason = Some(reason.into());
        self
    }

    pub fn with_next_action(mut self, next_action: impl Into<String>) -> Self {
        self.next_action = Some(next_action.into());
        self
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn support_level(&self) -> SupportLevel {
        self.support_level
    }

    pub fn source_kind(&self) -> ProviderSourceKind {
        self.source_kind
    }

    pub fn checksum_policy(&self) -> ChecksumPolicy {
        self.checksum_policy
    }

    pub fn selector_dimensions(&self) -> &[ProviderSelectorDimension] {
        &self.selector_dimensions
    }

    pub fn supports_selector_dimension(&self, dimension: ProviderSelectorDimension) -> bool {
        self.selector_dimensions.contains(&dimension)
    }

    pub fn platform_support(&self) -> &PlatformSupport {
        &self.platform_support
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }

    pub fn next_action(&self) -> Option<&str> {
        self.next_action.as_deref()
    }

    pub fn direct_install_unavailable_reason(&self) -> Option<String> {
        match self.support_level {
            SupportLevel::Direct => None,
            SupportLevel::Delegated => {
                Some(self.unavailable_reason.clone().unwrap_or_else(|| {
                    "installation is delegated to an external manager".to_owned()
                }))
            }
            SupportLevel::LocalOnly => Some(
                self.unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "remote install is not supported for this tool".to_owned()),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderRegistry {
    providers: Vec<ProviderCapability>,
}

impl ProviderRegistry {
    pub fn new(providers: impl IntoIterator<Item = ProviderCapability>) -> Self {
        Self {
            providers: providers.into_iter().collect(),
        }
    }

    pub fn providers(&self) -> &[ProviderCapability] {
        &self.providers
    }

    pub fn providers_for_tool(&self, tool: &ToolName) -> Vec<&ProviderCapability> {
        self.providers
            .iter()
            .filter(|provider| provider.tool() == tool)
            .collect()
    }

    pub fn find(&self, tool: &ToolName, provider: &ProviderId) -> Option<&ProviderCapability> {
        self.providers
            .iter()
            .find(|capability| capability.tool() == tool && capability.provider() == provider)
    }
}

pub const METADATA_CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataCacheKey {
    tool: ToolName,
    provider: ProviderId,
    selector: Option<String>,
}

impl MetadataCacheKey {
    pub fn new(tool: ToolName, provider: ProviderId) -> Self {
        Self {
            tool,
            provider,
            selector: None,
        }
    }

    pub fn with_selector(mut self, selector: impl Into<String>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn selector(&self) -> Option<&str> {
        self.selector.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPayloadKind {
    Raw,
    Normalized,
}

impl MetadataPayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Normalized => "normalized",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataCacheEntry {
    key: MetadataCacheKey,
    source_url: String,
    fetched_at: String,
    ttl_seconds: u64,
    validator_metadata: BTreeMap<String, String>,
    payload_sha256: String,
    payload_kind: MetadataPayloadKind,
    payload: String,
}

impl MetadataCacheEntry {
    pub fn new(
        key: MetadataCacheKey,
        source_url: impl Into<String>,
        fetched_at: impl Into<String>,
        ttl_seconds: u64,
        payload_sha256: impl Into<String>,
        payload_kind: MetadataPayloadKind,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            key,
            source_url: source_url.into(),
            fetched_at: fetched_at.into(),
            ttl_seconds,
            validator_metadata: BTreeMap::new(),
            payload_sha256: payload_sha256.into(),
            payload_kind,
            payload: payload.into(),
        }
    }

    pub fn with_validator_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.validator_metadata.insert(key.into(), value.into());
        self
    }

    pub fn cache_key(&self) -> &MetadataCacheKey {
        &self.key
    }

    pub fn source_url(&self) -> &str {
        &self.source_url
    }

    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    pub fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds
    }

    pub fn validator_metadata(&self) -> &BTreeMap<String, String> {
        &self.validator_metadata
    }

    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }

    pub fn payload_kind(&self) -> MetadataPayloadKind {
        self.payload_kind
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn freshness_at(&self, now: &str) -> MetadataFreshness {
        let Some(fetched_at) = parse_unix_timestamp(self.fetched_at()) else {
            return MetadataFreshness::Corrupt;
        };
        let Some(now) = parse_unix_timestamp(now) else {
            return MetadataFreshness::Corrupt;
        };

        if now <= fetched_at.saturating_add(self.ttl_seconds) {
            MetadataFreshness::Fresh
        } else {
            MetadataFreshness::Stale
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFreshness {
    Fresh,
    Stale,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataCacheStatus {
    Missing,
    Fresh(MetadataCacheEntry),
    Stale(MetadataCacheEntry),
    Corrupt { reason: String },
}

impl MetadataCacheStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Fresh(_) => "fresh",
            Self::Stale(_) => "stale",
            Self::Corrupt { .. } => "corrupt",
        }
    }
}

pub const CATALOG_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPayloadKind {
    NormalizedReleaseIndex,
}

impl CatalogPayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NormalizedReleaseIndex => "normalized-release-index",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPayloadDescriptor {
    path: String,
    sha256: String,
    payload_kind: CatalogPayloadKind,
    ttl_seconds: u64,
}

impl CatalogPayloadDescriptor {
    pub fn new(
        path: impl Into<String>,
        sha256: impl Into<String>,
        payload_kind: CatalogPayloadKind,
        ttl_seconds: u64,
    ) -> Self {
        Self {
            path: path.into(),
            sha256: sha256.into(),
            payload_kind,
            ttl_seconds,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn payload_kind(&self) -> CatalogPayloadKind {
        self.payload_kind
    }

    pub fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    key: MetadataCacheKey,
    descriptor: CatalogPayloadDescriptor,
}

impl CatalogEntry {
    pub fn new(key: MetadataCacheKey, descriptor: CatalogPayloadDescriptor) -> Self {
        Self { key, descriptor }
    }

    pub fn cache_key(&self) -> &MetadataCacheKey {
        &self.key
    }

    pub fn tool(&self) -> &ToolName {
        self.key.tool()
    }

    pub fn provider(&self) -> &ProviderId {
        self.key.provider()
    }

    pub fn descriptor(&self) -> &CatalogPayloadDescriptor {
        &self.descriptor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifest {
    schema_version: u32,
    catalog_id: String,
    generated_at: String,
    expires_at: String,
    catalog_version: String,
    min_devenv_version: String,
    sequence: u64,
    entries: Vec<CatalogEntry>,
}

impl CatalogManifest {
    pub fn new(
        catalog_id: impl Into<String>,
        generated_at: impl Into<String>,
        expires_at: impl Into<String>,
        catalog_version: impl Into<String>,
        min_devenv_version: impl Into<String>,
        sequence: u64,
        entries: impl IntoIterator<Item = CatalogEntry>,
    ) -> Self {
        Self {
            schema_version: CATALOG_MANIFEST_SCHEMA_VERSION,
            catalog_id: catalog_id.into(),
            generated_at: generated_at.into(),
            expires_at: expires_at.into(),
            catalog_version: catalog_version.into(),
            min_devenv_version: min_devenv_version.into(),
            sequence,
            entries: entries.into_iter().collect(),
        }
    }

    pub fn with_schema_version(mut self, schema_version: u32) -> Self {
        self.schema_version = schema_version;
        self
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn catalog_id(&self) -> &str {
        &self.catalog_id
    }

    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }

    pub fn catalog_version(&self) -> &str {
        &self.catalog_version
    }

    pub fn min_devenv_version(&self) -> &str {
        &self.min_devenv_version
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    pub fn entry_for(&self, key: &MetadataCacheKey) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.cache_key() == key)
    }

    pub fn is_expired_at(&self, now: &str) -> bool {
        matches!(
            compare_catalog_timestamps(now, self.expires_at()),
            Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
        )
    }

    pub fn requires_newer_devenv(&self, current_version: &str) -> bool {
        catalog_version_is_greater(self.min_devenv_version(), current_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogFetchRequest {
    reference: String,
    fetch_mode: MetadataFetchMode,
    max_manifest_bytes: usize,
    max_payload_bytes: usize,
}

impl CatalogFetchRequest {
    pub fn new(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
            fetch_mode: MetadataFetchMode::Online,
            max_manifest_bytes: 1024 * 1024,
            max_payload_bytes: 4 * 1024 * 1024,
        }
    }

    pub fn with_fetch_mode(mut self, fetch_mode: MetadataFetchMode) -> Self {
        self.fetch_mode = fetch_mode;
        self
    }

    pub fn with_max_manifest_bytes(mut self, bytes: usize) -> Self {
        self.max_manifest_bytes = bytes;
        self
    }

    pub fn with_max_payload_bytes(mut self, bytes: usize) -> Self {
        self.max_payload_bytes = bytes;
        self
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn fetch_mode(&self) -> MetadataFetchMode {
        self.fetch_mode
    }

    pub fn max_manifest_bytes(&self) -> usize {
        self.max_manifest_bytes
    }

    pub fn max_payload_bytes(&self) -> usize {
        self.max_payload_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogFetchResponse {
    source_reference: String,
    fetched_at: String,
    bytes: Vec<u8>,
}

impl CatalogFetchResponse {
    pub fn new(
        source_reference: impl Into<String>,
        fetched_at: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            source_reference: source_reference.into(),
            fetched_at: fetched_at.into(),
            bytes: bytes.into(),
        }
    }

    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }

    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustRoot {
    id: String,
    fingerprint: String,
}

impl TrustRoot {
    pub fn new(id: impl Into<String>, fingerprint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            fingerprint: fingerprint.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogTrustFailure {
    UnknownTrustRoot {
        trust_root_id: String,
    },
    SignatureMismatch {
        reason: String,
    },
    ExpiredCatalog {
        catalog_id: String,
        expires_at: String,
        now: String,
    },
    MinDevenvVersionMismatch {
        required: String,
        current: String,
    },
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    UnsupportedSchemaVersion {
        expected: u32,
        actual: u32,
    },
}

impl CatalogTrustFailure {
    pub fn is_trust_failure(&self) -> bool {
        true
    }
}

impl fmt::Display for CatalogTrustFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTrustRoot { trust_root_id } => {
                write!(formatter, "unknown catalog trust root `{trust_root_id}`")
            }
            Self::SignatureMismatch { reason } => {
                write!(formatter, "catalog signature mismatch: {reason}")
            }
            Self::ExpiredCatalog {
                catalog_id,
                expires_at,
                now,
            } => write!(
                formatter,
                "catalog `{catalog_id}` expired at {expires_at}; current time is {now}"
            ),
            Self::MinDevenvVersionMismatch { required, current } => write!(
                formatter,
                "catalog requires DevEnv {required} or newer; current version is {current}"
            ),
            Self::ChecksumMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "catalog payload `{path}` checksum mismatch: expected {expected}, actual {actual}"
            ),
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported catalog schema version {actual}: expected {expected}"
            ),
        }
    }
}

impl std::error::Error for CatalogTrustFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogVerificationResult {
    Trusted { trust_root_id: String },
    Rejected { failure: CatalogTrustFailure },
}

impl CatalogVerificationResult {
    pub fn trusted(trust_root_id: impl Into<String>) -> Self {
        Self::Trusted {
            trust_root_id: trust_root_id.into(),
        }
    }

    pub fn rejected(failure: CatalogTrustFailure) -> Self {
        Self::Rejected { failure }
    }

    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Trusted { .. })
    }

    pub fn trust_root_id(&self) -> Option<&str> {
        match self {
            Self::Trusted { trust_root_id } => Some(trust_root_id),
            Self::Rejected { .. } => None,
        }
    }

    pub fn failure(&self) -> Option<&CatalogTrustFailure> {
        match self {
            Self::Trusted { .. } => None,
            Self::Rejected { failure } => Some(failure),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataHttpRequest {
    url: String,
    headers: BTreeMap<String, String>,
    timeout_seconds: u64,
    max_body_bytes: usize,
}

impl MetadataHttpRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: BTreeMap::new(),
            timeout_seconds: 10,
            max_body_bytes: 2 * 1024 * 1024,
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn with_timeout_seconds(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    pub fn with_max_body_bytes(mut self, bytes: usize) -> Self {
        self.max_body_bytes = bytes;
        self
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }

    pub fn max_body_bytes(&self) -> usize {
        self.max_body_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataHttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl MetadataHttpResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: BTreeMap::new(),
            body: body.into(),
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn into_body(self) -> Vec<u8> {
        self.body
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFetchMode {
    Online,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataFetchOutcome {
    Fetched(MetadataHttpResponse),
    NotModified { headers: BTreeMap<String, String> },
    Offline { reason: String },
}

fn parse_unix_timestamp(value: &str) -> Option<u64> {
    value.strip_prefix("unix:")?.parse().ok()
}

fn compare_catalog_timestamps(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    match (
        parse_catalog_timestamp_as_unix(left),
        parse_catalog_timestamp_as_unix(right),
    ) {
        (Some(left), Some(right)) => return Some(left.cmp(&right)),
        (Some(_), None) | (None, Some(_)) => return None,
        (None, None) => {}
    }

    if is_lexicographic_utc_timestamp(left) && is_lexicographic_utc_timestamp(right) {
        Some(left.cmp(right))
    } else {
        None
    }
}

fn parse_catalog_timestamp_as_unix(value: &str) -> Option<u64> {
    parse_unix_timestamp(value).or_else(|| parse_utc_timestamp_as_unix(value))
}

fn parse_utc_timestamp_as_unix(value: &str) -> Option<u64> {
    if value.len() != "0000-00-00T00:00:00Z".len() || !is_lexicographic_utc_timestamp(value) {
        return None;
    }

    let year = parse_timestamp_part(value, 0, 4)?;
    let month = parse_timestamp_part(value, 5, 7)?;
    let day = parse_timestamp_part(value, 8, 10)?;
    let hour = parse_timestamp_part(value, 11, 13)?;
    let minute = parse_timestamp_part(value, 14, 16)?;
    let second = parse_timestamp_part(value, 17, 19)?;

    if year < 1970 || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let month_days = days_in_month(year, month)?;
    if day == 0 || day > month_days {
        return None;
    }

    let mut days = 0_u64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += u64::from(days_in_month(year, m)?);
    }
    days += u64::from(day - 1);

    Some(days * 86_400 + u64::from(hour) * 3_600 + u64::from(minute) * 60 + u64::from(second))
}

fn parse_timestamp_part(value: &str, start: usize, end: usize) -> Option<u32> {
    value.get(start..end)?.parse().ok()
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn is_leap_year(year: u32) -> bool {
    year % 4 == 0 && year % 100 != 0 || year % 400 == 0
}

fn is_lexicographic_utc_timestamp(value: &str) -> bool {
    value.len() >= "0000-00-00T00:00:00Z".len()
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.ends_with('Z')
}

fn catalog_version_is_greater(required: &str, current: &str) -> bool {
    let required = catalog_version_segments(required);
    let current = catalog_version_segments(current);

    if required.is_empty() || current.is_empty() {
        return false;
    }

    let length = required.len().max(current.len());
    for index in 0..length {
        let required = required.get(index).copied().unwrap_or_default();
        let current = current.get(index).copied().unwrap_or_default();
        match required.cmp(&current) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    false
}

fn catalog_version_segments(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .split('.')
        .filter_map(|segment| {
            let digits = segment
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>();
            if digits.is_empty() {
                None
            } else {
                digits.parse().ok()
            }
        })
        .collect()
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
pub struct ResolvedArtifact {
    tool: ToolName,
    provider: ProviderId,
    version: Version,
    platform: Platform,
    artifact: Artifact,
    metadata_fields: BTreeMap<String, String>,
}

impl ResolvedArtifact {
    pub fn new(
        tool: ToolName,
        provider: ProviderId,
        version: Version,
        platform: Platform,
        artifact: Artifact,
    ) -> Self {
        Self {
            tool,
            provider,
            version,
            platform,
            artifact,
            metadata_fields: BTreeMap::new(),
        }
    }

    pub fn with_metadata_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata_fields.insert(key.into(), value.into());
        self
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
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

    pub fn metadata_fields(&self) -> &BTreeMap<String, String> {
        &self.metadata_fields
    }

    pub fn metadata_field(&self, key: &str) -> Option<&str> {
        self.metadata_fields.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRelease {
    version: Version,
    artifacts: Vec<ResolvedArtifact>,
    metadata_fields: BTreeMap<String, String>,
}

impl RemoteRelease {
    pub fn new(version: Version, artifacts: impl IntoIterator<Item = ResolvedArtifact>) -> Self {
        Self {
            version,
            artifacts: artifacts.into_iter().collect(),
            metadata_fields: BTreeMap::new(),
        }
    }

    pub fn with_metadata_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata_fields.insert(key.into(), value.into());
        self
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn artifacts(&self) -> &[ResolvedArtifact] {
        &self.artifacts
    }

    pub fn metadata_field(&self, key: &str) -> Option<&str> {
        self.metadata_fields.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReleaseIndex {
    tool: ToolName,
    provider: ProviderId,
    releases: Vec<RemoteRelease>,
}

impl RemoteReleaseIndex {
    pub fn new(
        tool: ToolName,
        provider: ProviderId,
        releases: impl IntoIterator<Item = RemoteRelease>,
    ) -> Self {
        Self {
            tool,
            provider,
            releases: releases.into_iter().collect(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn releases(&self) -> &[RemoteRelease] {
        &self.releases
    }

    pub fn release_for_version(&self, version: &Version) -> Option<&RemoteRelease> {
        self.releases
            .iter()
            .find(|release| release.version() == version)
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
