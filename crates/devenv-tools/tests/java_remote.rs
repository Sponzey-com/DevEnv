use devenv_core::{
    Architecture, ArchiveType, ArtifactResolver, OperatingSystem, Platform, ToolName, Version,
    VersionSource,
};
use devenv_tools::{
    JavaArtifactResolver, JavaDistribution, JavaReleaseMetadata, JavaReleaseVersionSource,
};

#[test]
fn parse_fixture_temurin_metadata() {
    let metadata = JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse");

    assert_eq!(metadata.releases().len(), 3);
    assert_eq!(metadata.releases()[0].version().raw(), "17.0.11");
    assert_eq!(metadata.releases()[0].feature(), 17);
    assert_eq!(metadata.releases()[0].distribution().as_str(), "temurin");
    assert_eq!(metadata.releases()[0].files().len(), 6);
    assert_eq!(metadata.releases()[1].distribution().as_str(), "temurin");
}

#[test]
fn list_remote_java_returns_feature_and_exact_versions() {
    let tool = ToolName::new("java").expect("tool should be valid");
    let source = JavaReleaseVersionSource::new(
        JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );

    let versions = source
        .list_versions(&tool)
        .expect("versions should list")
        .into_iter()
        .map(|version| version.raw().to_owned())
        .collect::<Vec<_>>();

    assert!(versions.contains(&"17".to_owned()));
    assert!(versions.contains(&"17.0.11".to_owned()));
    assert!(versions.contains(&"21".to_owned()));
    assert!(versions.contains(&"21.0.2".to_owned()));
    assert!(!versions.contains(&"16".to_owned()));
}

#[test]
fn java_feature_version_resolves_to_temurin_artifact() {
    let resolver = JavaArtifactResolver::new(
        JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    let install_version = resolver
        .resolve_install_version(&Version::new("17").expect("version should be valid"))
        .expect("install version should resolve");

    assert_eq!(install_version.raw(), "17.0.11-temurin");

    let artifact = resolver
        .resolve_artifact(
            &ToolName::new("java").expect("tool should be valid"),
            &install_version,
            Platform::new(OperatingSystem::Linux, Architecture::X64),
        )
        .expect("artifact should resolve");

    assert_eq!(
        artifact.filename(),
        "OpenJDK17U-jdk_x64_linux_hotspot_17.0.11_9.tar.gz"
    );
    assert_eq!(artifact.archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.checksum(), Some("sha256-linux-x64"));
}

#[test]
fn java_exact_version_resolves_when_present() {
    let resolver = JavaArtifactResolver::new(
        JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    let install_version = resolver
        .resolve_install_version(&Version::new("17.0.11").expect("version should be valid"))
        .expect("install version should resolve");

    assert_eq!(install_version.raw(), "17.0.11-temurin");
}

#[test]
fn platform_mapping_resolves_macos_linux_and_windows_archives() {
    assert_eq!(
        resolve_for(OperatingSystem::Macos, Architecture::Arm64).filename(),
        "OpenJDK17U-jdk_aarch64_mac_hotspot_17.0.11_9.tar.gz"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Macos, Architecture::X64).filename(),
        "OpenJDK17U-jdk_x64_mac_hotspot_17.0.11_9.tar.gz"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Linux, Architecture::Arm64).filename(),
        "OpenJDK17U-jdk_aarch64_linux_hotspot_17.0.11_9.tar.gz"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Linux, Architecture::X64).filename(),
        "OpenJDK17U-jdk_x64_linux_hotspot_17.0.11_9.tar.gz"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Windows, Architecture::Arm64).filename(),
        "OpenJDK17U-jdk_aarch64_windows_hotspot_17.0.11_9.zip"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Windows, Architecture::X64).filename(),
        "OpenJDK17U-jdk_x64_windows_hotspot_17.0.11_9.zip"
    );
    assert_eq!(
        resolve_for(OperatingSystem::Windows, Architecture::X64).archive_type(),
        ArchiveType::Zip
    );
}

#[test]
fn unknown_distribution_produces_actionable_error() {
    let resolver = JavaArtifactResolver::with_distribution(
        JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
        JavaDistribution::named("zulu").expect("distribution should be valid"),
    );

    let error = resolver
        .resolve_install_version(&Version::new("17").expect("version should be valid"))
        .expect_err("unknown distribution should fail");

    assert!(error.to_string().contains("zulu"));
    assert!(error.to_string().contains("not found"));
}

fn resolve_for(os: OperatingSystem, arch: Architecture) -> devenv_core::Artifact {
    let resolver = JavaArtifactResolver::new(
        JavaReleaseMetadata::parse(fixture_metadata()).expect("metadata should parse"),
    );
    let install_version = resolver
        .resolve_install_version(&Version::new("17").expect("version should be valid"))
        .expect("install version should resolve");
    resolver
        .resolve_artifact(
            &ToolName::new("java").expect("tool should be valid"),
            &install_version,
            Platform::new(os, arch),
        )
        .expect("artifact should resolve")
}

fn fixture_metadata() -> &'static str {
    r#"
[[release]]
version = "17.0.11"
feature = 17
distribution = "temurin"
stable = true

[[release.file]]
filename = "OpenJDK17U-jdk_aarch64_mac_hotspot_17.0.11_9.tar.gz"
os = "macos"
arch = "arm64"
kind = "jdk"
sha256 = "sha256-macos-arm64"
size = 11

[[release.file]]
filename = "OpenJDK17U-jdk_x64_mac_hotspot_17.0.11_9.tar.gz"
os = "macos"
arch = "x64"
kind = "jdk"
sha256 = "sha256-macos-x64"
size = 12

[[release.file]]
filename = "OpenJDK17U-jdk_aarch64_linux_hotspot_17.0.11_9.tar.gz"
os = "linux"
arch = "arm64"
kind = "jdk"
sha256 = "sha256-linux-arm64"
size = 13

[[release.file]]
filename = "OpenJDK17U-jdk_x64_linux_hotspot_17.0.11_9.tar.gz"
os = "linux"
arch = "x64"
kind = "jdk"
sha256 = "sha256-linux-x64"
size = 14

[[release.file]]
filename = "OpenJDK17U-jdk_aarch64_windows_hotspot_17.0.11_9.zip"
os = "windows"
arch = "arm64"
kind = "jdk"
sha256 = "sha256-windows-arm64"
size = 15

[[release.file]]
filename = "OpenJDK17U-jdk_x64_windows_hotspot_17.0.11_9.zip"
os = "windows"
arch = "x64"
kind = "jdk"
sha256 = "sha256-windows-x64"
size = 16

[[release]]
version = "21.0.2"
stable = true

[[release.file]]
filename = "OpenJDK21U-jdk_x64_linux_hotspot_21.0.2_13.tar.gz"
os = "linux"
arch = "x64"
kind = "jdk"

[[release]]
version = "16.0.2"
stable = false

[[release.file]]
filename = "OpenJDK16U-jdk_x64_linux_hotspot_16.0.2_7.tar.gz"
os = "linux"
arch = "x64"
kind = "jdk"
"#
}
