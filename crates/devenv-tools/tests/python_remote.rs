use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    PythonArtifactResolver, PythonImplementation, PythonReleaseMetadata, PythonReleaseVersionSource,
};

#[test]
fn parse_fixture_python_release_metadata() {
    let metadata = PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 3);
    assert_eq!(metadata.releases()[0].version().raw(), "3.12.2");
    assert_eq!(
        metadata.releases()[0].implementation(),
        PythonImplementation::Cpython
    );
    assert!(metadata.releases()[0].stable());
    assert_eq!(metadata.releases()[0].files().len(), 6);
    assert_eq!(
        metadata.releases()[2].implementation(),
        PythonImplementation::Pypy
    );
}

#[test]
fn list_remote_python_returns_normalized_stable_cpython_versions() {
    let tool = ToolName::new("python").expect("tool should be valid");
    let source = PythonReleaseVersionSource::new(
        PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["3.13.0", "3.12.2"]);
}

#[test]
fn python_minor_install_requirement_resolves_to_latest_matching_cpython_release() {
    let resolver = PythonArtifactResolver::new(
        PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("3.12").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "3.12.2");
}

#[test]
fn exact_python_install_requirement_resolves_when_present() {
    let resolver = PythonArtifactResolver::new(
        PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let resolved = resolver
        .resolve_install_version(&Version::new("3.13.0").expect("version should be valid"))
        .expect("version should resolve");

    assert_eq!(resolved.raw(), "3.13.0");
}

#[test]
fn pypy_release_is_represented_but_not_install_resolved_by_cpython_resolver() {
    let resolver = PythonArtifactResolver::new(
        PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let error = resolver
        .resolve_install_version(&Version::new("3.10.14").expect("version should be valid"))
        .expect_err("PyPy install should be deferred");

    assert!(error.to_string().contains("CPython version"));
}

#[test]
fn resolve_macos_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::Arm64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-macos-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.checksum(), Some("sha256-macos-arm64"));
}

#[test]
fn resolve_macos_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Macos, Architecture::X64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-macos-x64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::X64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-linux-x64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_linux_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Linux, Architecture::Arm64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-linux-arm64.tar.gz");
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
}

#[test]
fn resolve_windows_x64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-windows-x64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

#[test]
fn resolve_windows_arm64_artifact() {
    let artifact = resolve_for(OperatingSystem::Windows, Architecture::Arm64);

    assert_eq!(artifact.filename(), "cpython-3.12.2-windows-arm64.zip");
    assert_eq!(artifact.archive_type(), ArchiveType::Zip);
}

fn resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let resolver = PythonArtifactResolver::new(
        PythonReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    resolver
        .resolve_artifact(
            &ToolName::new("python").expect("tool should be valid"),
            &Version::new("3.12.2").expect("version should be valid"),
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata() -> &'static str {
    r#"
[[release]]
version = "Python 3.12.2"
implementation = "cpython"
stable = true

[[release.file]]
filename = "cpython-3.12.2-macos-arm64.tar.gz"
os = "macos"
arch = "arm64"
kind = "archive"
sha256 = "sha256-macos-arm64"
size = 11

[[release.file]]
filename = "cpython-3.12.2-macos-x64.tar.gz"
os = "macos"
arch = "x64"
kind = "archive"
sha256 = "sha256-macos-x64"
size = 12

[[release.file]]
filename = "cpython-3.12.2-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
sha256 = "sha256-linux-x64"
size = 13

[[release.file]]
filename = "cpython-3.12.2-linux-arm64.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
sha256 = "sha256-linux-arm64"
size = 14

[[release.file]]
filename = "cpython-3.12.2-windows-x64.zip"
os = "windows"
arch = "x64"
kind = "archive"
sha256 = "sha256-windows-x64"
size = 15

[[release.file]]
filename = "cpython-3.12.2-windows-arm64.zip"
os = "windows"
arch = "arm64"
kind = "archive"
sha256 = "sha256-windows-arm64"
size = 16

[[release]]
version = "3.13.0"
implementation = "cpython"
stable = true

[[release.file]]
filename = "cpython-3.13.0-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"

[[release]]
version = "3.10.14"
implementation = "pypy"
stable = true

[[release.file]]
filename = "pypy3.10-v7.3.15-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
"#
}
