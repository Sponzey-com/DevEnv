use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    IacRuntimeDiscovery, IacRuntimeSource, IacTool, IacVersionMatcher,
    OpenTofuInstalledRuntimeValidator, OpenTofuToolAdapter, TerraformInstalledRuntimeValidator,
    TerraformToolAdapter, normalize_iac_version, validate_iac_tool_home,
};

#[test]
fn fake_terraform_single_binary_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = IacRuntimeDiscovery::new(IacTool::Terraform)
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].tool(), IacTool::Terraform);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "1.8.5");
    assert_eq!(runtimes[0].source(), &IacRuntimeSource::CandidatePath);
}

#[test]
fn fake_opentofu_single_binary_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "opentofu-1.7.2", "tofu", "1.7.2");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = IacRuntimeDiscovery::new(IacTool::OpenTofu)
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].tool(), IacTool::OpenTofu);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "1.7.2");
    assert_eq!(runtimes[0].source(), &IacRuntimeSource::CandidatePath);
}

#[test]
fn direct_binary_path_is_canonicalized_to_parent_root() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");

    let discovered = validate_iac_tool_home(runtime.join("terraform"), IacTool::Terraform)
        .expect("direct binary should validate");

    assert_eq!(discovered.root(), runtime.as_path());
    assert_eq!(discovered.version().raw(), "1.8.5");
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("terraform").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("1.8.5").expect("version should be valid"),
            platform,
            "/registered/terraform-1.8.5",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("1.9.0").expect("version should be valid"),
            platform,
            "/owned/terraform-1.9.0",
        ))
        .expect("installation should be added");

    let runtimes = IacRuntimeDiscovery::new(IacTool::Terraform)
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/terraform-1.8.5")
            && runtime.source() == &IacRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/terraform-1.9.0")
            && runtime.source() == &IacRuntimeSource::Installed
    }));
}

#[test]
fn iac_installed_validators_do_not_require_sdk_env_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let terraform = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");
    let opentofu = create_fake_iac_runtime(temp.path(), "opentofu-1.7.2", "tofu", "1.7.2");

    devenv_core::InstalledRuntimeValidator::validate(
        &TerraformInstalledRuntimeValidator,
        &terraform,
    )
    .expect("terraform single binary should validate");
    devenv_core::InstalledRuntimeValidator::validate(&OpenTofuInstalledRuntimeValidator, &opentofu)
        .expect("opentofu single binary should validate");
}

#[test]
fn invalid_iac_runtime_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("not-terraform");
    fs::create_dir_all(&runtime).expect("directory should be created");

    let error =
        validate_iac_tool_home(&runtime, IacTool::Terraform).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Terraform runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("terraform"));
}

#[test]
fn iac_version_output_is_normalized() {
    assert_eq!(
        normalize_iac_version("Terraform v1.8.5").expect("version should normalize"),
        "1.8.5"
    );
}

#[test]
fn iac_version_matching_accepts_prefix_requirements() {
    let matcher = IacVersionMatcher;
    let candidates = vec![
        Version::new("1.7.5").expect("version should be valid"),
        Version::new("1.8.0").expect("version should be valid"),
        Version::new("1.8.5").expect("version should be valid"),
    ];

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("1.8").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "1.8.5");
}

#[test]
fn terraform_activation_exposes_single_binary_root() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");
    let adapter = TerraformToolAdapter::new();
    let plan = adapter
        .activation_plan(&runtime)
        .expect("activation should succeed");

    assert_eq!(adapter.exposed_binaries(), ["terraform"]);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path } if path == &runtime
    ));
}

#[test]
fn opentofu_activation_exposes_tofu_binary() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "opentofu-1.7.2", "tofu", "1.7.2");
    let adapter = OpenTofuToolAdapter::new();
    let plan = adapter
        .activation_plan(&runtime)
        .expect("activation should succeed");

    assert_eq!(adapter.exposed_binaries(), ["tofu"]);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path } if path == &runtime
    ));
}

fn create_fake_iac_runtime(parent: &Path, name: &str, binary_name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(&runtime).expect("runtime dir should be created");
    fs::write(runtime.join(binary_name), "").expect("binary should be written");
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}
