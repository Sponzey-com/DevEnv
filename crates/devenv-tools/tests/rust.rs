use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore, Installation,
    OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolName, Version,
    VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    RustRuntimeDiscovery, RustRuntimeSource, RustToolAdapter, RustVersionMatcher,
    normalize_rust_version, validate_rust_toolchain_home,
};

#[test]
fn fake_rust_toolchain_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain(temp.path(), "1.85.0-aarch64-apple-darwin", "rustc 1.85.0");
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = RustRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(&registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), toolchain.as_path());
    assert_eq!(runtimes[0].version().raw(), "1.85.0");
    assert_eq!(runtimes[0].source(), &RustRuntimeSource::CandidatePath);
}

#[test]
fn rustup_home_toolchains_are_discovered_without_running_rustup() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let rustup_home = temp.path().join("rustup-home");
    let toolchain = create_fake_rust_toolchain(
        &rustup_home.join("toolchains"),
        "1.85.0-aarch64-apple-darwin",
        "rustc 1.85.0",
    );
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = RustRuntimeDiscovery::new()
        .with_rustup_home(&rustup_home)
        .discover(&registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), toolchain.as_path());
    assert_eq!(runtimes[0].version().raw(), "1.85.0");
    assert_eq!(runtimes[0].source(), &RustRuntimeSource::Rustup);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("rust").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, devenv_core::Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("1.85.0").expect("version should be valid"),
            platform,
            "/registered/rust-1.85.0",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("1.86.0").expect("version should be valid"),
            platform,
            "/owned/rust-1.86.0",
        ))
        .expect("installation should be added");

    let runtimes = RustRuntimeDiscovery::new()
        .discover(&registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/rust-1.85.0")
            && runtime.source() == &RustRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/rust-1.86.0")
            && runtime.source() == &RustRuntimeSource::Installed
    }));
}

#[test]
fn invalid_rust_toolchain_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain = temp.path().join("not-rust");
    fs::create_dir_all(&toolchain).expect("directory should be created");

    let error = validate_rust_toolchain_home(&toolchain).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Rust toolchain"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/rustc"));
}

#[test]
fn unsupported_channel_style_rustup_state_is_actionable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain_without_version(temp.path(), "stable-aarch64-apple-darwin");

    let error = validate_rust_toolchain_home(&toolchain).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("unsupported rustup toolchain"));
    assert!(message.contains("channel-style"));
    assert!(message.contains("VERSION"));
}

#[test]
fn versioned_rustup_directory_name_can_supply_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain_without_version(temp.path(), "1.85.0-aarch64-apple-darwin");

    let runtime = validate_rust_toolchain_home(&toolchain).expect("toolchain should validate");

    assert_eq!(runtime.version().raw(), "1.85.0");
}

#[test]
fn rust_version_output_is_normalized() {
    assert_eq!(
        normalize_rust_version("rustc 1.85.0 (4d91de4e4 2025-02-17)")
            .expect("version should normalize"),
        "1.85.0"
    );
}

#[test]
fn rust_minor_requirement_matches_latest_patch_version() {
    let matcher = RustVersionMatcher;
    let requirement = VersionRequirement::exact("1.85").expect("requirement should be valid");
    let candidates = versions(["1.85.0", "1.85.1", "1.86.0"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "1.85.1");
}

#[test]
fn exact_rust_version_wins_over_prefix_match() {
    let matcher = RustVersionMatcher;
    let candidates = versions(["1.85", "1.85.1"]);

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("1.85").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "1.85");
}

#[test]
fn rust_activation_prepends_toolchain_bin() {
    let adapter = RustToolAdapter::new();
    let plan = adapter
        .activation_plan(Path::new("/opt/rust-1.85.0"))
        .expect("activation should be built");

    assert_eq!(plan.operations().len(), 1);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/rust-1.85.0/bin")
    ));
}

#[test]
fn rust_exposes_compiler_and_cargo_but_not_rustup_manager() {
    let adapter = RustToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["rustc", "cargo"]);
    assert!(
        !adapter
            .exposed_binaries()
            .iter()
            .any(|binary| binary == "rustup")
    );
}

fn create_fake_rust_toolchain(parent: &Path, name: &str, version: &str) -> PathBuf {
    let toolchain = create_fake_rust_toolchain_without_version(parent, name);
    fs::write(toolchain.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    toolchain
}

fn create_fake_rust_toolchain_without_version(parent: &Path, name: &str) -> PathBuf {
    let toolchain = parent.join(name);
    fs::create_dir_all(toolchain.join("bin")).expect("Rust bin should be created");
    fs::write(toolchain.join("bin/rustc"), "").expect("rustc should be written");
    fs::write(toolchain.join("bin/cargo"), "").expect("cargo should be written");
    toolchain
}

fn versions<const N: usize>(values: [&str; N]) -> Vec<Version> {
    values
        .into_iter()
        .map(|value| Version::new(value).expect("version should be valid"))
        .collect()
}
