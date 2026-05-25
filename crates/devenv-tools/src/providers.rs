use devenv_core::{
    Architecture, ChecksumPolicy, OperatingSystem, Platform, ProviderCapability, ProviderId,
    ProviderRegistry, ProviderSelectorDimension, ProviderSourceKind, SupportLevel, ToolName,
};

pub fn builtin_provider_registry() -> ProviderRegistry {
    ProviderRegistry::new(builtin_provider_capabilities())
}

pub fn builtin_provider_capabilities() -> Vec<ProviderCapability> {
    vec![
        direct_provider(
            "go",
            "official",
            "Go official releases",
            ProviderSourceKind::OfficialApi,
        )
        .with_supported_platforms(common_platforms()),
        direct_provider(
            "java",
            "temurin",
            "Eclipse Temurin via Adoptium",
            ProviderSourceKind::OfficialApi,
        )
        .with_selector_dimension(ProviderSelectorDimension::Distribution)
        .with_selector_dimension(ProviderSelectorDimension::ImageType)
        .with_selector_dimension(ProviderSelectorDimension::PackageType)
        .with_supported_platforms(common_platforms()),
        direct_provider(
            "node",
            "official",
            "Node.js official releases",
            ProviderSourceKind::StaticIndex,
        )
        .with_supported_platforms(common_platforms()),
        direct_provider(
            "python",
            "cpython",
            "CPython fixture-backed metadata",
            ProviderSourceKind::LocalFixture,
        )
        .with_selector_dimension(ProviderSelectorDimension::Implementation)
        .with_supported_platforms(common_platforms())
        .with_next_action(
            "Live CPython direct provider is deferred; use DEVENV_PYTHON_RELEASE_METADATA fixture metadata or `devenv add python <path>`. See docs/adr/0008-python-install-strategy.md.",
        ),
        direct_provider(
            "flutter",
            "stable",
            "Flutter stable channel",
            ProviderSourceKind::StaticIndex,
        )
        .with_selector_dimension(ProviderSelectorDimension::Channel)
        .with_supported_platforms(common_platforms()),
        direct_provider(
            "terraform",
            "hashicorp",
            "HashiCorp Terraform releases",
            ProviderSourceKind::StaticIndex,
        )
        .with_supported_platforms(common_platforms()),
        direct_provider(
            "opentofu",
            "opentofu",
            "OpenTofu official releases",
            ProviderSourceKind::StaticIndex,
        )
        .with_supported_platforms(common_platforms()),
        ProviderCapability::new(
            tool("rust"),
            provider("rustup"),
            "Rust delegated to rustup",
            SupportLevel::Delegated,
            ProviderSourceKind::DelegatedCommand,
            ChecksumPolicy::Unavailable,
        )
        .with_unavailable_reason("Rust installation is delegated to rustup")
        .with_next_action(
            "Install or update Rust with rustup, then let DevEnv discover RUSTUP_HOME/toolchains or register a toolchain with `devenv add rust <path>`.",
        ),
        ProviderCapability::new(
            tool("ruby"),
            provider("local"),
            "Ruby local registration",
            SupportLevel::LocalOnly,
            ProviderSourceKind::LocalFixture,
            ChecksumPolicy::Unavailable,
        )
        .with_unavailable_reason("Ruby remote install is deferred")
        .with_next_action("Register an existing Ruby runtime with `devenv add ruby <path>`."),
        ProviderCapability::new(
            tool("php"),
            provider("local"),
            "PHP local registration",
            SupportLevel::LocalOnly,
            ProviderSourceKind::LocalFixture,
            ChecksumPolicy::Unavailable,
        )
        .with_unavailable_reason("PHP remote install is deferred")
        .with_next_action("Register an existing PHP runtime with `devenv add php <path>`."),
    ]
}

fn direct_provider(
    tool_name: &str,
    provider_id: &str,
    display_name: &str,
    source_kind: ProviderSourceKind,
) -> ProviderCapability {
    ProviderCapability::new(
        tool(tool_name),
        provider(provider_id),
        display_name,
        SupportLevel::Direct,
        source_kind,
        ChecksumPolicy::Required,
    )
}

fn tool(value: &str) -> ToolName {
    ToolName::new(value).expect("built-in tool name should be valid")
}

fn provider(value: &str) -> ProviderId {
    ProviderId::new(value).expect("built-in provider id should be valid")
}

fn common_platforms() -> Vec<Platform> {
    [
        OperatingSystem::Macos,
        OperatingSystem::Linux,
        OperatingSystem::Windows,
    ]
    .into_iter()
    .flat_map(|os| {
        [Architecture::X64, Architecture::Arm64]
            .into_iter()
            .map(move |architecture| Platform::new(os, architecture))
    })
    .collect()
}
