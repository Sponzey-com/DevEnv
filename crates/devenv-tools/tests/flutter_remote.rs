use std::collections::BTreeMap;

use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    FLUTTER_OFFICIAL_BASE_URL, FlutterArtifactResolver, FlutterOfficialReleaseMetadata,
    FlutterReleaseMetadata, FlutterReleaseVersionSource,
};

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
fn parse_official_flutter_stable_release_metadata_to_normalized_index() {
    let official = FlutterOfficialReleaseMetadata::parse_stable(&official_payloads())
        .expect("official metadata should parse");
    let index = official.release_index();

    assert_eq!(index.tool().as_str(), "flutter");
    assert_eq!(index.provider().as_str(), "stable");
    assert_eq!(index.releases().len(), 1);
    assert_eq!(index.releases()[0].version().raw(), "3.24.0");
    assert_eq!(
        index.releases()[0].metadata_field("channel"),
        Some("stable")
    );
    assert_eq!(index.releases()[0].artifacts().len(), 4);
}

#[test]
fn official_flutter_channel_metadata_is_preserved() {
    let metadata = FlutterOfficialReleaseMetadata::parse_stable(&official_payloads())
        .and_then(FlutterOfficialReleaseMetadata::into_release_metadata)
        .expect("official metadata should parse");

    assert_eq!(metadata.releases()[0].channel(), "stable");
    assert!(metadata.releases()[0].stable());
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
fn official_flutter_resolves_macos_arm64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "flutter_macos_arm64_3.24.0-stable.zip");
    assert_eq!(
        artifact.url(),
        format!("{FLUTTER_OFFICIAL_BASE_URL}/stable/macos/flutter_macos_arm64_3.24.0-stable.zip")
    );
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
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
fn official_flutter_resolves_linux_x64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "flutter_linux_3.24.0-stable.tar.xz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarXz);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
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
fn official_flutter_resolves_windows_x64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "flutter_windows_3.24.0-stable.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
    );
}

#[test]
fn official_flutter_rejects_missing_checksum() {
    let mut payloads = official_payloads();
    payloads.insert(
        "linux".to_owned(),
        r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "releases": [
    {
      "hash": "linux-x64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_arch": "x64",
      "archive": "stable/linux/flutter_linux_3.24.0-stable.tar.xz",
      "sha256": "not-a-sha"
    }
  ]
}
"#
        .to_owned(),
    );

    let error = FlutterOfficialReleaseMetadata::parse_stable(&payloads)
        .expect_err("missing checksum should fail");

    assert!(error.to_string().contains("invalid sha256 checksum"));
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

fn resolve_official_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let metadata = FlutterOfficialReleaseMetadata::parse_stable(&official_payloads())
        .and_then(FlutterOfficialReleaseMetadata::into_release_metadata)
        .expect("official metadata should parse");
    let resolver = FlutterArtifactResolver::new(metadata);
    resolver
        .resolve_artifact(
            &ToolName::new("flutter").expect("tool should be valid"),
            &Version::new("3.24.0").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn official_payloads() -> BTreeMap<String, String> {
    [
        ("macos", official_macos_payload()),
        ("linux", official_linux_payload()),
        ("windows", official_windows_payload()),
    ]
    .into_iter()
    .map(|(platform, payload)| (platform.to_owned(), payload.to_owned()))
    .collect()
}

fn official_macos_payload() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "macos-arm64"},
  "releases": [
    {
      "hash": "macos-arm64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_version": "3.5.0",
      "dart_sdk_arch": "arm64",
      "release_date": "2024-08-01T00:00:00.000000Z",
      "archive": "stable/macos/flutter_macos_arm64_3.24.0-stable.zip",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "hash": "macos-x64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_version": "3.5.0",
      "dart_sdk_arch": "x64",
      "release_date": "2024-08-01T00:00:00.000000Z",
      "archive": "stable/macos/flutter_macos_3.24.0-stable.zip",
      "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    },
    {
      "hash": "macos-beta",
      "channel": "beta",
      "version": "3.25.0-0.1.pre",
      "dart_sdk_arch": "arm64",
      "archive": "beta/macos/flutter_macos_arm64_3.25.0-0.1.pre-beta.zip",
      "sha256": "1111111111111111111111111111111111111111111111111111111111111111"
    }
  ]
}
"#
}

fn official_linux_payload() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "linux-x64"},
  "releases": [
    {
      "hash": "linux-x64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_version": "3.5.0",
      "archive": "stable/linux/flutter_linux_3.24.0-stable.tar.xz",
      "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    }
  ]
}
"#
}

fn official_windows_payload() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "windows-x64"},
  "releases": [
    {
      "hash": "windows-x64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_version": "3.5.0",
      "archive": "stable/windows/flutter_windows_3.24.0-stable.zip",
      "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    }
  ]
}
"#
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
