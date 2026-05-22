use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    NodeInstalledRuntimeValidator, NodeRuntimeDiscovery, NodeRuntimeSource, NodeToolAdapter,
    NodeVersionMatcher, normalize_node_version, validate_node_home,
};

#[test]
fn fake_node_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_node_runtime(temp.path(), "node-v20.11.1", "v20.11.1");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = NodeRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "20.11.1");
    assert_eq!(runtimes[0].source(), &NodeRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("node").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("20.11.1").expect("version should be valid"),
            platform,
            "/registered/node-v20.11.1",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("21.2.0").expect("version should be valid"),
            platform,
            "/owned/node-v21.2.0",
        ))
        .expect("installation should be added");

    let runtimes = NodeRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/node-v20.11.1")
            && runtime.source() == &NodeRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/node-v21.2.0")
            && runtime.source() == &NodeRuntimeSource::Installed
    }));
}

#[test]
fn installed_node_archive_with_top_level_node_directory_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("node").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut install_store = InMemoryInstallStore::default();
    let registry = InMemoryRuntimeRegistry::default();
    let install_root = temp.path().join("installs/node/20.11.1/linux-x64");
    create_fake_node_runtime(&install_root, "node-v20.11.1", "v20.11.1");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("20.11.1").expect("version should be valid"),
            platform,
            &install_root,
        ))
        .expect("installation should be added");

    let runtimes = NodeRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == install_root.join("node-v20.11.1")
            && runtime.version().raw() == "20.11.1"
            && runtime.source() == &NodeRuntimeSource::Installed
    }));
}

#[test]
fn node_install_validator_accepts_top_level_node_archive_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    create_fake_node_runtime(temp.path(), "node-v20.11.1", "v20.11.1");

    devenv_core::InstalledRuntimeValidator::validate(&NodeInstalledRuntimeValidator, temp.path())
        .expect("nested Node.js archive layout should validate");
}

#[test]
fn node_version_header_metadata_is_supported() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("node-v20.11.1");
    fs::create_dir_all(runtime.join("bin")).expect("Node.js bin should be created");
    fs::create_dir_all(runtime.join("include/node")).expect("Node.js include should be created");
    for binary in ["node", "npm", "npx", "corepack"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(
        runtime.join("include/node/node_version.h"),
        "#define NODE_VERSION \"20.11.1\"\n",
    )
    .expect("header should be written");

    let discovered = validate_node_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.version().raw(), "20.11.1");
}

#[test]
fn invalid_node_runtime_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("not-node");
    fs::create_dir_all(&runtime).expect("directory should be created");

    let error = validate_node_home(&runtime).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Node.js runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/node"));
}

#[test]
fn node_v_prefix_is_removed_during_normalization() {
    assert_eq!(
        normalize_node_version("v20.11.1").expect("version should normalize"),
        "20.11.1"
    );
}

#[test]
fn node_major_requirement_matches_latest_patch_version() {
    let matcher = NodeVersionMatcher;
    let requirement = VersionRequirement::exact("20").expect("requirement should be valid");
    let candidates = versions(["20.10.0", "20.11.1", "21.2.0"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "20.11.1");
}

#[test]
fn exact_node_version_wins_over_prefix_match() {
    let matcher = NodeVersionMatcher;
    let candidates = versions(["20", "20.11.1"]);

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("20").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "20");
}

#[test]
fn node_activation_prepends_runtime_bin_without_package_manager_policy() {
    let adapter = NodeToolAdapter::new();
    let plan = adapter
        .activation_plan(Path::new("/opt/node-v20.11.1"))
        .expect("activation should be built");

    assert_eq!(plan.operations().len(), 1);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/node-v20.11.1/bin")
    ));
}

#[test]
fn node_exposes_runtime_and_standard_companion_binaries() {
    let adapter = NodeToolAdapter::new();

    assert_eq!(
        adapter.exposed_binaries(),
        ["node", "npm", "npx", "corepack"]
    );
}

fn create_fake_node_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Node.js bin should be created");
    for binary in ["node", "npm", "npx", "corepack"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn versions<const N: usize>(values: [&str; N]) -> Vec<Version> {
    values
        .into_iter()
        .map(|value| Version::new(value).expect("version should be valid"))
        .collect()
}
