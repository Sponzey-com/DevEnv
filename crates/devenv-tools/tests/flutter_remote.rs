use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{FlutterArtifactResolver, FlutterReleaseMetadata, FlutterReleaseVersionSource};

#[test]
fn parse_fixture_flutter_release_metadata() {
    let metadata =
        FlutterReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 2);
    assert_eq!(metadata.releases()[0].version().raw(), "3.24.0");
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
}

#[test]
fn list_remote_flutter_returns_normalized_stable_versions() {
    let tool = ToolName::new("flutter").expect("tool should be valid");
    let source = FlutterReleaseVersionSource::new(
        FlutterReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["3.24.0"]);
}

#[test]
fn flutter_minor_install_requirement_resolves_to_latest_matching_release() {
    let resolver = FlutterArtifactResolver::new(
        FlutterReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("3.24").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "3.24.0");
}

#[test]
fn resolve_macos_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "flutter_macos_arm64_3.24.0-stable.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(artifact.checksum(), Some("sha256-macos-arm64"));
}

#[test]
fn resolve_macos_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::X64);

    assert_eq!(artifact.filename(), "flutter_macos_x64_3.24.0-stable.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn resolve_linux_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(
        artifact.filename(),
        "flutter_linux_x64_3.24.0-stable.tar.gz"
    );
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::Arm64);

    assert_eq!(
        artifact.filename(),
        "flutter_linux_arm64_3.24.0-stable.tar.gz"
    );
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_windows_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "flutter_windows_x64_3.24.0-stable.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn resolve_windows_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::Arm64);

    assert_eq!(
        artifact.filename(),
        "flutter_windows_arm64_3.24.0-stable.zip"
    );
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

fn resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let resolver = FlutterArtifactResolver::new(
        FlutterReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    resolver
        .resolve_artifact(
            &ToolName::new("flutter").expect("tool should be valid"),
            &Version::new("3.24.0").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata() -> &'static str {
    r#"
[[release]]
version = "Flutter 3.24.0"
channel = "stable"
stable = true

[[release.file]]
filename = "flutter_macos_arm64_3.24.0-stable.zip"
os = "macos"
arch = "arm64"
kind = "archive"
sha256 = "sha256-macos-arm64"
size = 11

[[release.file]]
filename = "flutter_macos_x64_3.24.0-stable.zip"
os = "macos"
arch = "x64"
kind = "archive"
sha256 = "sha256-macos-x64"
size = 12

[[release.file]]
filename = "flutter_linux_x64_3.24.0-stable.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
sha256 = "sha256-linux-x64"
size = 13

[[release.file]]
filename = "flutter_linux_arm64_3.24.0-stable.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
sha256 = "sha256-linux-arm64"
size = 14

[[release.file]]
filename = "flutter_windows_x64_3.24.0-stable.zip"
os = "windows"
arch = "x64"
kind = "archive"
sha256 = "sha256-windows-x64"
size = 15

[[release.file]]
filename = "flutter_windows_arm64_3.24.0-stable.zip"
os = "windows"
arch = "arm64"
kind = "archive"
sha256 = "sha256-windows-arm64"
size = 16

[[release]]
version = "3.22.3"
channel = "stable"
stable = false

[[release.file]]
filename = "flutter_linux_x64_3.22.3-stable.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
"#
}
