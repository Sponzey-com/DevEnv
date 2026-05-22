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
    ActivationPlan, Architecture, ArchiveType, Artifact, DomainError, DownloadedArtifact, EnvDelta,
    EnvOperation, ExtractionManifest, InstallPlan, InstallTransaction, Installation,
    InstallationMetadata, NormalizedVersion, OperatingSystem, Platform, RegisteredRuntime,
    ShimSpec, ToolDistribution, ToolName, ToolSpec, Version, VersionRequirement, VersionScheme,
};
pub use error::{CoreError, CoreResult};
pub use fakes::{
    FakeActivationRenderer, FakeArchiveExtractor, FakeChecksumVerifier, FakeCommandRunner,
    FakeDownloader, FakeInstallTransactionManager, FakePlatformDetector, FakeShimWriter,
    FakeToolAdapter, InMemoryConfigRepository, InMemoryInstallStore, InMemoryLockManager,
    InMemoryRuntimeRegistry, StaticArtifactResolver, StaticClock, StaticVersionSource,
};
pub use ports::{
    ActivationRenderer, ArchiveExtractor, ArtifactResolver, ChecksumVerifier, Clock,
    CommandInvocation, CommandOutput, CommandRunner, ConfigRepository, Downloader, InstallStore,
    InstallTransactionManager, InstalledRuntimeValidator, LockKey, LockManager, PlatformDetector,
    RuntimeRegistry, ShimWriter, ToolAdapter, ToolMetadata, VersionMatcher, VersionSource,
};
pub use usecases::{
    ACTIVE_SHIM_ENV, ExecCommand, InstallRuntimePorts, InstallRuntimeRequest,
    activation_plan_for_selected_runtime, add_external_runtime, collect_shim_specs,
    dispatch_shim_command, install_lock_key, install_runtime, list_remote_versions, rehash_shims,
    remove_external_runtime, tool_for_shim_binary, uninstall_runtime, validate_archive_manifest,
};

pub const PRODUCT_NAME: &str = "DevEnv";

pub fn product_name() -> &'static str {
    PRODUCT_NAME
}
