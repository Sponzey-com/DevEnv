mod config;
mod domain;
mod error;
mod fakes;
mod ports;
mod usecases;

pub use config::{
    ConfigFormat, ConfigScope, ConfigSource, ProjectConfig, ResolvedSelection, SelectionCandidate,
    SelectionSource, ToolConfig, parse_devenv_toml, parse_go_version, parse_java_version,
    parse_node_version, parse_nvmrc, parse_python_version, parse_ruby_version, parse_tool_versions,
    resolve_tool_selection,
};
pub use domain::{
    ActivationPlan, Architecture, ArchiveType, Artifact, CATALOG_MANIFEST_SCHEMA_VERSION,
    CatalogEntry, CatalogFetchRequest, CatalogFetchResponse, CatalogManifest,
    CatalogPayloadDescriptor, CatalogPayloadKind, CatalogTrustFailure, CatalogVerificationResult,
    ChecksumPolicy, DomainError, DownloadedArtifact, EnvDelta, EnvOperation, ExtractionManifest,
    InstallPlan, InstallTransaction, Installation, InstallationMetadata,
    METADATA_CACHE_SCHEMA_VERSION, MetadataCacheEntry, MetadataCacheKey, MetadataCacheStatus,
    MetadataFetchMode, MetadataFetchOutcome, MetadataFreshness, MetadataHttpRequest,
    MetadataHttpResponse, MetadataPayloadKind, NormalizedVersion, OperatingSystem, Platform,
    PlatformSupport, ProviderCapability, ProviderId, ProviderRegistry, ProviderSelectorDimension,
    ProviderSourceKind, RegisteredRuntime, RemoteRelease, RemoteReleaseIndex, ResolvedArtifact,
    ShimSpec, SupportLevel, ToolDistribution, ToolName, ToolSpec, TrustRoot, Version,
    VersionRequirement, VersionScheme,
};
pub use error::{CoreError, CoreResult};
pub use fakes::{
    CatalogTrustVerificationCall, FakeActivationRenderer, FakeArchiveExtractor, FakeCatalogSource,
    FakeCatalogTrustVerifier, FakeChecksumVerifier, FakeCommandRunner, FakeDownloader,
    FakeInstallTransactionManager, FakeMetadataHttpClient, FakePlatformDetector, FakeShimWriter,
    FakeToolAdapter, InMemoryConfigRepository, InMemoryInstallStore, InMemoryLockManager,
    InMemoryRuntimeRegistry, StaticArtifactResolver, StaticClock, StaticVersionSource,
};
pub use ports::{
    ActivationRenderer, ArchiveExtractor, ArtifactResolver, CatalogSource, CatalogTrustVerifier,
    ChecksumVerifier, Clock, CommandInvocation, CommandOutput, CommandRunner, ConfigRepository,
    Downloader, InstallStore, InstallTransactionManager, InstalledRuntimeValidator, LockKey,
    LockManager, MetadataCache, MetadataHttpClient, PlatformDetector, RuntimeRegistry, ShimWriter,
    ToolAdapter, ToolMetadata, VersionMatcher, VersionSource,
};
pub use usecases::{
    ACTIVE_SHIM_ENV, ExecCommand, InstallRuntimePorts, InstallRuntimeRequest,
    MetadataPayloadFetchRequest, activation_plan_for_selected_runtime, add_external_runtime,
    collect_shim_specs, dispatch_shim_command, fetch_metadata_payload, install_lock_key,
    install_runtime, list_remote_versions, plan_install_runtime, rehash_shims,
    remove_external_runtime, tool_for_shim_binary, uninstall_runtime, validate_archive_manifest,
};

pub const PRODUCT_NAME: &str = "DevEnv";

pub fn product_name() -> &'static str {
    PRODUCT_NAME
}
