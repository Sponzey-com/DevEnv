use std::fs;

use devenv_adapters::archive::ManifestArchiveExtractor;
use devenv_adapters::checksum::Sha256ChecksumVerifier;
use devenv_adapters::install::FileInstallTransactionManager;
use devenv_core::{
    Architecture, ArchiveExtractor, ArchiveType, Artifact, ChecksumVerifier, InstallPlan,
    InstallTransactionManager, OperatingSystem, Platform, ToolName, Version,
};

#[test]
fn sha256_checksum_verifier_accepts_matching_digest() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = temp.path().join("artifact.bin");
    fs::write(&artifact, b"abc").expect("artifact should be written");

    Sha256ChecksumVerifier
        .verify(
            &artifact,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .expect("checksum should pass");
}

#[test]
fn sha256_checksum_verifier_rejects_mismatch() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = temp.path().join("artifact.bin");
    fs::write(&artifact, b"abc").expect("artifact should be written");

    let error = Sha256ChecksumVerifier
        .verify(
            &artifact,
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect_err("checksum should fail");

    assert!(error.to_string().contains("checksum mismatch"));
}

#[test]
fn manifest_archive_extractor_rejects_path_traversal_before_writing() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = temp.path().join("archive.manifest");
    let destination = temp.path().join("extract");
    fs::write(&archive, "../escape\n").expect("archive manifest should be written");

    let error = ManifestArchiveExtractor
        .extract(&archive, &destination, ArchiveType::TarGz)
        .expect_err("extraction should fail");

    assert!(error.to_string().contains("unsafe archive entry"));
    assert!(!destination.exists());
    assert!(!temp.path().join("escape").exists());
}

#[test]
fn plain_file_extractor_copies_single_binary_to_destination() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = temp.path().join("terraform");
    let destination = temp.path().join("extract");
    fs::write(&artifact, "binary").expect("artifact should be written");

    let manifest = ManifestArchiveExtractor
        .extract(&artifact, &destination, ArchiveType::PlainFile)
        .expect("plain file should extract");

    assert_eq!(manifest.entries(), &[std::path::PathBuf::from("terraform")]);
    assert_eq!(
        fs::read_to_string(destination.join("terraform")).expect("binary should be readable"),
        "binary"
    );
}

#[test]
fn file_install_transaction_commits_extract_root_and_cleans_temp() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let tool = ToolName::new("fake").expect("tool should be valid");
    let version = Version::new("1.0.0").expect("version should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let mut transactions = FileInstallTransactionManager::new(temp.path().join("installs"));
    let artifact = Artifact::new(
        "https://example.invalid/fake.tar.gz",
        "fake.tar.gz",
        ArchiveType::TarGz,
        None,
    );
    let install_root = transactions.install_root(&tool, &version, platform);
    let plan = InstallPlan::new(tool, version, platform, artifact, &install_root);

    let transaction = transactions.begin(&plan).expect("transaction should begin");
    fs::write(transaction.extract_root().join("runtime.txt"), "ok")
        .expect("extracted file should be written");

    transactions
        .commit(&transaction)
        .expect("transaction should commit");
    transactions
        .cleanup(&transaction)
        .expect("transaction should clean temp");

    assert!(install_root.join("runtime.txt").is_file());
    assert!(!transaction.temp_root().exists());
}
