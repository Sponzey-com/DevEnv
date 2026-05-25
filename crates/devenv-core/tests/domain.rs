use std::str::FromStr;

use devenv_core::{
    Architecture, ArchiveType, Artifact, ChecksumPolicy, MetadataCacheEntry, MetadataCacheKey,
    MetadataFreshness, MetadataPayloadKind, OperatingSystem, Platform, PlatformSupport,
    ProviderCapability, ProviderId, ProviderSelectorDimension, ProviderSourceKind, RemoteRelease,
    RemoteReleaseIndex, ResolvedArtifact, SupportLevel, ToolName, ToolSpec, Version,
    VersionRequirement,
};

#[test]
fn tool_name_normalizes_to_lowercase() {
    let name = ToolName::new("JaVa").expect("tool name should be valid");

    assert_eq!(name.as_str(), "java");
}

#[test]
fn provider_id_normalizes_to_lowercase_and_rejects_empty_values() {
    let provider = ProviderId::new("Temurin").expect("provider id should be valid");

    assert_eq!(provider.as_str(), "temurin");
    assert!(ProviderId::new("   ").is_err());
}

#[test]
fn tool_spec_parses_java_requirement_without_semver_assumption() {
    let spec = ToolSpec::from_str("java@17").expect("java spec should parse");

    assert_eq!(spec.tool().as_str(), "java");
    assert_eq!(spec.requirement().raw(), "17");
    assert!(matches!(spec.requirement(), VersionRequirement::Exact(_)));
}

#[test]
fn tool_spec_parses_go_requirement_without_java_specific_semantics() {
    let spec = ToolSpec::from_str("go@1.22.5").expect("go spec should parse");

    assert_eq!(spec.tool().as_str(), "go");
    assert_eq!(spec.requirement().raw(), "1.22.5");
    assert!(matches!(spec.requirement(), VersionRequirement::Exact(_)));
}

#[test]
fn invalid_tool_specs_are_actionable() {
    for input in ["@17", "java@", "java@@17", ""] {
        let error = ToolSpec::from_str(input).expect_err("spec should be invalid");
        let message = error.to_string();

        assert!(message.contains("invalid tool spec"));
        assert!(message.contains("<tool>@<version>"));
        assert!(message.contains("java@17"));
    }
}

#[test]
fn platform_models_supported_operating_systems_and_architectures() {
    let macos_arm64 = Platform::new(OperatingSystem::Macos, Architecture::Arm64);
    let linux_x64 = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let windows_x64 = Platform::new(OperatingSystem::Windows, Architecture::X64);

    assert_eq!(macos_arm64.os(), OperatingSystem::Macos);
    assert_eq!(macos_arm64.architecture(), Architecture::Arm64);
    assert_eq!(linux_x64.os(), OperatingSystem::Linux);
    assert_eq!(linux_x64.architecture(), Architecture::X64);
    assert_eq!(windows_x64.os(), OperatingSystem::Windows);
    assert_eq!(windows_x64.architecture(), Architecture::X64);
}

#[test]
fn platform_support_detects_supported_and_unsupported_platforms() {
    let linux_x64 = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let macos_arm64 = Platform::new(OperatingSystem::Macos, Architecture::Arm64);
    let support = PlatformSupport::new([linux_x64, linux_x64]);

    assert_eq!(support.platforms(), &[linux_x64]);
    assert!(support.supports(linux_x64));
    assert!(!support.supports(macos_arm64));
}

#[test]
fn provider_capability_models_support_level_and_selectors() {
    let capability = ProviderCapability::new(
        ToolName::new("java").expect("tool should parse"),
        ProviderId::new("temurin").expect("provider should parse"),
        "Eclipse Temurin",
        SupportLevel::Direct,
        ProviderSourceKind::OfficialApi,
        ChecksumPolicy::Required,
    )
    .with_selector_dimension(ProviderSelectorDimension::Distribution)
    .with_supported_platforms([Platform::new(OperatingSystem::Linux, Architecture::X64)]);

    assert_eq!(capability.tool().as_str(), "java");
    assert_eq!(capability.provider().as_str(), "temurin");
    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::Distribution));
    assert!(capability.direct_install_unavailable_reason().is_none());
}

#[test]
fn non_direct_provider_explains_install_unavailability() {
    let capability = ProviderCapability::new(
        ToolName::new("ruby").expect("tool should parse"),
        ProviderId::new("local").expect("provider should parse"),
        "Ruby local registration",
        SupportLevel::LocalOnly,
        ProviderSourceKind::LocalFixture,
        ChecksumPolicy::Unavailable,
    )
    .with_unavailable_reason("Ruby remote install is deferred")
    .with_next_action("Register an existing Ruby runtime with `devenv add ruby <path>`");

    assert_eq!(
        capability.direct_install_unavailable_reason().as_deref(),
        Some("Ruby remote install is deferred")
    );
    assert_eq!(
        capability.next_action(),
        Some("Register an existing Ruby runtime with `devenv add ruby <path>`")
    );
}

#[test]
fn normalized_release_index_carries_provider_artifact_metadata() {
    let tool = ToolName::new("java").expect("tool should parse");
    let provider = ProviderId::new("temurin").expect("provider should parse");
    let version = Version::new("21.0.2+13").expect("version should parse");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let artifact = Artifact::new(
        "https://example.invalid/jdk.tar.gz",
        "jdk.tar.gz",
        ArchiveType::TarGz,
        Some("sha256:fixture".to_owned()),
    );
    let resolved = ResolvedArtifact::new(
        tool.clone(),
        provider.clone(),
        version.clone(),
        platform,
        artifact,
    )
    .with_metadata_field("distribution", "temurin");
    let release = RemoteRelease::new(version.clone(), [resolved.clone()])
        .with_metadata_field("feature", "21");
    let index = RemoteReleaseIndex::new(tool.clone(), provider.clone(), [release]);

    assert_eq!(index.tool(), &tool);
    assert_eq!(index.provider(), &provider);
    let release = index
        .release_for_version(&version)
        .expect("release should be present");
    assert_eq!(release.metadata_field("feature"), Some("21"));
    let artifact = &release.artifacts()[0];
    assert_eq!(artifact.provider(), &provider);
    assert_eq!(artifact.version(), &version);
    assert_eq!(artifact.platform(), platform);
    assert_eq!(artifact.artifact().archive_type(), ArchiveType::TarGz);
    assert_eq!(artifact.artifact().checksum(), Some("sha256:fixture"));
    assert_eq!(artifact.metadata_field("distribution"), Some("temurin"));
}

#[test]
fn metadata_cache_entry_reports_freshness_from_unix_timestamps() {
    let entry = MetadataCacheEntry::new(
        MetadataCacheKey::new(
            ToolName::new("go").expect("tool should parse"),
            ProviderId::new("official").expect("provider should parse"),
        ),
        "https://example.invalid/go.json",
        "unix:100",
        60,
        "sha256:fixture",
        MetadataPayloadKind::Raw,
        "{}",
    );

    assert_eq!(entry.freshness_at("unix:159"), MetadataFreshness::Fresh);
    assert_eq!(entry.freshness_at("unix:161"), MetadataFreshness::Stale);
    assert_eq!(entry.freshness_at("not-a-time"), MetadataFreshness::Corrupt);
}
