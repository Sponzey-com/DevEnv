use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    RubyRuntimeDiscovery, RubyRuntimeSource, RubyToolAdapter, RubyVersionMatcher,
    normalize_ruby_version, validate_ruby_home,
};

#[test]
fn fake_ruby_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_ruby_runtime(temp.path(), "ruby-3.3.0", "ruby 3.3.0");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = RubyRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "3.3.0");
    assert_eq!(runtimes[0].source(), &RubyRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("ruby").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("3.3.0").expect("version should be valid"),
            platform,
            "/registered/ruby-3.3.0",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("3.4.0").expect("version should be valid"),
            platform,
            "/owned/ruby-3.4.0",
        ))
        .expect("installation should be added");

    let runtimes = RubyRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/ruby-3.3.0")
            && runtime.source() == &RubyRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/ruby-3.4.0")
            && runtime.source() == &RubyRuntimeSource::Installed
    }));
}

#[test]
fn ruby_version_header_metadata_is_supported() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("ruby-3.3.0");
    fs::create_dir_all(runtime.join("bin")).expect("Ruby bin should be created");
    fs::create_dir_all(runtime.join("include/ruby-3.3.0/ruby"))
        .expect("Ruby include should be created");
    for binary in ["ruby", "gem", "bundle"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(
        runtime.join("include/ruby-3.3.0/ruby/version.h"),
        "#define RUBY_VERSION \"3.3.0\"\n",
    )
    .expect("header should be written");

    let discovered = validate_ruby_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.version().raw(), "3.3.0");
}

#[test]
fn invalid_ruby_runtime_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("not-ruby");
    fs::create_dir_all(&runtime).expect("directory should be created");

    let error = validate_ruby_home(&runtime).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Ruby runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/ruby"));
}

#[test]
fn ruby_version_output_is_normalized() {
    assert_eq!(
        normalize_ruby_version("ruby 3.3.0p0").expect("version should normalize"),
        "3.3.0p0"
    );
    assert_eq!(
        normalize_ruby_version("ruby-3.3.0").expect("version should normalize"),
        "3.3.0"
    );
}

#[test]
fn ruby_minor_requirement_matches_latest_patch_version() {
    let matcher = RubyVersionMatcher;
    let requirement = VersionRequirement::exact("3.3").expect("requirement should be valid");
    let candidates = versions(["3.2.4", "3.3.0", "3.3.2"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "3.3.2");
}

#[test]
fn ruby_activation_exposes_expected_binaries() {
    let adapter = RubyToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/ruby-3.3.0").as_path())
        .expect("activation should succeed");

    assert_eq!(adapter.exposed_binaries(), ["ruby", "gem", "bundle"]);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/ruby-3.3.0/bin")
    ));
}

fn create_fake_ruby_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Ruby bin should be created");
    for binary in ["ruby", "gem", "bundle"] {
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
