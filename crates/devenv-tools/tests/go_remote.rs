use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    GoArtifactResolver, GoCatalogReleaseMetadata, GoReleaseMetadata, GoReleaseVersionSource,
};
use devenv_tools::{
    GoOfficialReleaseMetadata, GoRemoteArtifactResolver, GoRemoteReleaseVersionSource,
};

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

#[test]
fn parse_official_go_release_metadata_to_normalized_index() {
    let metadata = GoOfficialReleaseMetadata::parse(official_metadata())
        .expect("official metadata should parse");
    let index = metadata.release_index();

    assert_eq!(index.tool().as_str(), "go");
    assert_eq!(index.provider().as_str(), "official");
    assert_eq!(index.releases().len(), 2);
    assert_eq!(index.releases()[0].version().raw(), "1.23.4");
    assert!(
        index
            .releases()
            .iter()
            .all(|release| !release.version().raw().contains("rc"))
    );
    assert_eq!(
        index.releases()[0].metadata_field("upstream_version"),
        Some("go1.23.4")
    );
    assert_eq!(index.releases()[0].metadata_field("stable"), Some("true"));
}

#[test]
fn list_remote_go_from_official_metadata_returns_stable_normalized_versions() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let metadata = GoOfficialReleaseMetadata::parse(official_metadata())
        .expect("official metadata should parse");
    let source = GoRemoteReleaseVersionSource::new(metadata.into_release_index());

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.23.4"]);
}

#[test]
fn resolve_official_macos_arm64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "go1.23.4.darwin-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn resolve_official_macos_x64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Macos, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.23.4.darwin-amd64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_official_linux_x64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.23.4.linux-amd64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.size(), Some(33));
}

#[test]
fn resolve_official_windows_x64_artifact() {
    let artifact = resolve_official_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "go1.23.4.windows-amd64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn official_metadata_can_feed_existing_go_release_resolver() {
    let metadata = GoOfficialReleaseMetadata::parse(official_metadata())
        .expect("official metadata should parse")
        .into_release_metadata()
        .expect("official metadata should convert");
    let resolver = GoArtifactResolver::new(metadata);
    let artifact = resolver
        .resolve_artifact(
            &ToolName::new("go").expect("tool should be valid"),
            &Version::new("1.23.4").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect("artifact should resolve");

    assert_eq!(artifact.filename(), "go1.23.4.darwin-arm64.tar.gz");
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn go_catalog_metadata_parses_to_normalized_index() {
    let metadata =
        GoCatalogReleaseMetadata::parse(catalog_metadata()).expect("catalog metadata should parse");
    let index = metadata.release_index();

    assert_eq!(index.tool().as_str(), "go");
    assert_eq!(index.provider().as_str(), "official");
    assert_eq!(index.releases().len(), 2);
    assert_eq!(index.releases()[0].version().raw(), "1.23.5");
    assert_eq!(index.releases()[0].metadata_field("yanked"), Some("true"));
    assert_eq!(index.releases()[1].version().raw(), "1.23.4");
    assert_eq!(index.releases()[1].metadata_field("stable"), Some("true"));
}

#[test]
fn go_catalog_metadata_excludes_yanked_versions_from_default_listing() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let metadata =
        GoCatalogReleaseMetadata::parse(catalog_metadata()).expect("catalog metadata should parse");
    let source = GoRemoteReleaseVersionSource::new(metadata.into_release_index());

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.23.4"]);
}

#[test]
fn go_catalog_metadata_can_feed_existing_go_release_resolver() {
    let metadata = GoCatalogReleaseMetadata::parse(catalog_metadata())
        .expect("catalog metadata should parse")
        .into_release_metadata()
        .expect("catalog metadata should convert");
    let resolver = GoArtifactResolver::new(metadata);
    let artifact = resolver
        .resolve_artifact(
            &ToolName::new("go").expect("tool should be valid"),
            &Version::new("1.23.4").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect("artifact should resolve");

    assert_eq!(artifact.filename(), "go1.23.4.darwin-arm64.tar.gz");
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
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

fn resolve_official_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let metadata = GoOfficialReleaseMetadata::parse(official_metadata())
        .expect("official metadata should parse");
    let resolver = GoRemoteArtifactResolver::new(metadata.into_release_index());
    resolver
        .resolve_artifact(
            &ToolName::new("go").expect("tool should be valid"),
            &Version::new("1.23.4").expect("version should be valid"),
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

fn official_metadata() -> &'static str {
    r#"
[
  {
    "version": "go1.23.4",
    "stable": true,
    "files": [
      {
        "filename": "go1.23.4.darwin-arm64.tar.gz",
        "os": "darwin",
        "arch": "arm64",
        "version": "go1.23.4",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "size": 11,
        "kind": "archive"
      },
      {
        "filename": "go1.23.4.darwin-amd64.tar.gz",
        "os": "darwin",
        "arch": "amd64",
        "version": "go1.23.4",
        "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "size": 22,
        "kind": "archive"
      },
      {
        "filename": "go1.23.4.linux-amd64.tar.gz",
        "os": "linux",
        "arch": "amd64",
        "version": "go1.23.4",
        "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "size": 33,
        "kind": "archive"
      },
      {
        "filename": "go1.23.4.windows-amd64.zip",
        "os": "windows",
        "arch": "amd64",
        "version": "go1.23.4",
        "sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "size": 44,
        "kind": "archive"
      },
      {
        "filename": "go1.23.4.src.tar.gz",
        "os": "",
        "arch": "",
        "version": "go1.23.4",
        "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "size": 55,
        "kind": "source"
      }
    ]
  },
  {
    "version": "go1.23.5",
    "stable": false,
    "files": []
  },
  {
    "version": "go1.26rc3",
    "stable": false,
    "files": []
  }
]
"#
}

fn catalog_metadata() -> &'static str {
    r#"
{
  "schema_version": 1,
  "tool": "go",
  "provider": "official",
  "releases": [
    {
      "version": "1.23.5",
      "stable": true,
      "yanked": true,
      "yanked_reason": "catalog test",
      "artifacts": [
        {
          "filename": "go1.23.5.darwin-arm64.tar.gz",
          "os": "darwin",
          "arch": "arm64",
          "kind": "archive",
          "url": "https://go.dev/dl/go1.23.5.darwin-arm64.tar.gz",
          "checksum": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "size": 44
        }
      ]
    },
    {
      "version": "go1.23.4",
      "stable": true,
      "artifacts": [
        {
          "filename": "go1.23.4.darwin-arm64.tar.gz",
          "os": "darwin",
          "arch": "arm64",
          "kind": "archive",
          "url": "https://go.dev/dl/go1.23.4.darwin-arm64.tar.gz",
          "checksum": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "size": 33
        }
      ]
    }
  ]
}
"#
}
