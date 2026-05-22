use std::fs;

use devenv_adapters::fs::{
    NativeConfigRepository, discover_project_config, discover_project_config_from,
    write_devenv_toml_tool,
};
use devenv_core::{ConfigFormat, ConfigRepository, ConfigScope, ToolName, VersionRequirement};

#[test]
fn discovers_config_in_current_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join("devenv.toml");
    fs::write(
        &config_path,
        r#"
        [tools]
        go = "1.22.5"
        "#,
    )
    .expect("config should be written");

    let config = discover_project_config(root, root)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("go").expect("tool should be valid"))
            .expect("go should exist")
            .requirement()
            .raw(),
        "1.22.5"
    );
    let source = config.source().expect("source should be present");
    assert_eq!(
        source.path(),
        &config_path
            .canonicalize()
            .expect("config path should canonicalize")
    );
    assert_eq!(source.scope(), ConfigScope::Project);
    assert_eq!(source.format(), ConfigFormat::DevenvToml);
}

#[test]
fn discovers_parent_config_from_nested_child_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let child = root.join("a/b/c");
    fs::create_dir_all(&child).expect("child should be created");
    fs::write(
        root.join(".tool-versions"),
        r#"
        java 17
        go 1.22.5
        "#,
    )
    .expect("config should be written");

    let config = discover_project_config(root, &child)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("java").expect("tool should be valid"))
            .expect("java should exist")
            .requirement()
            .raw(),
        "17"
    );
    assert_eq!(
        config.source().expect("source should be present").format(),
        ConfigFormat::ToolVersions
    );
}

#[test]
fn discovers_node_version_file() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join(".node-version");
    fs::write(&config_path, "20.11.1\n").expect("config should be written");

    let config = discover_project_config(root, root)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("node").expect("tool should be valid"))
            .expect("node should exist")
            .requirement()
            .raw(),
        "20.11.1"
    );
    let source = config.source().expect("source should be present");
    assert_eq!(
        source.path(),
        &config_path
            .canonicalize()
            .expect("config path should canonicalize")
    );
    assert_eq!(source.format(), ConfigFormat::NodeVersion);
}

#[test]
fn discovers_nvmrc_file() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join(".nvmrc");
    fs::write(&config_path, "v20.11.1\n").expect("config should be written");

    let config = discover_project_config(root, root)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("node").expect("tool should be valid"))
            .expect("node should exist")
            .requirement()
            .raw(),
        "v20.11.1"
    );
    let source = config.source().expect("source should be present");
    assert_eq!(
        source.path(),
        &config_path
            .canonicalize()
            .expect("config path should canonicalize")
    );
    assert_eq!(source.format(), ConfigFormat::Nvmrc);
}

#[test]
fn discovers_python_version_file() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join(".python-version");
    fs::write(&config_path, "3.12.2\n").expect("config should be written");

    let config = discover_project_config(root, root)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("python").expect("tool should be valid"))
            .expect("python should exist")
            .requirement()
            .raw(),
        "3.12.2"
    );
    let source = config.source().expect("source should be present");
    assert_eq!(
        source.path(),
        &config_path
            .canonicalize()
            .expect("config path should canonicalize")
    );
    assert_eq!(source.format(), ConfigFormat::PythonVersion);
}

#[test]
fn discovers_ruby_version_file() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join(".ruby-version");
    fs::write(&config_path, "3.3.0\n").expect("config should be written");

    let config = discover_project_config(root, root)
        .expect("discovery should succeed")
        .expect("config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("ruby").expect("tool should be valid"))
            .expect("ruby should exist")
            .requirement()
            .raw(),
        "3.3.0"
    );
    let source = config.source().expect("source should be present");
    assert_eq!(
        source.path(),
        &config_path
            .canonicalize()
            .expect("config path should canonicalize")
    );
    assert_eq!(source.format(), ConfigFormat::RubyVersion);
}

#[test]
fn returns_none_when_no_project_config_exists_under_root() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let child = root.join("a/b/c");
    fs::create_dir_all(&child).expect("child should be created");

    let config = discover_project_config(root, &child).expect("discovery should succeed");

    assert!(config.is_none());
}

#[test]
fn invalid_config_error_includes_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let config_path = root.join("devenv.toml");
    fs::write(
        &config_path,
        r#"
        [tools
        go = "1.22.5"
        "#,
    )
    .expect("config should be written");

    let error = discover_project_config(root, root).expect_err("discovery should fail");
    let message = error.to_string();

    assert!(message.contains("failed to parse config"));
    assert!(message.contains(config_path.to_string_lossy().as_ref()));
    assert!(message.contains("invalid devenv.toml"));
}

#[test]
fn discovers_nearest_config_without_injected_root() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let root = temp.path();
    let child = root.join("child");
    fs::create_dir_all(&child).expect("child should be created");
    fs::write(root.join("devenv.toml"), "[tools]\njava = \"11\"\n")
        .expect("parent config should be written");
    fs::write(child.join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("child config should be written");

    let config = discover_project_config_from(&child)
        .expect("discovery should succeed")
        .expect("nearest config should be discovered");

    assert_eq!(
        config
            .tool(&ToolName::new("java").expect("tool should be valid"))
            .expect("java should exist")
            .requirement()
            .raw(),
        "17"
    );
    assert_eq!(
        config.source().expect("source should be present").path(),
        &child
            .join("devenv.toml")
            .canonicalize()
            .expect("child config should canonicalize")
    );
}

#[test]
fn writes_native_toml_tool_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let config_path = temp.path().join("devenv.toml");

    write_devenv_toml_tool(
        &config_path,
        ConfigScope::Project,
        ToolName::new("java").expect("tool should be valid"),
        VersionRequirement::exact("17").expect("requirement should be valid"),
    )
    .expect("selection should be written");

    let contents = fs::read_to_string(&config_path).expect("config should be readable");
    assert!(contents.contains("[tools]"));
    assert!(contents.contains("java = \"17\""));
}

#[test]
fn native_config_repository_reads_and_writes_requirements() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let config_path = temp.path().join("global/devenv.toml");
    let mut repository = NativeConfigRepository::new(&config_path, ConfigScope::Global);
    let tool = ToolName::new("go").expect("tool should be valid");

    repository
        .set_requirement(
            tool.clone(),
            VersionRequirement::exact("1.22.5").expect("requirement should be valid"),
        )
        .expect("requirement should be written");

    let requirement = repository
        .get_requirement(&tool)
        .expect("requirement read should succeed")
        .expect("requirement should exist");

    assert_eq!(requirement.raw(), "1.22.5");
}
