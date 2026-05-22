use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{IacArtifactResolver, IacReleaseMetadata, IacReleaseVersionSource, IacTool};

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
