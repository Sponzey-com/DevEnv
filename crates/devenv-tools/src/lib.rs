pub mod flutter;
pub mod go;
pub mod iac;
pub mod java;
pub mod node;
pub mod php;
pub mod providers;
pub mod python;
pub mod ruby;
pub mod rust;

use std::path::Path;

use devenv_core::{
    ActivationPlan, CoreResult, ToolAdapter, ToolMetadata, ToolName, Version, VersionRequirement,
    VersionScheme,
};
pub use flutter::{
    FLUTTER_OFFICIAL_BASE_URL, FLUTTER_OFFICIAL_LINUX_RELEASES_URL,
    FLUTTER_OFFICIAL_MACOS_RELEASES_URL, FLUTTER_OFFICIAL_WINDOWS_RELEASES_URL,
    FlutterArtifactResolver, FlutterInstalledRuntimeValidator, FlutterOfficialReleaseMetadata,
    FlutterRelease, FlutterReleaseFile, FlutterReleaseMetadata, FlutterReleaseVersionSource,
    FlutterRuntime, FlutterRuntimeDiscovery, FlutterRuntimeSource, FlutterToolAdapter,
    FlutterVersionMatcher, match_flutter_runtime, match_flutter_version, normalize_flutter_version,
    validate_flutter_sdk_home,
};
pub use go::{
    GO_OFFICIAL_METADATA_URL, GoArtifactResolver, GoCatalogReleaseMetadata,
    GoInstalledRuntimeValidator, GoOfficialReleaseMetadata, GoRelease, GoReleaseFile,
    GoReleaseMetadata, GoReleaseVersionSource, GoRemoteArtifactResolver,
    GoRemoteReleaseVersionSource, GoRuntime, GoRuntimeDiscovery, GoRuntimeSource, GoToolAdapter,
    GoVersionMatcher, match_go_runtime, match_go_version, normalize_go_version,
    validate_go_sdk_home,
};
pub use iac::{
    IacArtifactResolver, IacCatalogReleaseMetadata, IacOfficialReleaseMetadata, IacRelease,
    IacReleaseFile, IacReleaseMetadata, IacReleaseVersionSource, IacRuntime, IacRuntimeDiscovery,
    IacRuntimeSource, IacTool, IacVersionMatcher, OPENTOFU_OFFICIAL_BASE_URL,
    OPENTOFU_OFFICIAL_RELEASES_URL, OpenTofuInstalledRuntimeValidator, OpenTofuToolAdapter,
    TERRAFORM_OFFICIAL_BASE_URL, TERRAFORM_OFFICIAL_INDEX_URL, TerraformInstalledRuntimeValidator,
    TerraformToolAdapter, match_iac_runtime, match_iac_version, normalize_iac_version,
    parse_iac_sha256s, validate_iac_tool_home,
};
pub use java::{
    JAVA_TEMURIN_METADATA_URL_HINT, JavaArtifactResolver, JavaDistribution,
    JavaInstalledRuntimeValidator, JavaRelease, JavaReleaseFile, JavaReleaseMetadata,
    JavaReleaseVersionSource, JavaRuntime, JavaRuntimeDiscovery, JavaRuntimeSource,
    JavaTemurinReleaseMetadata, JavaToolAdapter, JavaVersionMatcher, match_java_runtime,
    match_java_version, normalize_java_version, validate_jdk_home,
};
pub use node::{
    NODE_OFFICIAL_DIST_BASE_URL, NODE_OFFICIAL_INDEX_URL, NodeArtifactResolver,
    NodeCatalogReleaseMetadata, NodeInstalledRuntimeValidator, NodeOfficialReleaseMetadata,
    NodeRelease, NodeReleaseFile, NodeReleaseMetadata, NodeReleaseVersionSource, NodeRuntime,
    NodeRuntimeDiscovery, NodeRuntimeSource, NodeToolAdapter, NodeVersionMatcher,
    match_node_runtime, match_node_version, node_official_required_shasums_versions,
    normalize_node_version, parse_node_shasums256, validate_node_home,
};
pub use php::{
    PhpRuntime, PhpRuntimeDiscovery, PhpRuntimeSource, PhpToolAdapter, PhpVersionMatcher,
    match_php_runtime, match_php_version, normalize_php_version, validate_php_home,
};
pub use providers::{builtin_provider_capabilities, builtin_provider_registry};
pub use python::{
    PythonArtifactResolver, PythonImplementation, PythonInstalledRuntimeValidator, PythonRelease,
    PythonReleaseFile, PythonReleaseMetadata, PythonReleaseVersionSource, PythonRuntime,
    PythonRuntimeDiscovery, PythonRuntimeSource, PythonToolAdapter, PythonVersionMatcher,
    match_python_runtime, match_python_version, normalize_python_version, validate_python_home,
};
pub use ruby::{
    RubyRuntime, RubyRuntimeDiscovery, RubyRuntimeSource, RubyToolAdapter, RubyVersionMatcher,
    match_ruby_runtime, match_ruby_version, normalize_ruby_version, validate_ruby_home,
};
pub use rust::{
    RustRuntime, RustRuntimeDiscovery, RustRuntimeSource, RustToolAdapter, RustVersionMatcher,
    match_rust_runtime, match_rust_version, normalize_rust_version, validate_rust_toolchain_home,
};

