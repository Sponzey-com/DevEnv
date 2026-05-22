use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    PhpRuntimeDiscovery, PhpRuntimeSource, PhpToolAdapter, PhpVersionMatcher,
    normalize_php_version, validate_php_home,
};

#[test]
fn fake_php_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_php_runtime(temp.path(), "php-8.3.7", "PHP 8.3.7");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = PhpRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "8.3.7");
    assert_eq!(runtimes[0].source(), &PhpRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("php").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("8.3.7").expect("version should be valid"),
            platform,
            "/registered/php-8.3.7",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("8.4.0").expect("version should be valid"),
            platform,
            "/owned/php-8.4.0",
        ))
        .expect("installation should be added");

    let runtimes = PhpRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/php-8.3.7")
            && runtime.source() == &PhpRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/php-8.4.0")
            && runtime.source() == &PhpRuntimeSource::Installed
    }));
}

#[test]
fn php_version_header_metadata_is_supported() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("php-8.3.7");
    fs::create_dir_all(runtime.join("bin")).expect("PHP bin should be created");
    fs::create_dir_all(runtime.join("include/main")).expect("PHP include should be created");
    for binary in ["php", "phpize", "php-config"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(
        runtime.join("include/main/php_version.h"),
        "#define PHP_VERSION \"8.3.7\"\n",
    )
    .expect("header should be written");

    let discovered = validate_php_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.version().raw(), "8.3.7");
}

#[test]
fn invalid_php_runtime_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("not-php");
    fs::create_dir_all(&runtime).expect("directory should be created");

    let error = validate_php_home(&runtime).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid PHP runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/php"));
}

#[test]
fn php_version_output_is_normalized() {
    assert_eq!(
        normalize_php_version("PHP 8.3.7").expect("version should normalize"),
        "8.3.7"
    );
}

#[test]
fn php_minor_requirement_matches_latest_patch_version() {
    let matcher = PhpVersionMatcher;
    let requirement = VersionRequirement::exact("8.3").expect("requirement should be valid");
    let candidates = versions(["8.2.12", "8.3.1", "8.3.7"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "8.3.7");
}

#[test]
fn php_activation_exposes_expected_binaries() {
    let adapter = PhpToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/php-8.3.7").as_path())
        .expect("activation should succeed");

    assert_eq!(adapter.exposed_binaries(), ["php", "phpize", "php-config"]);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/php-8.3.7/bin")
    ));
}

fn create_fake_php_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("PHP bin should be created");
    for binary in ["php", "phpize", "php-config"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn versions<const N: usize>(versions: [&str; N]) -> Vec<Version> {
    versions
        .into_iter()
        .map(|version| Version::new(version).expect("version should be valid"))
        .collect()
}
