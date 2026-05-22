use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    FlutterInstalledRuntimeValidator, FlutterRuntimeDiscovery, FlutterRuntimeSource,
    FlutterToolAdapter, FlutterVersionMatcher, normalize_flutter_version,
    validate_flutter_sdk_home,
};

#[test]
fn fake_flutter_sdk_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_flutter_sdk(temp.path(), "flutter-3.24.0", "Flutter 3.24.0");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = FlutterRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), sdk.as_path());
    assert_eq!(runtimes[0].version().raw(), "3.24.0");
    assert_eq!(runtimes[0].source(), &FlutterRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("flutter").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("3.24.0").expect("version should be valid"),
            platform,
            "/registered/flutter-3.24.0",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("3.27.1").expect("version should be valid"),
            platform,
            "/owned/flutter-3.27.1",
        ))
        .expect("installation should be added");

    let runtimes = FlutterRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/flutter-3.24.0")
            && runtime.source() == &FlutterRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/flutter-3.27.1")
            && runtime.source() == &FlutterRuntimeSource::Installed
    }));
}

#[test]
fn installed_flutter_archive_with_top_level_flutter_directory_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("flutter").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut install_store = InMemoryInstallStore::default();
    let registry = InMemoryRuntimeRegistry::default();
    let install_root = temp.path().join("installs/flutter/3.24.0/linux-x64");
    create_fake_flutter_sdk(&install_root, "flutter", "3.24.0");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("3.24.0").expect("version should be valid"),
            platform,
            &install_root,
        ))
        .expect("installation should be added");

    let runtimes = FlutterRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == install_root.join("flutter")
            && runtime.version().raw() == "3.24.0"
            && runtime.source() == &FlutterRuntimeSource::Installed
    }));
}

#[test]
fn flutter_install_validator_accepts_top_level_flutter_archive_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    create_fake_flutter_sdk(temp.path(), "flutter", "3.24.0");

    devenv_core::InstalledRuntimeValidator::validate(
        &FlutterInstalledRuntimeValidator,
        temp.path(),
    )
    .expect("nested Flutter archive layout should validate");
}

#[test]
fn invalid_flutter_sdk_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = temp.path().join("not-flutter");
    fs::create_dir_all(&sdk).expect("directory should be created");

    let error = validate_flutter_sdk_home(&sdk).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Flutter SDK"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/flutter"));
}

#[test]
fn flutter_version_output_is_normalized() {
    assert_eq!(
        normalize_flutter_version("Flutter 3.24.0 stable").expect("version should normalize"),
        "3.24.0"
    );
}

#[test]
fn flutter_version_matching_accepts_prefix_requirements() {
    let matcher = FlutterVersionMatcher;
    let candidates = vec![
        Version::new("3.22.3").expect("version should be valid"),
        Version::new("3.24.0").expect("version should be valid"),
        Version::new("3.24.2").expect("version should be valid"),
    ];

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("3.24").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "3.24.2");
}

#[test]
fn flutter_activation_exposes_flutter_and_dart() {
    let adapter = FlutterToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/flutter-3.24.0").as_path())
        .expect("activation should succeed");

    assert_eq!(adapter.exposed_binaries(), ["flutter", "dart"]);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "FLUTTER_ROOT" && value == "/opt/flutter-3.24.0"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/flutter-3.24.0/bin")
    ));
}

fn create_fake_flutter_sdk(parent: &Path, name: &str, version: &str) -> PathBuf {
    let sdk = parent.join(name);
    fs::create_dir_all(sdk.join("bin")).expect("Flutter bin should be created");
    fs::write(sdk.join("bin/flutter"), "").expect("flutter binary should be written");
    fs::write(sdk.join("bin/dart"), "").expect("dart binary should be written");
    fs::write(sdk.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    sdk
}
