use std::collections::BTreeMap;

use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    NodeArtifactResolver, NodeCatalogReleaseMetadata, NodeOfficialReleaseMetadata,
    NodeReleaseMetadata, NodeReleaseVersionSource, parse_node_shasums256,
};

#[test]
fn parse_fixture_node_release_metadata() {
    let metadata = NodeReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 3);
    assert_eq!(metadata.releases()[0].version().raw(), "20.11.1");
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
}

#[test]
fn parse_official_node_index_to_normalized_release_index() {
    let metadata = NodeOfficialReleaseMetadata::parse(
        official_index_fixture(),
        &official_shasums_by_version(),
    )
    .expect("official metadata should parse");
    let index = metadata.release_index();

    assert_eq!(index.tool().as_str(), "node");
    assert_eq!(index.provider().as_str(), "official");
    assert_eq!(index.releases().len(), 2);
    assert_eq!(index.releases()[0].version().raw(), "20.11.1");
    assert_eq!(index.releases()[0].metadata_field("stable"), Some("true"));
    assert_eq!(
        index.releases()[0].artifacts()[0].artifact().checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn parse_official_node_shasums_fixture() {
    let checksums = parse_node_shasums256(official_shasums_20()).expect("SHASUMS256 should parse");

    assert_eq!(
        checksums
            .get("node-v20.11.1-linux-x64.tar.gz")
            .map(String::as_str),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
}

#[test]
fn official_node_metadata_lists_normalized_versions_without_v_prefix() {
    let tool = ToolName::new("node").expect("tool should be valid");
    let source = NodeReleaseVersionSource::new(
        NodeOfficialReleaseMetadata::parse(
            official_index_fixture(),
            &official_shasums_by_version(),
        )
        .expect("official metadata should parse")
        .into_release_metadata()
        .expect("release metadata should convert"),
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
fn official_node_major_install_requirement_resolves_to_latest_matching_release() {
    let resolver = official_resolver();

    let resolved = resolver
        .resolve_install_version(&Version::new("20").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "20.11.1");
}

#[test]
fn official_node_resolve_macos_arm64_artifact() {
    let artifact = official_resolve_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "node-v20.11.1-darwin-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn official_node_resolve_linux_x64_artifact() {
    let artifact = official_resolve_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "node-v20.11.1-linux-x64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
}

#[test]
fn official_node_resolve_windows_x64_artifact() {
    let artifact = official_resolve_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "node-v20.11.1-win-x64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
    );
}

#[test]
fn official_node_missing_shasums_checksum_skips_that_archive() {
    let mut shasums = official_shasums_by_version();
    shasums.insert(
        "20.11.1".to_owned(),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  node-v20.11.1-darwin-arm64.tar.gz\n"
            .to_owned(),
    );

    let metadata = NodeOfficialReleaseMetadata::parse(official_index_fixture(), &shasums)
        .expect("missing archive checksum should skip only that archive")
        .into_release_metadata()
        .expect("release metadata should convert");
    let resolver = NodeArtifactResolver::new(metadata);
    let artifact = resolver
        .resolve_artifact(
            &ToolName::new("node").expect("tool should be valid"),
            &Version::new("20.11.1").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect("checksummed archive should still resolve");
    let error = resolver
        .resolve_artifact(
            &ToolName::new("node").expect("tool should be valid"),
            &Version::new("20.11.1").expect("version should be valid"),
            Platform::new(OperatingSystem::Linux, Architecture::X64),
        )
        .expect_err("checksumless archive should not be installable");

    assert_eq!(artifact.filename(), "node-v20.11.1-darwin-arm64.tar.gz");
    assert!(error.to_string().contains("does not provide an archive"));
}

#[test]
fn official_node_release_without_checksummed_archives_is_not_listed() {
    let mut shasums = official_shasums_by_version();
    shasums.insert("21.2.0".to_owned(), String::new());
    let tool = ToolName::new("node").expect("tool should be valid");
    let source = NodeReleaseVersionSource::new(
        NodeOfficialReleaseMetadata::parse(official_index_fixture(), &shasums)
            .expect("official metadata should parse")
            .into_release_metadata()
            .expect("release metadata should convert"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["20.11.1"]);
}

#[test]
fn node_catalog_metadata_parses_to_normalized_index() {
    let metadata = NodeCatalogReleaseMetadata::parse(catalog_metadata())
        .expect("catalog metadata should parse");
    let index = metadata.release_index();

    assert_eq!(index.tool().as_str(), "node");
    assert_eq!(index.provider().as_str(), "official");
    assert_eq!(index.releases().len(), 2);
    assert_eq!(index.releases()[0].version().raw(), "20.12.0");
    assert_eq!(index.releases()[0].metadata_field("stable"), Some("true"));
    assert_eq!(
        index.releases()[0].artifacts()[0].artifact().checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn node_catalog_major_install_requirement_resolves_to_latest_patch() {
    let metadata = NodeCatalogReleaseMetadata::parse(catalog_metadata())
        .expect("catalog metadata should parse")
        .into_release_metadata()
        .expect("catalog metadata should convert");
    let resolver = NodeArtifactResolver::new(metadata);

    let resolved = resolver
        .resolve_install_version(&Version::new("20").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "20.12.0");
}

#[test]
fn node_catalog_missing_checksum_makes_artifact_non_installable() {
    let metadata = NodeCatalogReleaseMetadata::parse(catalog_metadata_missing_checksum())
        .expect("catalog metadata should parse")
        .into_release_metadata()
        .expect("catalog metadata should convert");
    let resolver = NodeArtifactResolver::new(metadata);
    let error = resolver
        .resolve_artifact(
            &ToolName::new("node").expect("tool should be valid"),
            &Version::new("20.12.0").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect_err("checksumless artifact should not be installable");

    assert!(error.to_string().contains("does not provide an archive"));
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

fn official_resolver() -> NodeArtifactResolver {
    NodeArtifactResolver::new(
        NodeOfficialReleaseMetadata::parse(
            official_index_fixture(),
            &official_shasums_by_version(),
        )
        .expect("official metadata should parse")
        .into_release_metadata()
        .expect("release metadata should convert"),
    )
}

fn official_resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    official_resolver()
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

fn official_index_fixture() -> &'static str {
    r#"
[
  {
    "version": "v20.11.1",
    "date": "2024-02-13",
    "files": [
      "osx-arm64-tar",
      "osx-x64-tar",
      "linux-x64",
      "linux-arm64",
      "win-x64-zip",
      "win-arm64-zip",
      "src",
      "headers"
    ],
    "lts": "Iron"
  },
  {
    "version": "v21.2.0",
    "date": "2023-11-14",
    "files": [
      "linux-x64"
    ],
    "lts": false
  }
]
"#
}

fn official_shasums_by_version() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("20.11.1".to_owned(), official_shasums_20().to_owned()),
        ("21.2.0".to_owned(), official_shasums_21().to_owned()),
    ])
}

fn official_shasums_20() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  node-v20.11.1-darwin-arm64.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  node-v20.11.1-darwin-x64.tar.gz
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  node-v20.11.1-linux-x64.tar.gz
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  node-v20.11.1-linux-arm64.tar.gz
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  node-v20.11.1-win-x64.zip
ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff  node-v20.11.1-win-arm64.zip
1111111111111111111111111111111111111111111111111111111111111111  node-v20.11.1.tar.gz
"#
}

fn official_shasums_21() -> &'static str {
    r#"
9999999999999999999999999999999999999999999999999999999999999999  node-v21.2.0-linux-x64.tar.gz
"#
}

fn catalog_metadata() -> &'static str {
    r#"
{
  "schema_version": 1,
  "tool": "node",
  "provider": "official",
  "releases": [
    {
      "version": "v20.12.0",
      "stable": true,
      "artifacts": [
        {
          "filename": "node-v20.12.0-darwin-arm64.tar.gz",
          "os": "darwin",
          "arch": "arm64",
          "kind": "archive",
          "url": "https://nodejs.org/dist/v20.12.0/node-v20.12.0-darwin-arm64.tar.gz",
          "checksum": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "size": 33
        }
      ]
    },
    {
      "version": "v20.11.1",
      "stable": true,
      "artifacts": [
        {
          "filename": "node-v20.11.1-darwin-arm64.tar.gz",
          "os": "darwin",
          "arch": "arm64",
          "kind": "archive",
          "url": "https://nodejs.org/dist/v20.11.1/node-v20.11.1-darwin-arm64.tar.gz",
          "checksum": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "size": 22
        }
      ]
    }
  ]
}
"#
}

fn catalog_metadata_missing_checksum() -> &'static str {
    r#"
{
  "schema_version": 1,
  "tool": "node",
  "provider": "official",
  "releases": [
    {
      "version": "v20.12.0",
      "stable": true,
      "artifacts": [
        {
          "filename": "node-v20.12.0-darwin-arm64.tar.gz",
          "os": "darwin",
          "arch": "arm64",
          "kind": "archive",
          "url": "https://nodejs.org/dist/v20.12.0/node-v20.12.0-darwin-arm64.tar.gz",
          "size": 33
        }
      ]
    }
  ]
}
"#
}
