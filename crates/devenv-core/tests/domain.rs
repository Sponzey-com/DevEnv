use std::str::FromStr;

use devenv_core::{
    Architecture, OperatingSystem, Platform, ToolName, ToolSpec, VersionRequirement,
};

#[test]
fn tool_name_normalizes_to_lowercase() {
    let name = ToolName::new("JaVa").expect("tool name should be valid");

    assert_eq!(name.as_str(), "java");
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
