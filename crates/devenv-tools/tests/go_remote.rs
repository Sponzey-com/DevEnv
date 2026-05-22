use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{GoArtifactResolver, GoReleaseMetadata, GoReleaseVersionSource};

#[test]
fn parse_fixture_go_release_metadata() {
    let metadata = GoReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 2);
    assert_eq!(metadata.releases()[0].version().raw(), "1.22.5");
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
}

#[test]
fn list_remote_go_returns_normalized_stable_versions() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let source = GoReleaseVersionSource::new(
        GoReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.22.5"]);
}

#[test]
fn resolve_macos_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "go1.22.5.darwin-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.checksum(), Some("sha256-darwin-arm64"));
}

#[test]
fn resolve_macos_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.22.5.darwin-amd64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.22.5.linux-amd64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::Arm64);

    assert_eq!(artifact.filename(), "go1.22.5.linux-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_windows_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.22.5.windows-amd64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn resolve_windows_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::Arm64);

    assert_eq!(artifact.filename(), "go1.22.5.windows-arm64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

fn resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let resolver = GoArtifactResolver::new(
        GoReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    resolver
        .resolve_artifact(
            &ToolName::new("go").expect("tool should be valid"),
            &Version::new("1.22.5").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata() -> &'static str {
    r#"
[[release]]
version = "go1.22.5"
stable = true

[[release.file]]
filename = "go1.22.5.darwin-arm64.tar.gz"
os = "darwin"
arch = "arm64"
kind = "archive"
sha256 = "sha256-darwin-arm64"
size = 11

[[release.file]]
filename = "go1.22.5.darwin-amd64.tar.gz"
os = "darwin"
arch = "amd64"
kind = "archive"
sha256 = "sha256-darwin-amd64"
size = 12

[[release.file]]
filename = "go1.22.5.linux-amd64.tar.gz"
os = "linux"
arch = "amd64"
kind = "archive"
sha256 = "sha256-linux-amd64"
size = 13

[[release.file]]
filename = "go1.22.5.linux-arm64.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
sha256 = "sha256-linux-arm64"
size = 14

[[release.file]]
filename = "go1.22.5.windows-amd64.zip"
os = "windows"
arch = "amd64"
kind = "archive"
sha256 = "sha256-windows-amd64"
size = 15

[[release.file]]
filename = "go1.22.5.windows-arm64.zip"
os = "windows"
arch = "arm64"
kind = "archive"
sha256 = "sha256-windows-arm64"
size = 16

[[release]]
version = "go1.21.0"
stable = false

[[release.file]]
filename = "go1.21.0.linux-amd64.tar.gz"
os = "linux"
arch = "amd64"
kind = "archive"
"#
}
