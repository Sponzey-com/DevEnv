use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement, activation_plan_for_selected_runtime,
};
use devenv_tools::{
    GoInstalledRuntimeValidator, GoRuntimeDiscovery, GoRuntimeSource, GoToolAdapter,
    GoVersionMatcher, normalize_go_version, validate_go_sdk_home,
};

#[test]
fn fake_go_sdk_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = GoRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), sdk.as_path());
    assert_eq!(runtimes[0].version().raw(), "1.22.5");
    assert_eq!(runtimes[0].source(), &GoRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("1.22.5").expect("version should be valid"),
            platform,
            "/registered/go-1.22.5",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("1.23.0").expect("version should be valid"),
            platform,
            "/owned/go-1.23.0",
        ))
        .expect("installation should be added");

    let runtimes = GoRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/go-1.22.5")
            && runtime.source() == &GoRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/go-1.23.0")
            && runtime.source() == &GoRuntimeSource::Installed
    }));
}

#[test]
fn installed_go_archive_with_top_level_go_directory_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("go").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut install_store = InMemoryInstallStore::default();
    let registry = InMemoryRuntimeRegistry::default();
    let install_root = temp.path().join("installs/go/1.22.5/linux-x64");
    create_fake_go_sdk(&install_root, "go", "go1.22.5");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("1.22.5").expect("version should be valid"),
            platform,
            &install_root,
        ))
        .expect("installation should be added");

    let runtimes = GoRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == install_root.join("go")
            && runtime.version().raw() == "1.22.5"
            && runtime.source() == &GoRuntimeSource::Installed
    }));
}

#[test]
fn go_install_validator_accepts_top_level_go_archive_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    create_fake_go_sdk(temp.path(), "go", "go1.22.5");

    devenv_core::InstalledRuntimeValidator::validate(&GoInstalledRuntimeValidator, temp.path())
        .expect("nested Go archive layout should validate");
}

#[test]
fn invalid_go_sdk_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = temp.path().join("not-go");
    fs::create_dir_all(&sdk).expect("directory should be created");

    let error = validate_go_sdk_home(&sdk).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Go runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/go"));
}

#[test]
fn go_release_prefix_is_removed_during_normalization() {
    assert_eq!(
        normalize_go_version("go1.22.5").expect("version should normalize"),
        "1.22.5"
    );
}

#[test]
fn go_minor_requirement_matches_latest_patch_version() {
    let matcher = GoVersionMatcher;
    let requirement = VersionRequirement::exact("1.22").expect("requirement should be valid");
    let candidates = versions(["1.22.1", "1.22.5", "1.23.0"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "1.22.5");
}

#[test]
fn exact_go_version_wins_over_prefix_match() {
    let matcher = GoVersionMatcher;
    let candidates = versions(["1.22", "1.22.5"]);

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("1.22").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "1.22");
}

#[test]
fn go_activation_sets_goroot_and_prepends_sdk_bin() {
    let adapter = GoToolAdapter::new();
    let plan = adapter
        .activation_plan(Path::new("/opt/go-1.22.5"))
        .expect("activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "GOROOT" && value == "/opt/go-1.22.5"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/go-1.22.5/bin")
    ));
}

#[test]
fn java_and_go_share_core_selected_runtime_activation_use_case() {
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let java = ToolName::new("java").expect("tool should be valid");
    let go = ToolName::new("go").expect("tool should be valid");
    let install_store = InMemoryInstallStore::default();
    let mut registry = InMemoryRuntimeRegistry::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            java.clone(),
            Version::new("17").expect("version should be valid"),
            platform,
            "/opt/jdk-17",
        ))
        .expect("java should be registered");
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            go.clone(),
            Version::new("1.22.5").expect("version should be valid"),
            platform,
            "/opt/go-1.22.5",
        ))
        .expect("go should be registered");

    let java_plan = activation_plan_for_selected_runtime(
        &java,
        &VersionRequirement::exact("17").expect("requirement should be valid"),
        platform,
        &install_store,
        &registry,
        &devenv_tools::JavaToolAdapter::new(),
    )
    .expect("java activation should resolve");
    let go_plan = activation_plan_for_selected_runtime(
        &go,
        &VersionRequirement::exact("1.22.5").expect("requirement should be valid"),
        platform,
        &install_store,
        &registry,
        &GoToolAdapter::new(),
    )
    .expect("go activation should resolve");

    assert_eq!(java_plan.operations().len(), 2);
    assert_eq!(go_plan.operations().len(), 2);
}

fn create_fake_go_sdk(parent: &Path, name: &str, version: &str) -> PathBuf {
    let sdk = parent.join(name);
    fs::create_dir_all(sdk.join("bin")).expect("Go SDK bin should be created");
    fs::write(sdk.join("bin/go"), "").expect("go binary should be written");
    fs::write(sdk.join("bin/gofmt"), "").expect("gofmt binary should be written");
    fs::write(sdk.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    sdk
}

fn versions<const N: usize>(values: [&str; N]) -> Vec<Version> {
    values
        .into_iter()
        .map(|value| Version::new(value).expect("version should be valid"))
        .collect()
}