pub fn tools_ready() -> bool {
    true
}

pub fn builtin_tool_adapter(tool: &ToolName) -> BuiltInToolAdapter {
    match tool.as_str() {
        "java" => BuiltInToolAdapter::Java(JavaToolAdapter::new()),
        "go" => BuiltInToolAdapter::Go(GoToolAdapter::new()),
        "flutter" => BuiltInToolAdapter::Flutter(FlutterToolAdapter::new()),
        "terraform" => BuiltInToolAdapter::Terraform(TerraformToolAdapter::new()),
        "opentofu" => BuiltInToolAdapter::OpenTofu(OpenTofuToolAdapter::new()),
        "node" => BuiltInToolAdapter::Node(NodeToolAdapter::new()),
        "python" => BuiltInToolAdapter::Python(PythonToolAdapter::new()),
        "ruby" => BuiltInToolAdapter::Ruby(RubyToolAdapter::new()),
        "php" => BuiltInToolAdapter::Php(PhpToolAdapter::new()),
        "rust" => BuiltInToolAdapter::Rust(RustToolAdapter::new()),
        _ => BuiltInToolAdapter::Generic(GenericToolAdapter::new(tool.clone())),
    }
}

#[derive(Debug, Clone)]
pub enum BuiltInToolAdapter {
    Java(JavaToolAdapter),
    Go(GoToolAdapter),
    Flutter(FlutterToolAdapter),
    Terraform(TerraformToolAdapter),
    OpenTofu(OpenTofuToolAdapter),
    Node(NodeToolAdapter),
    Python(PythonToolAdapter),
    Ruby(RubyToolAdapter),
    Php(PhpToolAdapter),
    Rust(RustToolAdapter),
    Generic(GenericToolAdapter),
}

impl ToolAdapter for BuiltInToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        match self {
            Self::Java(adapter) => adapter.metadata(),
            Self::Go(adapter) => adapter.metadata(),
            Self::Flutter(adapter) => adapter.metadata(),
            Self::Terraform(adapter) => adapter.metadata(),
            Self::OpenTofu(adapter) => adapter.metadata(),
            Self::Node(adapter) => adapter.metadata(),
            Self::Python(adapter) => adapter.metadata(),
            Self::Ruby(adapter) => adapter.metadata(),
            Self::Php(adapter) => adapter.metadata(),
            Self::Rust(adapter) => adapter.metadata(),
            Self::Generic(adapter) => adapter.metadata(),
        }
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        match self {
            Self::Java(adapter) => adapter.resolve_version(requirement),
            Self::Go(adapter) => adapter.resolve_version(requirement),
            Self::Flutter(adapter) => adapter.resolve_version(requirement),
            Self::Terraform(adapter) => adapter.resolve_version(requirement),
            Self::OpenTofu(adapter) => adapter.resolve_version(requirement),
            Self::Node(adapter) => adapter.resolve_version(requirement),
            Self::Python(adapter) => adapter.resolve_version(requirement),
            Self::Ruby(adapter) => adapter.resolve_version(requirement),
            Self::Php(adapter) => adapter.resolve_version(requirement),
            Self::Rust(adapter) => adapter.resolve_version(requirement),
            Self::Generic(adapter) => adapter.resolve_version(requirement),
        }
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        match self {
            Self::Java(adapter) => adapter.activation_plan(runtime_root),
            Self::Go(adapter) => adapter.activation_plan(runtime_root),
            Self::Flutter(adapter) => adapter.activation_plan(runtime_root),
            Self::Terraform(adapter) => adapter.activation_plan(runtime_root),
            Self::OpenTofu(adapter) => adapter.activation_plan(runtime_root),
            Self::Node(adapter) => adapter.activation_plan(runtime_root),
            Self::Python(adapter) => adapter.activation_plan(runtime_root),
            Self::Ruby(adapter) => adapter.activation_plan(runtime_root),
            Self::Php(adapter) => adapter.activation_plan(runtime_root),
            Self::Rust(adapter) => adapter.activation_plan(runtime_root),
            Self::Generic(adapter) => adapter.activation_plan(runtime_root),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericToolAdapter {
    metadata: ToolMetadata,
}

impl GenericToolAdapter {
    pub fn new(tool: ToolName) -> Self {
        Self {
            metadata: ToolMetadata::new(
                tool.clone(),
                VersionScheme::Custom(tool.as_str().to_owned()),
                vec![tool.as_str().to_owned()],
            ),
        }
    }
}

impl ToolAdapter for GenericToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(requirement.raw())?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}
