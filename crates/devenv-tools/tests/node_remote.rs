use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{NodeArtifactResolver, NodeReleaseMetadata, NodeReleaseVersionSource};

#[test]
fn parse_fixture_node_release_metadata() {
    let metadata = NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 3);
    assert_eq!(metadata.releases()[0].version().raw(), "20.11.1");
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
}

#[test]
fn list_remote_node_returns_normalized_stable_versions() {
    let tool = ToolName::new("node").expect("tool should be valid");
    let source = NodeReleaseVersionSource::new(
        NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["21.2.0", "20.11.1"]);
}

#[test]
fn node_major_install_requirement_resolves_to_latest_matching_release() {
    let resolver = NodeArtifactResolver::new(
        NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("20").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "20.11.1");
}

#[test]
fn exact_node_install_requirement_resolves_when_present() {
    let resolver = NodeArtifactResolver::new(
        NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("21.2.0").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "21.2.0");
}

#[test]
fn resolve_macos_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "node-v20.11.1-darwin-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.checksum(), Some("sha256-darwin-arm64"));
}

#[test]
fn resolve_macos_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::X64);

    assert_eq!(artifact.filename(), "node-v20.11.1-darwin-x64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "node-v20.11.1-linux-x64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::Arm64);

    assert_eq!(artifact.filename(), "node-v20.11.1-linux-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_windows_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "node-v20.11.1-win-x64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn resolve_windows_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::Arm64);

    assert_eq!(artifact.filename(), "node-v20.11.1-win-arm64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

fn resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let resolver = NodeArtifactResolver::new(
        NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    resolver
        .resolve_artifact(
            &ToolName::new("node").expect("tool should be valid"),
            &Version::new("20.11.1").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata() -> &'static str {
    r#"
[[release]]
version = "v20.11.1"
stable = true

[[release.file]]
filename = "node-v20.11.1-darwin-arm64.tar.gz"
os = "darwin"
arch = "arm64"
kind = "archive"
sha256 = "sha256-darwin-arm64"
size = 11

[[release.file]]
filename = "node-v20.11.1-darwin-x64.tar.gz"
os = "darwin"
arch = "x64"
kind = "archive"
sha256 = "sha256-darwin-x64"
size = 12

[[release.file]]
filename = "node-v20.11.1-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
sha256 = "sha256-linux-x64"
size = 13

[[release.file]]
filename = "node-v20.11.1-linux-arm64.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
sha256 = "sha256-linux-arm64"
size = 14

[[release.file]]
filename = "node-v20.11.1-win-x64.zip"
os = "win"
arch = "x64"
kind = "archive"
sha256 = "sha256-win-x64"
size = 15

[[release.file]]
filename = "node-v20.11.1-win-arm64.zip"
os = "win"
arch = "arm64"
kind = "archive"
sha256 = "sha256-win-arm64"
size = 16

[[release]]
version = "v21.2.0"
stable = true

[[release.file]]
filename = "node-v21.2.0-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"

[[release]]
version = "v19.9.0"
stable = false

[[release.file]]
filename = "node-v19.9.0-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
"#
}
