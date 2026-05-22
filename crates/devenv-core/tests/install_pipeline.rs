use std::path::{Path, PathBuf};

use devenv_core::{
    Architecture, ArchiveType, Artifact, FakeArchiveExtractor, FakeChecksumVerifier,
    FakeDownloader, FakeInstallTransactionManager, InMemoryInstallStore, InMemoryLockManager,
    InstallRuntimePorts, InstallRuntimeRequest, InstallStore, LockManager, OperatingSystem,
    Platform, StaticArtifactResolver, StaticClock, StaticVersionSource, ToolName, Version,
    install_lock_key, install_runtime, list_remote_versions,
};

#[test]
fn list_remote_displays_fake_remote_versions() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let mut source = StaticVersionSource::default();
    source
        .add_versions(tool.clone(), ["1.22.5", "1.23.0"])
        .expect("versions should be added");

    let versions = list_remote_versions(&tool, &source).expect("versions should list");
    let output = versions
        .iter()
        .map(|version| format!("{tool} {version}"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(output.contains("go 1.22.5"));
    assert!(output.contains("go 1.23.0"));
}

#[test]
fn install_pipeline_succeeds_with_fake_artifact_and_fake_extractor() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = linux_x64();
    let artifact = Artifact::new(
        "https://example.invalid/fake.tar.gz",
        "fake.tar.gz",
        ArchiveType::TarGz,
        Some("sha256:fixture".to_owned()),
    )
    .with_size(7);
    let resolver = StaticArtifactResolver::with_artifact(artifact);
    let mut downloader = FakeDownloader::new("payload");
    let checksum = FakeChecksumVerifier::passing();
    let mut extractor = FakeArchiveExtractor::with_entries(["bin/fake"]);
    let mut transactions = FakeInstallTransactionManager::new(temp.path());
    let mut store = InMemoryInstallStore::default();
    let mut locks = InMemoryLockManager::default();
    let clock = StaticClock::new("2026-05-21T00:00:00Z");

    let metadata = install_runtime(
        InstallRuntimeRequest::new(tool.clone(), version.clone(), platform),
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut store,
            lock_manager: &mut locks,
            clock: &clock,
            installed_runtime_validator: None,
        },
    )
    .expect("install should succeed");

    assert_eq!(metadata.installation().tool(), &tool);
    assert_eq!(metadata.installation().version(), &version);
    assert_eq!(metadata.installation().platform(), platform);
    assert_eq!(metadata.source(), "https://example.invalid/fake.tar.gz");
    assert_eq!(metadata.checksum(), Some("sha256:fixture"));
    assert_eq!(metadata.installed_at(), "2026-05-21T00:00:00Z");
    assert!(metadata.installation().root().exists());
    assert_eq!(store.installation_metadata(), &[metadata]);
    assert_eq!(transactions.committed().len(), 1);
    assert_eq!(transactions.cleaned().len(), 1);
}

#[test]
fn checksum_mismatch_aborts_install_and_cleans_temp() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = linux_x64();
    let resolver = StaticArtifactResolver::with_artifact(Artifact::new(
        "https://example.invalid/fake.tar.gz",
        "fake.tar.gz",
        ArchiveType::TarGz,
        Some("sha256:expected".to_owned()),
    ));
    let mut downloader = FakeDownloader::new("payload");
    let checksum = FakeChecksumVerifier::failing("fixture mismatch");
    let mut extractor = FakeArchiveExtractor::with_entries(["bin/fake"]);
    let mut transactions = FakeInstallTransactionManager::new(temp.path());
    let mut store = InMemoryInstallStore::default();
    let mut locks = InMemoryLockManager::default();
    let clock = StaticClock::new("2026-05-21T00:00:00Z");

    let error = install_runtime(
        InstallRuntimeRequest::new(tool.clone(), version.clone(), platform),
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut store,
            lock_manager: &mut locks,
            clock: &clock,
            installed_runtime_validator: None,
        },
    )
    .expect_err("install should fail");

    assert!(error.to_string().contains("checksum mismatch"));
    assert!(store.list_installations(&tool).is_empty());
    assert!(transactions.committed().is_empty());
    assert_temp_cleaned(&transactions);
    assert!(!install_root(temp.path(), &tool, &version, platform).exists());
}

