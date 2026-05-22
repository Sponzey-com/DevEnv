use std::path::PathBuf;

use devenv_core::{
    ConfigFormat, ConfigScope, ConfigSource, ProjectConfig, SelectionCandidate, SelectionSource,
    ToolName, VersionRequirement, parse_devenv_toml, parse_go_version, parse_java_version,
    parse_node_version, parse_nvmrc, parse_python_version, parse_ruby_version, parse_tool_versions,
    resolve_tool_selection,
};

#[test]
fn parse_native_config_with_inline_string_tool_value() {
    let config = parse_devenv_toml(
        r#"
        [tools]
        go = "1.22.5"
        "#,
    )
    .expect("config should parse");

    let go = config
        .tool(&ToolName::new("go").expect("tool should be valid"))
        .expect("go should be configured");

    assert_eq!(go.requirement().raw(), "1.22.5");
    assert!(go.distribution().is_none());
}

#[test]
fn parse_native_config_with_table_value_and_distribution() {
    let config = parse_devenv_toml(
        r#"
        [tools.java]
        version = "17"
        distribution = "temurin"
        "#,
    )
    .expect("config should parse");

    let java = config
        .tool(&ToolName::new("java").expect("tool should be valid"))
        .expect("java should be configured");

    assert_eq!(java.requirement().raw(), "17");
    assert_eq!(
        java.distribution()
            .expect("distribution should be present")
            .as_str(),
        "temurin"
    );
}

#[test]
fn parse_tool_versions_with_first_wave_tools() {
    let config = parse_tool_versions(
        r#"
        java 17
        go 1.22.5
        node 20.11.1
        python 3.12.2
        rust 1.85.0
        ruby 3.3.0
        php 8.3.7
        flutter 3.24.0
        terraform 1.8.5
        opentofu 1.8.5
        "#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("java").expect("tool should be valid"))
            .expect("java should exist")
            .requirement()
            .raw(),
        "17"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("go").expect("tool should be valid"))
            .expect("go should exist")
            .requirement()
            .raw(),
        "1.22.5"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("node").expect("tool should be valid"))
            .expect("node should exist")
            .requirement()
            .raw(),
        "20.11.1"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("python").expect("tool should be valid"))
            .expect("python should exist")
            .requirement()
            .raw(),
        "3.12.2"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("rust").expect("tool should be valid"))
            .expect("rust should exist")
            .requirement()
            .raw(),
        "1.85.0"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("ruby").expect("tool should be valid"))
            .expect("ruby should exist")
            .requirement()
            .raw(),
        "3.3.0"
    );
    assert_eq!(
        config
            .tool(&ToolName::new("php").expect("tool should be valid"))
            .expect("php should exist")
            .requirement()
            .raw(),
        "8.3.7"
    );
}

#[test]
fn parse_tool_versions_ignores_blank_lines_and_comments() {
    let config = parse_tool_versions(
        r#"
        # project runtimes

        java 17 # lts

        go 1.22.5
        "#,
    )
    .expect("config should parse");

    assert_eq!(config.tools().len(), 2);
}

#[test]
fn parse_java_version_file() {
    let config = parse_java_version("17\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("java").expect("tool should be valid"))
            .expect("java should exist")
            .requirement()
            .raw(),
        "17"
    );
}

#[test]
fn parse_go_version_file() {
    let config = parse_go_version("1.22.5\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("go").expect("tool should be valid"))
            .expect("go should exist")
            .requirement()
            .raw(),
        "1.22.5"
    );
}

#[test]
fn parse_node_version_file() {
    let config = parse_node_version("20.11.1\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("node").expect("tool should be valid"))
            .expect("node should exist")
            .requirement()
            .raw(),
        "20.11.1"
    );
}

#[test]
fn parse_nvmrc_file_preserves_nvm_style_requirement() {
    let config = parse_nvmrc("v20.11.1\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("node").expect("tool should be valid"))
            .expect("node should exist")
            .requirement()
            .raw(),
        "v20.11.1"
    );
}

