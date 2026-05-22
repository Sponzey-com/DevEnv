use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, EnvOperation, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolAdapter,
    ToolName, Version, VersionMatcher, VersionRequirement,
};
use devenv_tools::{
    PythonImplementation, PythonInstalledRuntimeValidator, PythonRuntimeDiscovery,
    PythonRuntimeSource, PythonToolAdapter, PythonVersionMatcher, normalize_python_version,
    validate_python_home,
};

#[test]
fn fake_python_layout_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime =
        create_fake_python_runtime(temp.path(), "cpython-3.12.2", "Python 3.12.2", "cpython");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let registry = InMemoryRuntimeRegistry::default();
    let install_store = InMemoryInstallStore::default();

    let runtimes = PythonRuntimeDiscovery::new()
        .with_candidate_root(temp.path())
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].root(), runtime.as_path());
    assert_eq!(runtimes[0].version().raw(), "3.12.2");
    assert_eq!(runtimes[0].implementation(), PythonImplementation::Cpython);
    assert_eq!(runtimes[0].source(), &PythonRuntimeSource::CandidatePath);
}

#[test]
fn registry_and_install_store_are_discovery_sources() {
    let tool = ToolName::new("python").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut registry = InMemoryRuntimeRegistry::default();
    let mut install_store = InMemoryInstallStore::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("3.12.2").expect("version should be valid"),
            platform,
            "/registered/cpython-3.12.2",
        ))
        .expect("runtime should be registered");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("3.13.0").expect("version should be valid"),
            platform,
            "/owned/cpython-3.13.0",
        ))
        .expect("installation should be added");

    let runtimes = PythonRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/registered/cpython-3.12.2")
            && runtime.source() == &PythonRuntimeSource::Registered
    }));
    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == Path::new("/owned/cpython-3.13.0")
            && runtime.source() == &PythonRuntimeSource::Installed
    }));
}

#[test]
fn installed_python_archive_with_top_level_directory_is_discovered() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("python").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut install_store = InMemoryInstallStore::default();
    let registry = InMemoryRuntimeRegistry::default();
    let install_root = temp.path().join("installs/python/3.12.2/linux-x64");
    create_fake_python_runtime(&install_root, "cpython-3.12.2", "Python 3.12.2", "cpython");
    install_store
        .add_installation(Installation::new(
            tool,
            Version::new("3.12.2").expect("version should be valid"),
            platform,
            &install_root,
        ))
        .expect("installation should be added");

    let runtimes = PythonRuntimeDiscovery::new()
        .discover(platform, &registry, &install_store)
        .expect("discovery should succeed");

    assert!(runtimes.iter().any(|runtime| {
        runtime.root() == install_root.join("cpython-3.12.2")
            && runtime.version().raw() == "3.12.2"
            && runtime.implementation() == PythonImplementation::Cpython
            && runtime.source() == &PythonRuntimeSource::Installed
    }));
}

#[test]
fn python_install_validator_accepts_top_level_python_archive_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    create_fake_python_runtime(temp.path(), "cpython-3.12.2", "Python 3.12.2", "cpython");

    devenv_core::InstalledRuntimeValidator::validate(&PythonInstalledRuntimeValidator, temp.path())
        .expect("nested Python archive layout should validate");
}

#[test]
fn python_patchlevel_header_metadata_is_supported() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("cpython-3.12.2");
    fs::create_dir_all(runtime.join("bin")).expect("Python bin should be created");
    fs::create_dir_all(runtime.join("include/python3.12"))
        .expect("Python include should be created");
    for binary in ["python", "python3", "pip"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(
        runtime.join("include/python3.12/patchlevel.h"),
        "#define PY_VERSION \"3.12.2\"\n",
    )
    .expect("header should be written");

    let discovered = validate_python_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.version().raw(), "3.12.2");
    assert_eq!(discovered.implementation(), PythonImplementation::Cpython);
}

#[test]
fn pypy_runtime_metadata_is_represented() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_python_runtime(temp.path(), "pypy3.10-7.3.15", "3.10.14", "pypy");

    let discovered = validate_python_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.version().raw(), "3.10.14");
    assert_eq!(discovered.implementation(), PythonImplementation::Pypy);
}

#[test]
fn unknown_python_implementation_is_represented() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("custom-runtime");
    fs::create_dir_all(runtime.join("bin")).expect("Python bin should be created");
    for binary in ["python", "python3", "pip"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), "3.12.2\n").expect("version should be written");

    let discovered = validate_python_home(&runtime).expect("runtime should validate");

    assert_eq!(discovered.implementation(), PythonImplementation::Unknown);
}

#[test]
fn invalid_python_runtime_path_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("not-python");
    fs::create_dir_all(&runtime).expect("directory should be created");

    let error = validate_python_home(&runtime).expect_err("validation should fail");
    let message = error.to_string();

    assert!(message.contains("invalid Python runtime"));
    assert!(message.contains("missing"));
    assert!(message.contains("bin/python"));
}

#[test]
fn virtual_environment_path_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_python_runtime(temp.path(), "venv", "3.12.2", "cpython");
    fs::write(runtime.join("pyvenv.cfg"), "home = /opt/cpython-3.12.2\n")
        .expect("venv marker should be written");

    let error = validate_python_home(&runtime).expect_err("validation should fail");

    assert!(
        error
            .to_string()
            .contains("virtual environments are not supported")
    );
}

#[test]
fn python_prefixes_are_removed_during_normalization() {
    assert_eq!(
        normalize_python_version("Python 3.12.2").expect("version should normalize"),
        "3.12.2"
    );
    assert_eq!(
        normalize_python_version("cpython-3.12.2+build").expect("version should normalize"),
        "3.12.2"
    );
}

#[test]
fn python_minor_requirement_matches_latest_patch_version() {
    let matcher = PythonVersionMatcher;
    let requirement = VersionRequirement::exact("3.12").expect("requirement should be valid");
    let candidates = versions(["3.12.1", "3.12.2", "3.13.0"]);

    let matched = matcher
        .match_version(&requirement, &candidates)
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "3.12.2");
}

#[test]
fn exact_python_version_wins_over_prefix_match() {
    let matcher = PythonVersionMatcher;
    let candidates = versions(["3.12", "3.12.2"]);

    let matched = matcher
        .match_version(
            &VersionRequirement::exact("3.12").expect("requirement should be valid"),
            &candidates,
        )
        .expect("matching should succeed")
        .expect("version should match");

    assert_eq!(matched.raw(), "3.12");
}

#[test]
fn python_activation_prepends_runtime_bin_without_virtualenv_policy() {
    let adapter = PythonToolAdapter::new();
    let plan = adapter
        .activation_plan(Path::new("/opt/cpython-3.12.2"))
        .expect("activation should be built");

    assert_eq!(plan.operations().len(), 1);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/cpython-3.12.2/bin")
    ));
}

#[test]
fn python_exposes_runtime_and_safe_companion_binaries() {
    let adapter = PythonToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["python", "python3", "pip"]);
}

fn create_fake_python_runtime(
    parent: &Path,
    name: &str,
    version: &str,
    implementation: &str,
) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Python bin should be created");
    for binary in ["python", "python3", "pip"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    fs::write(
        runtime.join("IMPLEMENTATION"),
        format!("{implementation}\n"),
    )
    .expect("implementation metadata should be written");
    runtime
}

fn versions<const N: usize>(values: [&str; N]) -> Vec<Version> {
    values
        .into_iter()
        .map(|value| Version::new(value).expect("version should be valid"))
        .collect()
}