#[test]
fn failed_extraction_does_not_register_install() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = linux_x64();
    let resolver =
        StaticArtifactResolver::new("https://example.invalid/fake.tar.gz", "fake.tar.gz");
    let mut downloader = FakeDownloader::new("payload");
    let checksum = FakeChecksumVerifier::passing();
    let mut extractor = FakeArchiveExtractor::failing("extract failed");
    let mut transactions = FakeInstallTransactionManager::new(temp.path());
    let mut store = InMemoryInstallStore::default();
    let mut locks = InMemoryLockManager::default();
    let clock = StaticClock::new("2026-05-21T00:00:00Z");

    let error = install_runtime(
        InstallRuntimeRequest::new(tool.clone(), version.clone(), platform),
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut store,
            lock_manager: &mut locks,
            clock: &clock,
            installed_runtime_validator: None,
        },
    )
    .expect_err("install should fail");

    assert!(error.to_string().contains("extract failed"));
    assert!(store.list_installations(&tool).is_empty());
    assert!(transactions.committed().is_empty());
    assert_temp_cleaned(&transactions);
}

#[test]
fn archive_path_traversal_fixture_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = linux_x64();
    let resolver =
        StaticArtifactResolver::new("https://example.invalid/fake.tar.gz", "fake.tar.gz");
    let mut downloader = FakeDownloader::new("payload");
    let checksum = FakeChecksumVerifier::passing();
    let mut extractor = FakeArchiveExtractor::with_entries(["../escape"]);
    let mut transactions = FakeInstallTransactionManager::new(temp.path());
    let mut store = InMemoryInstallStore::default();
    let mut locks = InMemoryLockManager::default();
    let clock = StaticClock::new("2026-05-21T00:00:00Z");

    let error = install_runtime(
        InstallRuntimeRequest::new(tool.clone(), version.clone(), platform),
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut store,
            lock_manager: &mut locks,
            clock: &clock,
            installed_runtime_validator: None,
        },
    )
    .expect_err("install should fail");

    assert!(error.to_string().contains("unsafe archive entry"));
    assert!(store.list_installations(&tool).is_empty());
    assert!(transactions.committed().is_empty());
    assert_temp_cleaned(&transactions);
}

#[test]
fn same_tool_version_install_is_serialized_by_lock_manager() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = linux_x64();
    let resolver =
        StaticArtifactResolver::new("https://example.invalid/fake.tar.gz", "fake.tar.gz");
    let mut downloader = FakeDownloader::new("payload");
    let checksum = FakeChecksumVerifier::passing();
    let mut extractor = FakeArchiveExtractor::with_entries(["bin/fake"]);
    let mut transactions = FakeInstallTransactionManager::new(temp.path());
    let mut store = InMemoryInstallStore::default();
    let mut locks = InMemoryLockManager::default();
    let clock = StaticClock::new("2026-05-21T00:00:00Z");
    locks
        .acquire(install_lock_key(&tool, &version, platform))
        .expect("lock should be acquired");

    let error = install_runtime(
        InstallRuntimeRequest::new(tool.clone(), version.clone(), platform),
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut store,
            lock_manager: &mut locks,
            clock: &clock,
            installed_runtime_validator: None,
        },
    )
    .expect_err("install should fail");

    assert!(error.to_string().contains("already in progress"));
    assert!(downloader.downloads().is_empty());
    assert!(transactions.begun().is_empty());
    assert!(store.list_installations(&tool).is_empty());
}

fn linux_x64() -> Platform {
    Platform::new(OperatingSystem::Linux, Architecture::X64)
}

fn install_root(root: &Path, tool: &ToolName, version: &Version, platform: Platform) -> PathBuf {
    root.join("installs")
        .join(tool.as_str())
        .join(version.raw())
        .join(platform.id())
}

fn assert_temp_cleaned(transactions: &FakeInstallTransactionManager) {
    assert_eq!(transactions.cleaned().len(), 1);
    assert!(
        !transactions.cleaned()[0].exists(),
        "temp root should be cleaned: {}",
        transactions.cleaned()[0].display()
    );
}