#[test]
fn parse_python_version_file() {
    let config = parse_python_version("3.12.2\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("python").expect("tool should be valid"))
            .expect("python should exist")
            .requirement()
            .raw(),
        "3.12.2"
    );
}

#[test]
fn parse_ruby_version_file() {
    let config = parse_ruby_version("3.3.0\n").expect("config should parse");

    assert_eq!(
        config
            .tool(&ToolName::new("ruby").expect("tool should be valid"))
            .expect("ruby should exist")
            .requirement()
            .raw(),
        "3.3.0"
    );
}

#[test]
fn invalid_toml_returns_actionable_error() {
    let error = parse_devenv_toml(
        r#"
        [tools
        go = "1.22.5"
        "#,
    )
    .expect_err("config should fail");
    let message = error.to_string();

    assert!(message.contains("invalid devenv.toml"));
    assert!(message.contains("TOML parse error"));
}

#[test]
fn project_config_carries_source_metadata() {
    let source = ConfigSource::new(
        PathBuf::from("/workspace/devenv.toml"),
        ConfigScope::Project,
        ConfigFormat::DevenvToml,
    );

    let config = ProjectConfig::empty().with_source(source.clone());

    assert_eq!(config.source(), Some(&source));
}

#[test]
fn parser_only_reads_user_intent_without_validating_version_existence() {
    let config = parse_devenv_toml(
        r#"
        [tools]
        java = "not-a-real-version"
        "#,
    )
    .expect("config should parse");

    assert!(matches!(
        config
            .tool(&ToolName::new("java").expect("tool should be valid"))
            .expect("java should exist")
            .requirement(),
        VersionRequirement::Exact(_)
    ));
}

#[test]
fn project_config_renders_native_toml_for_writes() {
    let mut config = ProjectConfig::empty();
    config.set_tool_requirement(
        ToolName::new("java").expect("tool should be valid"),
        VersionRequirement::exact("17").expect("requirement should be valid"),
    );
    config.set_tool_requirement(
        ToolName::new("go").expect("tool should be valid"),
        VersionRequirement::exact("1.22.5").expect("requirement should be valid"),
    );

    let rendered = config.to_devenv_toml();

    assert!(rendered.contains("[tools]"));
    assert!(rendered.contains("go = \"1.22.5\""));
    assert!(rendered.contains("java = \"17\""));
}

#[test]
fn selection_precedence_prefers_cli_then_shell_then_project_then_global_then_default() {
    let tool = ToolName::new("java").expect("tool should be valid");
    let resolved = resolve_tool_selection(
        tool,
        [
            SelectionCandidate::new(
                SelectionSource::Default,
                VersionRequirement::exact("8").expect("requirement should be valid"),
            ),
            SelectionCandidate::new(
                SelectionSource::Global,
                VersionRequirement::exact("11").expect("requirement should be valid"),
            ),
            SelectionCandidate::new(
                SelectionSource::Project,
                VersionRequirement::exact("17").expect("requirement should be valid"),
            ),
            SelectionCandidate::new(
                SelectionSource::Shell,
                VersionRequirement::exact("21").expect("requirement should be valid"),
            ),
            SelectionCandidate::new(
                SelectionSource::CliOverride,
                VersionRequirement::exact("22").expect("requirement should be valid"),
            ),
        ],
    )
    .expect("selection should resolve");

    assert_eq!(resolved.source(), SelectionSource::CliOverride);
    assert_eq!(resolved.requirement().raw(), "22");
}

#[test]
fn selection_uses_project_when_no_cli_or_shell_candidate_exists() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let resolved = resolve_tool_selection(
        tool,
        [
            SelectionCandidate::new(
                SelectionSource::Global,
                VersionRequirement::exact("1.21.0").expect("requirement should be valid"),
            ),
            SelectionCandidate::new(
                SelectionSource::Project,
                VersionRequirement::exact("1.22.5").expect("requirement should be valid"),
            ),
        ],
    )
    .expect("selection should resolve");

    assert_eq!(resolved.source(), SelectionSource::Project);
    assert_eq!(resolved.requirement().raw(), "1.22.5");
}
