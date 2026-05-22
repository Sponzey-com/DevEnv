use std::collections::BTreeMap;
use std::fs;

use devenv_adapters::store::{DevEnvHome, FileInstallStore, FileRuntimeRegistry};
use devenv_core::{
    Architecture, InstallStore, Installation, InstallationMetadata, OperatingSystem, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolName, Version,
};

#[test]
fn devenv_home_override_is_used() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let environment = BTreeMap::from([(
        "DEVENV_HOME".to_owned(),
        temp.path()
            .join("custom-home")
            .to_string_lossy()
            .into_owned(),
    )]);

    let home = DevEnvHome::resolve_from_env(&environment).expect("home should resolve");

    assert_eq!(home.root(), temp.path().join("custom-home"));
}

#[test]
fn devenv_home_uses_platform_data_directory_when_override_is_absent() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let environment = BTreeMap::from([
        (
            "HOME".to_owned(),
            temp.path().join("home").to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_owned(),
            temp.path().join("xdg-data").to_string_lossy().into_owned(),
        ),
        (
            "LOCALAPPDATA".to_owned(),
            temp.path()
                .join("local-app-data")
                .to_string_lossy()
                .into_owned(),
        ),
    ]);

    let home = DevEnvHome::resolve_from_env(&environment).expect("home should resolve");

    if cfg!(target_os = "windows") {
        assert_eq!(home.root(), temp.path().join("local-app-data/devenv"));
    } else if cfg!(target_os = "macos") {
        assert_eq!(
            home.root(),
            temp.path().join("home/Library/Application Support/devenv")
        );
    } else {
        assert_eq!(home.root(), temp.path().join("xdg-data/devenv"));
    }
}

#[test]
fn store_layout_creates_expected_directories() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));

    home.create_layout().expect("layout should be created");

    for directory in [
        "installs",
        "registry",
        "downloads",
        "shims",
        "state",
        "logs",
    ] {
        assert!(
            home.root().join(directory).is_dir(),
            "{directory} should exist"
        );
    }
}

#[test]
fn file_runtime_registry_adds_and_lists_external_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let runtime = fake_registered_runtime(temp.path().join("jdk-17"));

    registry
        .add_registered_runtime(runtime.clone())
        .expect("runtime should be registered");

    let java = ToolName::new("java").expect("tool should be valid");
    assert_eq!(registry.list_registered_runtimes(&java), vec![runtime]);

    let contents =
        fs::read_to_string(home.external_registry_file()).expect("registry should be readable");
    assert!(contents.contains("[[runtime]]"));
    assert!(contents.contains("tool = \"java\""));
    assert!(contents.contains("version = \"17\""));
    assert!(contents.contains("platform = \"linux-x64\""));
}

#[test]
fn removing_external_runtime_only_updates_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let external_runtime = temp.path().join("external-jdk");
    fs::create_dir_all(&external_runtime).expect("external runtime should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let runtime = fake_registered_runtime(&external_runtime);
    let tool = runtime.tool().clone();
    let version = runtime.version().clone();
    let platform = runtime.platform();

    registry
        .add_registered_runtime(runtime.clone())
        .expect("runtime should be registered");
    let removed = registry
        .remove_registered_runtime(&tool, &version, platform, Some(&external_runtime))
        .expect("runtime should be removed");

    assert_eq!(removed, vec![runtime]);
    assert!(registry.list_registered_runtimes(&tool).is_empty());
    assert!(external_runtime.exists());
}

#[test]
fn file_install_store_writes_owned_install_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut store = FileInstallStore::at_home(&home);
    let tool = ToolName::new("go").expect("tool should be valid");
    let version = Version::new("1.22.5").expect("version should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let root = store.install_root(&tool, &version, platform);
    let installation = Installation::new(tool.clone(), version.clone(), platform, &root);
    let metadata = InstallationMetadata::new(
        installation.clone(),
        "https://go.dev/dl/go1.22.5.tar.gz",
        Some("sha256:abc123".to_owned()),
        "2026-05-21T00:00:00Z",
    );

    store
        .add_installation_metadata(metadata.clone())
        .expect("install metadata should be written");

    assert_eq!(store.list_installations(&tool), vec![installation]);
    assert_eq!(
        store
            .read_installation_metadata(&root)
            .expect("metadata should be readable"),
        metadata
    );

    let contents = fs::read_to_string(root.join("devenv-install.toml"))
        .expect("metadata file should be readable");
    assert!(contents.contains("source = \"https://go.dev/dl/go1.22.5.tar.gz\""));
    assert!(contents.contains("checksum = \"sha256:abc123\""));
    assert!(contents.contains("installed_at = \"2026-05-21T00:00:00Z\""));
}

#[test]
fn file_install_store_rejects_deleting_outside_owned_installs() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut store = FileInstallStore::at_home(&home);
    let external_runtime = temp.path().join("external-jdk");
    fs::create_dir_all(&external_runtime).expect("external runtime should be created");

    let error = store
        .remove_installation_root(&external_runtime)
        .expect_err("external runtime deletion should be rejected");

    assert!(error.to_string().contains("refusing to modify"));
    assert!(external_runtime.exists());
}

#[test]
fn file_install_store_removes_owned_install_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut store = FileInstallStore::at_home(&home);
    let tool = ToolName::new("go").expect("tool should be valid");
    let version = Version::new("1.22.5").expect("version should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let root = store.install_root(&tool, &version, platform);
    let installation = Installation::new(tool.clone(), version.clone(), platform, &root);
    store
        .add_installation_metadata(InstallationMetadata::new(
            installation.clone(),
            "file:///go.tar.gz",
            Some("sha256:abc123".to_owned()),
            "2026-05-21T00:00:00Z",
        ))
        .expect("install metadata should be written");
    fs::write(root.join("runtime.txt"), "owned").expect("runtime file should be written");

    let removed = store
        .remove_installation_metadata(&tool, &version, platform)
        .expect("owned install should be removed");

    assert_eq!(
        removed.expect("metadata should be returned").installation(),
        &installation
    );
    assert!(!root.exists());
    assert!(store.list_installations(&tool).is_empty());
}

#[test]
fn file_install_store_rejects_path_traversal_delete_from_installs_dir() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    let mut store = FileInstallStore::at_home(&home);
    fs::create_dir_all(home.installs_dir()).expect("installs directory should be created");
    let external_runtime = home.root().join("external-runtime");
    fs::create_dir_all(&external_runtime).expect("external runtime should be created");
    let traversal = home.installs_dir().join("../external-runtime");

    let error = store
        .remove_installation_root(&traversal)
        .expect_err("path traversal deletion should be rejected");

    assert!(error.to_string().contains("refusing to modify"));
    assert!(external_runtime.exists());
}

fn fake_registered_runtime(root: impl Into<std::path::PathBuf>) -> RegisteredRuntime {
    RegisteredRuntime::new(
        ToolName::new("java").expect("tool should be valid"),
        Version::new("17").expect("version should be valid"),
        Platform::new(OperatingSystem::Linux, Architecture::X64),
        root,
    )
}
