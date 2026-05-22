use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore, Installation,
    OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolDistribution,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    JavaRuntimeDiscovery, JavaRuntimeSource, JavaToolAdapter, JavaVersionMatcher,
    normalize_java_version, validate_jdk_home,
};

#[test]
fn fake_jdk_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17", "17.0.11", None);
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = JavaRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), jdk.as_path());
    assert_eq!(runtimes[0].version().raw(), "17.0.11");
    assert_eq!(runtimes[0].source(), &JavaRuntimeSource::CandidatePath);
    assert_eq!(runtimes[0].distribution(), &ToolDistribution::Unknown);
}

#[test]
fn macos_bundle_home_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = create_fake_jdk(
        &temp.path().join("Temurin-17.jdk/Contents"),
        "Home",
        "17.0.11",
        Some("Eclipse Adoptium"),
    );
    let platform = Platform::new(OperatingSystem::Macos, Architecture::Arm64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = JavaRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes[0].root(), home.as_path());
    assert_eq!(
        runtimes[0].distribution(),
        &ToolDistribution::named("Eclipse Adoptium")
    );
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("java").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("17.0.11").expect("version should be valid"),
            platform,
            "/registered/jdk-17",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("21.0.2").expect("version should be valid"),
            platform,
            "/owned/jdk-21",
        ))
        .expect("installation should be added");

    let runtimes = JavaRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/jdk-17")
            && runtime.source() == &JavaRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/jdk-21")
            && runtime.source() == &JavaRuntimeSource::Installed
    }));
}

#[test]
fn invalid_jdk_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = temp.path().join("not-a-jdk");
    fs::create_dir_all(&jdk).expect("directory should be created");

    let error = validate_jdk_home(&jdk).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Java runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/java"));
}

#[test]
fn java_17_requirement_matches_full_java_version() {
    let matcher = JavaVersionMatcher;
    let requirement = VersionRequirement::exact("17").expect("requirement should be valid");
    let candidates = versions(["17.0.1", "17.0.11", "21.0.2"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "17.0.11");
}

#[test]
fn java_8_and_legacy_1_8_policy_match_the_same_runtime_family() {
    assert_eq!(
        normalize_java_version("1.8.0_402").expect("version should normalize"),
        "8.0.402"
    );

    let matcher = JavaVersionMatcher;
    let candidates = versions(["1.8.0_402", "11.0.22"]);

    for requirement in ["8", "1.8"] {
        let matched = matcher
            .match_version(
                &VersionRequirement::exact(requirement).expect("requirement should be valid"),
                &candidates,
            )
            .expect("matching should succeed")
            .expect("version should match");

        assert_eq!(matched.raw(), "1.8.0_402");
    }
}

#[test]
fn java_build_metadata_and_suffixes_do_not_change_feature_version_matching() {
    assert_eq!(
        normalize_java_version("17.0.11+9-LTS").expect("version should normalize"),
        "17.0.11"
    );
    assert_eq!(
        normalize_java_version("jdk-17.0.11").expect("version should normalize"),
        "17.0.11"
    );
    assert_eq!(
        normalize_java_version("17-ea").expect("version should normalize"),
        "17"
    );

    let matcher = JavaVersionMatcher;
    let candidates = versions(["17.0.11+9-LTS", "21.0.2+13"]);
    let matched = matcher
        .match_version(
            &VersionRequirement::exact("17.0.11").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "17.0.11+9-LTS");
}

#[test]
fn exact_java_version_wins_over_major_only_match() {
    let matcher = JavaVersionMatcher;
    let candidates = versions(["17", "17.0.11"]);

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("17").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "17");
}

#[test]
fn java_activation_sets_java_home_and_prepends_jdk_bin() {
    let adapter = JavaToolAdapter::new();
    let plan = adapter
        .activation_plan(Path::new("/opt/jdk-17"))
        .expect("activation should be built");

    assert_eq!(plan.operations().len(), 2);
}

fn create_fake_jdk(parent: &Path, name: &str, version: &str, implementor: Option<&str>) -> PathBuf {
    let jdk = parent.join(name);
    fs::create_dir_all(jdk.join("bin")).expect("JDK bin should be created");
    fs::write(jdk.join("bin/java"), "").expect("java binary should be written");
    fs::write(jdk.join("bin/javac"), "").expect("javac binary should be written");
    let mut release = format!("JAVA_VERSION=\"{version}\"\n");
    if let Some(implementor) = implementor {
        release.push_str(&format!("IMPLEMENTOR=\"{implementor}\"\n"));
    }
    fs::write(jdk.join("release"), release).expect("release metadata should be written");
    jdk
}

fn versions<const N: usize>(values: [&str; N]) -> Vec<Version> {
    values
        .into_iter()
        .map(|value| Version::new(value).expect("version should be valid"))
        .collect()
}
