use std::collections::BTreeMap;

use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    IacArtifactResolver, IacCatalogReleaseMetadata, IacOfficialReleaseMetadata, IacReleaseMetadata,
    IacReleaseVersionSource, IacTool, parse_iac_sha256s,
};

#[test]
fn parse_fixture_iac_release_metadata() {
    let metadata =
        IacReleaseMetadata::parse(&fixture_metadata("terraform")).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 2);
    assert_eq!(metadata.releases()[0].version().raw(), "1.8.5");
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
}

#[test]
fn parse_terraform_official_release_metadata_to_normalized_index() {
    let metadata = IacOfficialReleaseMetadata::parse_terraform(
        terraform_official_index_fixture(),
        &terraform_checksums_by_version(),
    )
    .expect("Terraform official metadata should parse");
    let index = metadata.release_index();

    assert_eq!(metadata.tool(), IacTool::Terraform);
    assert_eq!(index.tool().as_str(), "terraform");
    assert_eq!(index.provider().as_str(), "hashicorp");
    assert_eq!(index.releases()[0].version().raw(), "1.8.5");
    assert_eq!(
        index.releases()[0].artifacts()[0].artifact().checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn parse_terraform_sha256s_fixture() {
    let checksums = parse_iac_sha256s(terraform_sha256s_185()).expect("checksums should parse");

    assert_eq!(
        checksums
            .get("terraform_1.8.5_linux_amd64.zip")
            .map(String::as_str),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
}

#[test]
fn parse_opentofu_official_release_metadata_to_normalized_index() {
    let metadata = IacOfficialReleaseMetadata::parse_opentofu(
        opentofu_official_releases_fixture(),
        &opentofu_checksums_by_version(),
    )
    .expect("OpenTofu official metadata should parse");
    let index = metadata.release_index();

    assert_eq!(metadata.tool(), IacTool::OpenTofu);
    assert_eq!(index.tool().as_str(), "opentofu");
    assert_eq!(index.provider().as_str(), "opentofu");
    assert_eq!(index.releases()[0].version().raw(), "1.8.5");
    assert_eq!(
        index.releases()[0].artifacts()[0].artifact().checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn parse_opentofu_sha256s_fixture() {
    let checksums = parse_iac_sha256s(opentofu_sha256s_185()).expect("checksums should parse");

    assert_eq!(
        checksums
            .get("tofu_1.8.5_linux_amd64.zip")
            .map(String::as_str),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
}

#[test]
fn list_remote_terraform_returns_normalized_stable_versions() {
    let tool = ToolName::new("terraform").expect("tool should be valid");
    let source = IacReleaseVersionSource::new(
        IacTool::Terraform,
        IacReleaseMetadata::parse(&fixture_metadata("terraform")).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.8.5"]);
}

#[test]
fn list_remote_opentofu_returns_normalized_stable_versions() {
    let tool = ToolName::new("opentofu").expect("tool should be valid");
    let source = IacReleaseVersionSource::new(
        IacTool::OpenTofu,
        IacReleaseMetadata::parse(&fixture_metadata("tofu")).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.8.5"]);
}

#[test]
fn iac_minor_install_requirement_resolves_to_latest_matching_release() {
    let resolver = IacArtifactResolver::new(
        IacTool::Terraform,
        IacReleaseMetadata::parse(&fixture_metadata("terraform")).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("1.8").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "1.8.5");
}

#[test]
fn resolve_terraform_macos_arm64_artifact() {
    let artifact = resolve_for(
        IacTool::Terraform,
        "terraform",
        OperatingSystem::Macos,
        Architecture::Arm64,
    );

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
    assert_eq!(artifact.checksum(), Some("sha256-darwin-arm64"));
}

#[test]
fn resolve_terraform_macos_x64_artifact() {
    let artifact = resolve_for(
        IacTool::Terraform,
        "terraform",
        OperatingSystem::Macos,
        Architecture::X64,
    );

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
}

#[test]
fn resolve_terraform_linux_x64_artifact() {
    let artifact = resolve_for(
        IacTool::Terraform,
        "terraform",
        OperatingSystem::Linux,
        Architecture::X64,
    );

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
}

#[test]
fn resolve_terraform_linux_arm64_artifact() {
    let artifact = resolve_for(
        IacTool::Terraform,
        "terraform",
        OperatingSystem::Linux,
        Architecture::Arm64,
    );

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
}

#[test]
fn resolve_terraform_windows_x64_artifact() {
    let artifact = resolve_for(
        IacTool::Terraform,
        "terraform",
        OperatingSystem::Windows,
        Architecture::X64,
    );

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
}

#[test]
fn resolve_opentofu_macos_arm64_artifact() {
    let artifact = resolve_for(
        IacTool::OpenTofu,
        "tofu",
        OperatingSystem::Macos,
        Architecture::Arm64,
    );

    assert_eq!(artifact.filename(), "tofu");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
    assert_eq!(artifact.checksum(), Some("sha256-darwin-arm64"));
}

#[test]
fn official_terraform_resolves_macos_arm64_artifact() {
    let artifact = official_resolve_for(
        IacTool::Terraform,
        OperatingSystem::Macos,
        Architecture::Arm64,
    );

    assert_eq!(artifact.filename(), "terraform_1.8.5_darwin_arm64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn official_terraform_resolves_linux_x64_artifact() {
    let artifact = official_resolve_for(
        IacTool::Terraform,
        OperatingSystem::Linux,
        Architecture::X64,
    );

    assert_eq!(artifact.filename(), "terraform_1.8.5_linux_amd64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
}

#[test]
fn official_opentofu_resolves_windows_x64_artifact() {
    let artifact = official_resolve_for(
        IacTool::OpenTofu,
        OperatingSystem::Windows,
        Architecture::X64,
    );

    assert_eq!(artifact.filename(), "tofu_1.8.5_windows_amd64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
    );
}

#[test]
fn official_iac_missing_checksum_is_rejected() {
    let mut checksums = terraform_checksums_by_version();
    checksums.insert(
        "1.8.5".to_owned(),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  terraform_1.8.5_darwin_arm64.zip\n"
            .to_owned(),
    );

    let error =
        IacOfficialReleaseMetadata::parse_terraform(terraform_official_index_fixture(), &checksums)
            .expect_err("missing checksum should fail");

    assert!(error.to_string().contains("missing checksum"));
}

#[test]
fn iac_catalog_metadata_parses_to_normalized_index() {
    let metadata = IacCatalogReleaseMetadata::parse_terraform(terraform_catalog_metadata())
        .expect("Terraform catalog metadata should parse");
    let index = metadata.release_index();

    assert_eq!(metadata.tool(), IacTool::Terraform);
    assert_eq!(index.tool().as_str(), "terraform");
    assert_eq!(index.provider().as_str(), "hashicorp");
    assert_eq!(index.releases().len(), 2);
    assert_eq!(index.releases()[0].version().raw(), "1.8.5");
    assert_eq!(index.releases()[0].metadata_field("stable"), Some("true"));
    assert_eq!(
        index.releases()[0].artifacts()[0].artifact().checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn iac_catalog_single_binary_artifact_shape_is_preserved() {
    let metadata = IacCatalogReleaseMetadata::parse_terraform(terraform_catalog_metadata())
        .expect("Terraform catalog metadata should parse")
        .into_release_metadata()
        .expect("release metadata should convert");
    let resolver = IacArtifactResolver::new(IacTool::Terraform, metadata);

    let artifact = resolver
        .resolve_artifact(
            &ToolName::new("terraform").expect("tool should be valid"),
            &Version::new("1.8.5").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect("artifact should resolve");

    assert_eq!(artifact.filename(), "terraform");
    assert_eq!(artifact.archive_type(), ArchiveType::PlainFile);
    assert_eq!(
        artifact.checksum(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn iac_catalog_missing_checksum_makes_artifact_non_installable() {
    let metadata =
        IacCatalogReleaseMetadata::parse_terraform(terraform_catalog_missing_checksum_metadata())
            .expect("Terraform catalog metadata should parse")
            .into_release_metadata()
            .expect("release metadata should convert");
    let resolver = IacArtifactResolver::new(IacTool::Terraform, metadata);
    let error = resolver
        .resolve_artifact(
            &ToolName::new("terraform").expect("tool should be valid"),
            &Version::new("1.8.5").expect("version should be valid"),
            Platform::new(OperatingSystem::Macos, Architecture::Arm64),
        )
        .expect_err("checksumless artifact should not be installable");

    assert!(error.to_string().contains("does not provide a binary"));
}

fn resolve_for(
    tool: IacTool,
    binary: &str,
    os: OperatingSystem,
    arch: Architecture,
) -> devenv_core::Artifact {
    let resolver = IacArtifactResolver::new(
        tool,
        IacReleaseMetadata::parse(&fixture_metadata(binary)).expect("metadata should parse"),
    );
    resolver
        .resolve_artifact(
            &tool.tool_name(),
            &Version::new("1.8.5").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn official_resolve_for(
    tool: IacTool,
    os: OperatingSystem,
    arch: Architecture,
) -> devenv_core::Artifact {
    let metadata = match tool {
        IacTool::Terraform => IacOfficialReleaseMetadata::parse_terraform(
            terraform_official_index_fixture(),
            &terraform_checksums_by_version(),
        ),
        IacTool::OpenTofu => IacOfficialReleaseMetadata::parse_opentofu(
            opentofu_official_releases_fixture(),
            &opentofu_checksums_by_version(),
        ),
    }
    .expect("official metadata should parse")
    .into_release_metadata()
    .expect("release metadata should convert");
    let resolver = IacArtifactResolver::new(tool, metadata);
    resolver
        .resolve_artifact(
            &tool.tool_name(),
            &Version::new("1.8.5").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata(binary: &str) -> String {
    format!(
        r#"
[[release]]
version = "v1.8.5"
stable = true

[[release.file]]
filename = "{binary}_1.8.5_darwin_arm64"
os = "darwin"
arch = "arm64"
kind = "binary"
sha256 = "sha256-darwin-arm64"
size = 11

[[release.file]]
filename = "{binary}_1.8.5_darwin_amd64"
os = "darwin"
arch = "amd64"
kind = "binary"
sha256 = "sha256-darwin-amd64"
size = 12

[[release.file]]
filename = "{binary}_1.8.5_linux_amd64"
os = "linux"
arch = "amd64"
kind = "binary"
sha256 = "sha256-linux-amd64"
size = 13

[[release.file]]
filename = "{binary}_1.8.5_linux_arm64"
os = "linux"
arch = "arm64"
kind = "binary"
sha256 = "sha256-linux-arm64"
size = 14

[[release.file]]
filename = "{binary}_1.8.5_windows_amd64.exe"
os = "windows"
arch = "amd64"
kind = "binary"
sha256 = "sha256-windows-amd64"
size = 15

[[release.file]]
filename = "{binary}_1.8.5_windows_arm64.exe"
os = "windows"
arch = "arm64"
kind = "binary"
sha256 = "sha256-windows-arm64"
size = 16

[[release]]
version = "1.7.5"
stable = false

[[release.file]]
filename = "{binary}_1.7.5_linux_amd64"
os = "linux"
arch = "amd64"
kind = "binary"
"#
    )
}

fn terraform_official_index_fixture() -> &'static str {
    r#"
{
  "versions": {
    "1.8.5": {
      "builds": [
        {"os": "darwin", "arch": "arm64", "filename": "terraform_1.8.5_darwin_arm64.zip", "url": "https://example.test/terraform_1.8.5_darwin_arm64.zip"},
        {"os": "darwin", "arch": "amd64", "filename": "terraform_1.8.5_darwin_amd64.zip", "url": "https://example.test/terraform_1.8.5_darwin_amd64.zip"},
        {"os": "linux", "arch": "amd64", "filename": "terraform_1.8.5_linux_amd64.zip", "url": "https://example.test/terraform_1.8.5_linux_amd64.zip"},
        {"os": "linux", "arch": "arm64", "filename": "terraform_1.8.5_linux_arm64.zip", "url": "https://example.test/terraform_1.8.5_linux_arm64.zip"},
        {"os": "windows", "arch": "amd64", "filename": "terraform_1.8.5_windows_amd64.zip", "url": "https://example.test/terraform_1.8.5_windows_amd64.zip"}
      ]
    }
  }
}
"#
}

fn opentofu_official_releases_fixture() -> &'static str {
    r#"
[
  {
    "tag_name": "v1.8.5",
    "draft": false,
    "prerelease": false,
    "assets": [
      {"name": "tofu_1.8.5_darwin_arm64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_darwin_arm64.zip"},
      {"name": "tofu_1.8.5_darwin_amd64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_darwin_amd64.zip"},
      {"name": "tofu_1.8.5_linux_amd64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_linux_amd64.zip"},
      {"name": "tofu_1.8.5_linux_arm64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_linux_arm64.zip"},
      {"name": "tofu_1.8.5_windows_amd64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_windows_amd64.zip"},
      {"name": "tofu_1.8.5_SHA256SUMS", "browser_download_url": "https://example.test/tofu_1.8.5_SHA256SUMS"}
    ]
  }
]
"#
}

fn terraform_checksums_by_version() -> BTreeMap<String, String> {
    BTreeMap::from([("1.8.5".to_owned(), terraform_sha256s_185().to_owned())])
}

fn opentofu_checksums_by_version() -> BTreeMap<String, String> {
    BTreeMap::from([("1.8.5".to_owned(), opentofu_sha256s_185().to_owned())])
}

fn terraform_sha256s_185() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  terraform_1.8.5_darwin_arm64.zip
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  terraform_1.8.5_darwin_amd64.zip
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  terraform_1.8.5_linux_amd64.zip
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  terraform_1.8.5_linux_arm64.zip
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  terraform_1.8.5_windows_amd64.zip
"#
}

fn opentofu_sha256s_185() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tofu_1.8.5_darwin_arm64.zip
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  tofu_1.8.5_darwin_amd64.zip
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  tofu_1.8.5_linux_amd64.zip
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  tofu_1.8.5_linux_arm64.zip
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  tofu_1.8.5_windows_amd64.zip
"#
}

fn terraform_catalog_metadata() -> &'static str {
    r#"
{
  "schema_version": 1,
  "tool": "terraform",
  "provider": "hashicorp",
  "releases": [
    {
      "version": "1.8.5",
      "stable": true,
      "artifacts": [
        {
          "filename": "terraform",
          "os": "darwin",
          "arch": "arm64",
          "kind": "single-binary",
          "url": "https://example.test/terraform/1.8.5/darwin/arm64/terraform",
          "checksum": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "size": 11
        }
      ]
    },
    {
      "version": "1.7.5",
      "stable": false,
      "artifacts": [
        {
          "filename": "terraform",
          "os": "darwin",
          "arch": "arm64",
          "kind": "single-binary",
          "url": "https://example.test/terraform/1.7.5/darwin/arm64/terraform",
          "checksum": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "size": 10
        }
      ]
    }
  ]
}
"#
}

fn terraform_catalog_missing_checksum_metadata() -> &'static str {
    r#"
{
  "schema_version": 1,
  "tool": "terraform",
  "provider": "hashicorp",
  "releases": [
    {
      "version": "1.8.5",
      "stable": true,
      "artifacts": [
        {
          "filename": "terraform",
          "os": "darwin",
          "arch": "arm64",
          "kind": "single-binary",
          "url": "https://example.test/terraform/1.8.5/darwin/arm64/terraform",
          "size": 11
        }
      ]
    }
  ]
}
"#
}
