use devenv_core::{
    Architecture, ChecksumPolicy, OperatingSystem, Platform, ProviderId, ProviderSelectorDimension,
    ProviderSourceKind, SupportLevel, ToolName,
};
use devenv_tools::builtin_provider_registry;

#[test]
fn go_provider_capability_is_direct_with_required_checksums() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("go"), &provider("official"))
        .expect("Go official provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
    assert!(
        capability
            .platform_support()
            .supports(Platform::new(OperatingSystem::Macos, Architecture::Arm64))
    );
}

#[test]
fn java_temurin_provider_exposes_distribution_selector() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("java"), &provider("temurin"))
        .expect("Java Temurin provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::Distribution));
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::ImageType));
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::PackageType));
}

#[test]
fn node_provider_capability_is_direct_with_required_checksums() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("node"), &provider("official"))
        .expect("Node official provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
}

#[test]
fn flutter_provider_capability_exposes_stable_channel_selector() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("flutter"), &provider("stable"))
        .expect("Flutter stable provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::Channel));
}

#[test]
fn python_provider_capability_exposes_implementation_selector() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("python"), &provider("cpython"))
        .expect("Python CPython provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Direct);
    assert_eq!(capability.source_kind(), ProviderSourceKind::LocalFixture);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Required);
    assert!(capability.supports_selector_dimension(ProviderSelectorDimension::Implementation));
    assert!(
        capability
            .next_action()
            .expect("Python provider should explain deferred live support")
            .contains("0008-python-install-strategy")
    );
}

#[test]
fn rust_provider_capability_is_delegated_to_rustup() {
    let registry = builtin_provider_registry();
    let capability = registry
        .find(&tool("rust"), &provider("rustup"))
        .expect("Rust rustup provider should be registered");

    assert_eq!(capability.support_level(), SupportLevel::Delegated);
    assert_eq!(capability.checksum_policy(), ChecksumPolicy::Unavailable);
    assert!(
        capability
            .direct_install_unavailable_reason()
            .expect("delegated tool should explain direct install unavailability")
            .contains("rustup")
    );
}

#[test]
fn ruby_and_php_provider_capabilities_are_local_only() {
    let registry = builtin_provider_registry();

    for tool_name in ["ruby", "php"] {
        let capability = registry
            .find(&tool(tool_name), &provider("local"))
            .expect("local-only provider should be registered");

        assert_eq!(capability.support_level(), SupportLevel::LocalOnly);
        assert_eq!(capability.checksum_policy(), ChecksumPolicy::Unavailable);
        assert!(
            capability
                .direct_install_unavailable_reason()
                .expect("local-only tool should explain direct install unavailability")
                .contains("remote install is deferred")
        );
    }
}

fn tool(value: &str) -> ToolName {
    ToolName::new(value).expect("tool name should parse")
}

fn provider(value: &str) -> ProviderId {
    ProviderId::new(value).expect("provider id should parse")
}
