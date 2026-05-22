use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn version_command_prints_binary_name_and_version() {
    let mut cmd = Command::cargo_bin("devenv").expect("devenv binary should build");

    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("devenv 0.1.0"))
        .stdout(predicate::str::contains("target="))
        .stdout(predicate::str::contains("profile="))
        .stdout(predicate::str::contains("git="));
}

#[test]
fn help_command_prints_top_level_usage() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: devenv <command> [args]"))
        .stdout(predicate::str::contains("Commands:"))
        .stdout(predicate::str::contains("uninstall"))
        .stdout(predicate::str::contains("Supported tools:"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn help_subcommand_prints_command_usage() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Usage: devenv install <tool>@<version>",
        ))
        .stdout(predicate::str::contains("DevEnv-owned storage"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("help")
        .arg("uninstall")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Usage: devenv uninstall <tool>@<version>",
        ))
        .stdout(predicate::str::contains(
            "Deletes only DevEnv-owned installs",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn doctor_reports_ok_when_store_registry_and_shims_are_valid() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("shim")
        .arg("rehash")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("DevEnv doctor"))
        .stdout(predicate::str::contains("status: ok"))
        .stdout(predicate::str::contains("[ok] DEVENV_HOME"))
        .stdout(predicate::str::contains("[ok] install store"))
        .stdout(predicate::str::contains("[ok] runtime registry"))
        .stdout(predicate::str::contains("[ok] shim directory"))
        .stdout(predicate::str::contains("[ok] project config"));
}

#[test]
fn doctor_reports_actionable_warning_when_shim_directory_is_missing() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    fs::create_dir_all(devenv_home.join("installs")).expect("installs should be created");
    fs::create_dir_all(devenv_home.join("registry")).expect("registry should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("status: warning"))
        .stdout(predicate::str::contains("[warning] shim directory"))
        .stdout(predicate::str::contains("run `devenv shim init`"));
}

#[test]
fn doctor_json_output_is_parseable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    fs::create_dir_all(devenv_home.join("installs")).expect("installs should be created");
    fs::create_dir_all(devenv_home.join("registry")).expect("registry should be created");

    let output = Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("doctor")
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("doctor JSON should parse");

    assert_eq!(parsed["status"], "warning");
    assert!(parsed["checks"].as_array().is_some_and(|checks| {
        checks
            .iter()
            .any(|check| check["name"] == "shim directory" && check["status"] == "warning")
    }));
}

#[test]
fn unknown_command_returns_non_zero_and_actionable_error() {
    let mut cmd = Command::cargo_bin("devenv").expect("devenv binary should build");

    cmd.arg("unknown")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown command `unknown`"))
        .stderr(predicate::str::contains("try `devenv doctor`"));
}

#[test]
fn adr_records_the_binary_name_used_by_tests() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adr_path = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate should live under crates/devenv-cli")
        .join("docs/adr/0001-cli-name.md");

    let adr = std::fs::read_to_string(&adr_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", adr_path.display()));

    assert!(adr.contains("Chosen binary name: `devenv`"));
}

#[test]
fn local_command_writes_project_devenv_toml() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("local")
        .arg("java@17")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 local"));

    let contents =
        fs::read_to_string(temp.path().join("devenv.toml")).expect("config should be readable");
    assert!(contents.contains("[tools]"));
    assert!(contents.contains("java = \"17\""));
}

#[test]
fn global_command_writes_injected_global_config() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let global_config = temp.path().join("config/devenv.toml");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GLOBAL_CONFIG", &global_config)
        .arg("global")
        .arg("go@1.22.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 global"));

    let contents = fs::read_to_string(&global_config).expect("global config should be readable");
    assert!(contents.contains("[tools]"));
    assert!(contents.contains("go = \"1.22.5\""));
}

#[test]
fn shell_command_outputs_export_without_writing_files() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let global_config = temp.path().join("global/devenv.toml");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GLOBAL_CONFIG", &global_config)
        .arg("shell")
        .arg("java@17")
        .assert()
        .success()
        .stdout(predicate::str::contains("export DEVENV_TOOL_JAVA='17'"));

    assert!(!temp.path().join("devenv.toml").exists());
    assert!(!global_config.exists());
}

#[test]
fn use_without_scope_defaults_to_local() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("use")
        .arg("java@17")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 local"));

    let contents =
        fs::read_to_string(temp.path().join("devenv.toml")).expect("config should be readable");
    assert!(contents.contains("java = \"17\""));
}

#[test]
fn use_scope_global_writes_injected_global_config() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let global_config = temp.path().join("global/devenv.toml");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GLOBAL_CONFIG", &global_config)
        .arg("use")
        .arg("go@1.22.5")
        .arg("--scope")
        .arg("global")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 global"));

    let contents = fs::read_to_string(&global_config).expect("global config should be readable");
    assert!(contents.contains("go = \"1.22.5\""));
    assert!(!temp.path().join("devenv.toml").exists());
}

#[test]
fn current_reports_nearest_project_config_over_parent_config() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let parent = temp.path();
    let child = parent.join("child");
    let grandchild = child.join("grandchild");
    fs::create_dir_all(&grandchild).expect("grandchild should be created");
    fs::write(parent.join("devenv.toml"), "[tools]\njava = \"11\"\n")
        .expect("parent config should be written");
    fs::write(child.join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("child config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(&grandchild)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 project"))
        .stdout(predicate::str::contains(
            child.join("devenv.toml").to_string_lossy(),
        ));
}

#[test]
fn current_prefers_shell_over_project_and_global_config() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let global_config = temp.path().join("global/devenv.toml");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");
    fs::create_dir_all(
        global_config
            .parent()
            .expect("global config should have a parent"),
    )
    .expect("global config directory should be created");
    fs::write(&global_config, "[tools]\njava = \"11\"\n").expect("global config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GLOBAL_CONFIG", &global_config)
        .env("DEVENV_TOOL_JAVA", "21")
        .arg("current")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21 shell"));
}

#[test]
fn current_cli_override_wins_when_tool_spec_is_given() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_TOOL_JAVA", "21")
        .arg("current")
        .arg("java@22")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 22 cli"));
}

#[test]
fn current_reports_go_project_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join("devenv.toml"), "[tools]\ngo = \"1.22\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22 project"));
}

#[test]
fn node_version_file_selects_node_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join(".node-version"), "20\n")
        .expect("node version config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20 project"));
}

#[test]
fn nvmrc_file_selects_node_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join(".nvmrc"), "v20.11.1\n").expect("nvmrc config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node v20.11.1 project"));
}

#[test]
fn node_package_manager_pin_is_not_used_as_runtime_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(
        temp.path().join("package.json"),
        r#"{ "packageManager": "npm@10.2.4" }"#,
    )
    .expect("package.json should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("node")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no version selected for node"));
}

#[test]
fn python_version_file_selects_python_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join(".python-version"), "3.12\n")
        .expect("python version config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("python")
        .assert()
        .success()
        .stdout(predicate::str::contains("python 3.12 project"));
}

#[test]
fn ruby_version_file_selects_ruby_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join(".ruby-version"), "3.3\n")
        .expect("ruby version config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("ruby")
        .assert()
        .success()
        .stdout(predicate::str::contains("ruby 3.3 project"));
}

#[test]
fn python_project_environment_files_are_not_used_as_runtime_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(
        temp.path().join("pyproject.toml"),
        "[project]\nrequires-python = \">=3.12\"\n",
    )
    .expect("pyproject should be written");
    fs::write(temp.path().join("uv.lock"), "").expect("uv lock should be written");
    fs::create_dir_all(temp.path().join(".venv")).expect("venv directory should be created");
    fs::write(temp.path().join(".venv/pyvenv.cfg"), "home = /opt/python\n")
        .expect("venv marker should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("python")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no version selected for python"));
}

#[test]
fn tool_versions_file_selects_rust_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join(".tool-versions"), "rust 1.85\n")
        .expect("tool versions config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("current")
        .arg("rust")
        .assert()
        .success()
        .stdout(predicate::str::contains("rust 1.85 project"));
}

#[test]
fn missing_current_selection_reports_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env_remove("DEVENV_TOOL_JAVA")
        .arg("current")
        .arg("java")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no version selected for java"))
        .stderr(predicate::str::contains("devenv local java@<version>"));
}

#[test]
fn exec_runs_command_with_injected_fake_runtime_activation() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("jdk-17");
    fs::create_dir_all(runtime.join("bin")).expect("runtime bin should be created");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env("DEVENV_RUNTIME_JAVA_17", &runtime)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$JAVA_HOME\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(runtime.to_string_lossy()))
        .stdout(predicate::str::contains(
            runtime.join("bin").to_string_lossy(),
        ));
}

#[test]
fn exec_missing_runtime_reports_install_and_add_guidance() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env_remove("DEVENV_RUNTIME_JAVA_17")
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("true")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "java@17 is selected but not installed or registered",
        ))
        .stderr(predicate::str::contains("devenv add java <path>"))
        .stderr(predicate::str::contains("devenv install java@17"))
        .stderr(predicate::str::contains("devenv list java"));
}

#[test]
fn exec_requires_separator_and_command() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .arg("exec")
        .arg("java")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "usage: devenv exec -- <command> [args...]",
        ));
}

#[test]
fn add_java_records_external_jdk_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17.0.11", "17.0.11");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("java")
        .arg(&jdk)
        .assert()
        .success()
        .stdout(predicate::str::contains("added java 17.0.11"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"java\""));
    assert!(registry.contains("version = \"17.0.11\""));
    assert!(registry.contains(jdk.to_string_lossy().as_ref()));
}

#[test]
fn add_java_rejects_invalid_jdk_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_jdk = temp.path().join("not-a-jdk");
    fs::create_dir_all(&invalid_jdk).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("java")
        .arg(&invalid_jdk)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid Java runtime"))
        .stderr(predicate::str::contains("bin/java"));
}

#[test]
fn list_java_reports_discovered_candidate_jdk() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17.0.11", "17.0.11");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_JAVA_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17.0.11 discovered"))
        .stdout(predicate::str::contains(jdk.to_string_lossy()))
        .stdout(predicate::str::contains("distribution=unknown"));
}

#[test]
fn remove_java_unregisters_jdk_without_deleting_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17.0.11", "17.0.11");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("java")
        .arg(&jdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("remove")
        .arg("java")
        .arg(&jdk)
        .assert()
        .success()
        .stdout(predicate::str::contains("removed java 17.0.11"));

    assert!(jdk.exists());
    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(!registry.contains("version = \"17.0.11\""));
}

#[test]
fn exec_uses_registered_java_runtime_with_major_version_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17.0.11", "17.0.11");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("java")
        .arg(&jdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$JAVA_HOME\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(jdk.to_string_lossy()))
        .stdout(predicate::str::contains(jdk.join("bin").to_string_lossy()));
}

#[test]
fn add_go_records_external_sdk_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success()
        .stdout(predicate::str::contains("added go 1.22.5"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"go\""));
    assert!(registry.contains("version = \"1.22.5\""));
    assert!(registry.contains(sdk.to_string_lossy().as_ref()));
}

#[test]
fn add_flutter_records_external_sdk_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_flutter_sdk(temp.path(), "flutter-3.24.0", "Flutter 3.24.0");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("flutter")
        .arg(&sdk)
        .assert()
        .success()
        .stdout(predicate::str::contains("added flutter 3.24.0"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"flutter\""));
    assert!(registry.contains("version = \"3.24.0\""));
    assert!(registry.contains(sdk.to_string_lossy().as_ref()));
}

#[test]
fn add_terraform_records_external_single_binary_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("terraform")
        .arg(runtime.join("terraform"))
        .assert()
        .success()
        .stdout(predicate::str::contains("added terraform 1.8.5"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"terraform\""));
    assert!(registry.contains("version = \"1.8.5\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_opentofu_records_external_single_binary_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "opentofu-1.7.2", "tofu", "1.7.2");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("opentofu")
        .arg(&runtime)
        .assert()
        .success()
        .stdout(predicate::str::contains("added opentofu 1.7.2"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"opentofu\""));
    assert!(registry.contains("version = \"1.7.2\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_node_records_external_runtime_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_node_runtime(temp.path(), "node-v20.11.1", "v20.11.1");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("node")
        .arg(&runtime)
        .assert()
        .success()
        .stdout(predicate::str::contains("added node 20.11.1"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"node\""));
    assert!(registry.contains("version = \"20.11.1\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_python_records_external_runtime_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime =
        create_fake_python_runtime(temp.path(), "cpython-3.12.2", "Python 3.12.2", "cpython");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("python")
        .arg(&runtime)
        .assert()
        .success()
        .stdout(predicate::str::contains("added python 3.12.2"))
        .stdout(predicate::str::contains("implementation=cpython"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"python\""));
    assert!(registry.contains("version = \"3.12.2\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_ruby_records_external_runtime_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_ruby_runtime(temp.path(), "ruby-3.3.0", "ruby 3.3.0");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("ruby")
        .arg(&runtime)
        .assert()
        .success()
        .stdout(predicate::str::contains("added ruby 3.3.0"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"ruby\""));
    assert!(registry.contains("version = \"3.3.0\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_php_records_external_runtime_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_php_runtime(temp.path(), "php-8.3.7", "PHP 8.3.7");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("php")
        .arg(&runtime)
        .assert()
        .success()
        .stdout(predicate::str::contains("added php 8.3.7"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"php\""));
    assert!(registry.contains("version = \"8.3.7\""));
    assert!(registry.contains(runtime.to_string_lossy().as_ref()));
}

#[test]
fn add_rust_records_external_toolchain_in_devenv_home_registry() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain(temp.path(), "1.85.0-aarch64-apple-darwin", "rustc 1.85.0");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("rust")
        .arg(&toolchain)
        .assert()
        .success()
        .stdout(predicate::str::contains("added rust 1.85.0"));

    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(registry.contains("tool = \"rust\""));
    assert!(registry.contains("version = \"1.85.0\""));
    assert!(registry.contains(toolchain.to_string_lossy().as_ref()));
}

#[test]
fn add_rust_rejects_channel_style_rustup_toolchain_without_version_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain_without_version(temp.path(), "stable-aarch64-apple-darwin");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("rust")
        .arg(&toolchain)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported rustup toolchain"))
        .stderr(predicate::str::contains("VERSION"));
}

#[test]
fn add_go_rejects_invalid_sdk_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_sdk = temp.path().join("not-go");
    fs::create_dir_all(&invalid_sdk).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("go")
        .arg(&invalid_sdk)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid Go runtime"))
        .stderr(predicate::str::contains("bin/go"));
}

#[test]
fn add_flutter_rejects_invalid_sdk_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_sdk = temp.path().join("not-flutter");
    fs::create_dir_all(&invalid_sdk).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("flutter")
        .arg(&invalid_sdk)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid Flutter SDK"))
        .stderr(predicate::str::contains("bin/flutter"));
}

#[test]
fn add_terraform_rejects_invalid_single_binary_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_runtime = temp.path().join("not-terraform");
    fs::create_dir_all(&invalid_runtime).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("terraform")
        .arg(&invalid_runtime)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid Terraform runtime"))
        .stderr(predicate::str::contains("terraform"));
}

#[test]
fn add_ruby_rejects_invalid_runtime_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_runtime = temp.path().join("not-ruby");
    fs::create_dir_all(&invalid_runtime).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("ruby")
        .arg(&invalid_runtime)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid Ruby runtime"))
        .stderr(predicate::str::contains("bin/ruby"));
}

#[test]
fn add_php_rejects_invalid_runtime_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let invalid_runtime = temp.path().join("not-php");
    fs::create_dir_all(&invalid_runtime).expect("invalid directory should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("add")
        .arg("php")
        .arg(&invalid_runtime)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid PHP runtime"))
        .stderr(predicate::str::contains("bin/php"));
}

#[test]
fn list_go_reports_discovered_candidate_sdk() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_GO_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 discovered"))
        .stdout(predicate::str::contains(sdk.to_string_lossy()));
}

#[test]
fn list_flutter_reports_discovered_candidate_sdk() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_flutter_sdk(temp.path(), "flutter-3.24.0", "Flutter 3.24.0");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_FLUTTER_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("flutter")
        .assert()
        .success()
        .stdout(predicate::str::contains("flutter 3.24.0 discovered"))
        .stdout(predicate::str::contains(sdk.to_string_lossy()));
}

#[test]
fn list_terraform_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "terraform-1.8.5", "terraform", "1.8.5");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_TERRAFORM_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("terraform")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5 discovered"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_opentofu_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_iac_runtime(temp.path(), "opentofu-1.7.2", "tofu", "1.7.2");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_OPENTOFU_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("opentofu")
        .assert()
        .success()
        .stdout(predicate::str::contains("opentofu 1.7.2 discovered"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_node_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_node_runtime(temp.path(), "node-v20.11.1", "v20.11.1");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_NODE_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1 discovered"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_python_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime =
        create_fake_python_runtime(temp.path(), "cpython-3.12.2", "Python 3.12.2", "cpython");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_PYTHON_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("python")
        .assert()
        .success()
        .stdout(predicate::str::contains("python 3.12.2 discovered"))
        .stdout(predicate::str::contains("implementation=cpython"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_ruby_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_ruby_runtime(temp.path(), "ruby-3.3.0", "ruby 3.3.0");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_RUBY_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("ruby")
        .assert()
        .success()
        .stdout(predicate::str::contains("ruby 3.3.0 discovered"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_php_reports_discovered_candidate_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_php_runtime(temp.path(), "php-8.3.7", "PHP 8.3.7");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_PHP_CANDIDATE_PATHS", temp.path())
        .arg("list")
        .arg("php")
        .assert()
        .success()
        .stdout(predicate::str::contains("php 8.3.7 discovered"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()));
}

#[test]
fn list_rust_reports_discovered_candidate_toolchain() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain(temp.path(), "1.85.0-aarch64-apple-darwin", "rustc 1.85.0");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_RUST_CANDIDATE_PATHS", temp.path())
        .env_remove("RUSTUP_HOME")
        .arg("list")
        .arg("rust")
        .assert()
        .success()
        .stdout(predicate::str::contains("rust 1.85.0 discovered"))
        .stdout(predicate::str::contains(toolchain.to_string_lossy()));
}

#[test]
fn list_rust_reports_rustup_home_toolchains_without_running_rustup() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let rustup_home = temp.path().join("rustup-home");
    let toolchain = create_fake_rust_toolchain(
        &rustup_home.join("toolchains"),
        "1.85.0-aarch64-apple-darwin",
        "rustc 1.85.0",
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("RUSTUP_HOME", &rustup_home)
        .arg("list")
        .arg("rust")
        .assert()
        .success()
        .stdout(predicate::str::contains("rust 1.85.0 rustup"))
        .stdout(predicate::str::contains(toolchain.to_string_lossy()));
}

#[test]
fn list_remote_go_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5"))
        .stdout(predicate::str::contains("go1.22.5").not());
}

#[test]
fn list_remote_go_reports_missing_metadata_source() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .arg("list-remote")
        .arg("go")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Go remote metadata is not configured",
        ))
        .stderr(predicate::str::contains("DEVENV_GO_RELEASE_METADATA"));
}

#[test]
fn list_remote_go_resolves_relative_metadata_path_from_current_dir() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);
    let metadata_file = metadata
        .file_name()
        .and_then(|name| name.to_str())
        .expect("metadata fixture should have utf-8 file name");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GO_RELEASE_METADATA", metadata_file)
        .arg("list-remote")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5"));
}

#[test]
fn list_remote_go_reports_unreadable_metadata_path() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GO_RELEASE_METADATA", "missing-go-releases.toml")
        .arg("list-remote")
        .arg("go")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "failed to read Go release metadata",
        ))
        .stderr(predicate::str::contains("missing-go-releases.toml"));
}

#[test]
fn list_remote_go_preserves_parser_context_for_invalid_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(
        temp.path().join("bad-go-releases.toml"),
        "not valid toml = [",
    )
    .expect("bad metadata should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GO_RELEASE_METADATA", "bad-go-releases.toml")
        .arg("list-remote")
        .arg("go")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "failed to parse Go release metadata fixture",
        ));
}

#[test]
fn list_remote_flutter_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_flutter_archive(temp.path());
    let metadata = write_flutter_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_FLUTTER_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("flutter")
        .assert()
        .success()
        .stdout(predicate::str::contains("flutter 3.24.0 stable"));
}

#[test]
fn list_remote_terraform_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let binary = write_fake_iac_binary(temp.path(), "terraform.fixture");
    let metadata = write_iac_release_metadata_fixture(temp.path(), &binary, "terraform");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_TERRAFORM_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("terraform")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5"));
}

#[test]
fn list_remote_opentofu_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let binary = write_fake_iac_binary(temp.path(), "tofu.fixture");
    let metadata = write_iac_release_metadata_fixture(temp.path(), &binary, "tofu");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_OPENTOFU_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("opentofu")
        .assert()
        .success()
        .stdout(predicate::str::contains("opentofu 1.8.5"));
}

#[test]
fn list_remote_node_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_node_archive(temp.path());
    let metadata = write_node_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_NODE_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1"))
        .stdout(predicate::str::contains("node v20.11.1").not());
}

#[test]
fn list_remote_python_uses_fixture_release_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_python_archive(temp.path());
    let metadata = write_python_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_PYTHON_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("python")
        .assert()
        .success()
        .stdout(predicate::str::contains("python 3.12.2 cpython"))
        .stdout(predicate::str::contains("pypy").not());
}

#[test]
fn list_remote_java_uses_fixture_temurin_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_JAVA_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 temurin"))
        .stdout(predicate::str::contains("java 17.0.11 temurin"));
}

#[test]
fn shim_rehash_generates_all_builtin_tool_shims() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("shim")
        .arg("rehash")
        .assert()
        .success()
        .stdout(predicate::str::contains("generated 25 shims"));

    for binary in [
        "java",
        "javac",
        "jar",
        "javadoc",
        "go",
        "gofmt",
        "flutter",
        "dart",
        "terraform",
        "tofu",
        "node",
        "npm",
        "npx",
        "corepack",
        "python",
        "python3",
        "pip",
        "ruby",
        "gem",
        "bundle",
        "php",
        "phpize",
        "php-config",
        "rustc",
        "cargo",
    ] {
        let shim = devenv_home.join("shims").join(binary);
        assert!(shim.is_file(), "shim should exist: {}", shim.display());
        let contents = fs::read_to_string(&shim).expect("shim should be readable");
        assert!(contents.contains("devenv"));
        assert!(contents.contains("shim dispatch"));
        assert!(contents.contains(binary));
    }
}

#[test]
fn shim_init_creates_shims_without_mutating_shell_profiles() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let profile = temp.path().join(".zshrc");
    fs::write(&profile, "existing profile\n").expect("profile should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("HOME", temp.path())
        .arg("shim")
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized shims"))
        .stdout(predicate::str::contains("shims: 25"));

    assert_eq!(
        fs::read_to_string(&profile).expect("profile should be readable"),
        "existing profile\n"
    );
}

#[test]
fn shim_dispatch_resolves_current_directory_config() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = temp.path().join("runtime");
    fs::create_dir_all(runtime.join("bin")).expect("runtime bin should be created");
    write_executable(
        &runtime.join("bin/java"),
        "#!/bin/sh\nprintf 'shim-java:%s:%s' \"$JAVA_HOME\" \"$1\"\n",
    );
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env("DEVENV_RUNTIME_JAVA_17", &runtime)
        .arg("shim")
        .arg("dispatch")
        .arg("java")
        .arg("--")
        .arg("hello")
        .assert()
        .success()
        .stdout(predicate::str::contains("shim-java:"))
        .stdout(predicate::str::contains(runtime.to_string_lossy()))
        .stdout(predicate::str::contains(":hello"));
}

#[test]
fn shim_dispatch_detects_recursion() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env("DEVENV_RUNTIME_JAVA_17", temp.path().join("runtime"))
        .env("DEVENV_ACTIVE_SHIM", "java")
        .arg("shim")
        .arg("dispatch")
        .arg("java")
        .arg("--")
        .arg("-version")
        .assert()
        .failure()
        .stderr(predicate::str::contains("shim recursion detected"));
}

#[test]
fn activate_renders_shell_scripts_without_writing_profiles() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let profile = temp.path().join(".bashrc");
    fs::write(&profile, "keep\n").expect("profile should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("HOME", temp.path())
        .arg("activate")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("export DEVENV_HOME="))
        .stdout(predicate::str::contains("shims"))
        .stdout(predicate::str::contains("PATH"));

    assert_eq!(
        fs::read_to_string(&profile).expect("profile should be readable"),
        "keep\n"
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("activate")
        .arg("zsh")
        .assert()
        .success()
        .stdout(predicate::str::contains("export DEVENV_HOME="))
        .stdout(predicate::str::contains("shims"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("activate")
        .arg("fish")
        .assert()
        .success()
        .stdout(predicate::str::contains("set -gx DEVENV_HOME"))
        .stdout(predicate::str::contains("set -gx PATH"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("activate")
        .arg("powershell")
        .assert()
        .success()
        .stdout(predicate::str::contains("$env:DEVENV_HOME"))
        .stdout(predicate::str::contains("$env:PATH"));
}

#[test]
fn activate_unsupported_shell_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("activate")
        .arg("tcsh")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported shell `tcsh`"))
        .stderr(predicate::str::contains("zsh, bash, fish, or powershell"));
}

#[test]
fn remove_go_unregisters_sdk_without_deleting_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("remove")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success()
        .stdout(predicate::str::contains("removed go 1.22.5"));

    assert!(sdk.exists());
    let registry = fs::read_to_string(devenv_home.join("registry/external-runtimes.toml"))
        .expect("registry should be readable");
    assert!(!registry.contains("version = \"1.22.5\""));
}

#[test]
fn exec_uses_registered_go_runtime_with_minor_version_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\ngo = \"1.22\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$GOROOT\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(sdk.to_string_lossy()))
        .stdout(predicate::str::contains(sdk.join("bin").to_string_lossy()));
}

#[test]
fn exec_uses_registered_rust_toolchain_with_minor_version_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let toolchain =
        create_fake_rust_toolchain(temp.path(), "1.85.0-aarch64-apple-darwin", "rustc 1.85.0");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\nrust = \"1.85\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("rust")
        .arg(&toolchain)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("RUSTUP_HOME")
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            toolchain.join("bin").to_string_lossy(),
        ));
}

#[test]
fn exec_uses_registered_ruby_runtime_with_minor_version_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_ruby_runtime(temp.path(), "ruby-3.3.0", "ruby 3.3.0");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\nruby = \"3.3\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("ruby")
        .arg(&runtime)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            runtime.join("bin").to_string_lossy(),
        ));
}

#[test]
fn exec_uses_registered_php_runtime_with_minor_version_selection() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let runtime = create_fake_php_runtime(temp.path(), "php-8.3.7", "PHP 8.3.7");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\nphp = \"8.3\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("php")
        .arg(&runtime)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            runtime.join("bin").to_string_lossy(),
        ));
}

#[test]
fn exec_activates_registered_java_and_go_from_the_same_selection_flow() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let jdk = create_fake_jdk(temp.path(), "jdk-17.0.11", "17.0.11");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\njava = \"17\"\ngo = \"1.22\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("java")
        .arg(&jdk)
        .assert()
        .success();
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s|%s' \"$JAVA_HOME\" \"$GOROOT\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains(jdk.to_string_lossy()))
        .stdout(predicate::str::contains(sdk.to_string_lossy()))
        .stdout(predicate::str::contains(jdk.join("bin").to_string_lossy()))
        .stdout(predicate::str::contains(sdk.join("bin").to_string_lossy()));
}

#[test]
fn install_go_from_fixture_archive_lists_and_activates_installed_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\ngo = \"1.22.5\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("go@1.22.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed go 1.22.5"))
        .stdout(predicate::str::contains("installs/go/1.22.5"));

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/go/1.22.5")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("requested_spec = \"go@1.22.5\""));
    assert!(install_metadata.contains("resolved_version = \"1.22.5\""));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 installed"))
        .stdout(predicate::str::contains("installs/go/1.22.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$GOROOT\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/go/1.22.5"))
        .stdout(predicate::str::contains("bin"));
}

#[test]
fn uninstall_removes_devenv_owned_install_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("go@1.22.5")
        .assert()
        .success();

    let install_root = find_single_install_root(&devenv_home, "go", "1.22.5");
    assert!(install_root.join("devenv-install.toml").is_file());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("uninstall")
        .arg("go@1.22")
        .assert()
        .success()
        .stdout(predicate::str::contains("uninstalled go 1.22.5"))
        .stdout(predicate::str::contains(install_root.to_string_lossy()));

    assert!(!install_root.exists());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 installed").not());
}

#[test]
fn uninstall_does_not_remove_external_registered_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let sdk = create_fake_go_sdk(temp.path(), "go-1.22.5", "go1.22.5");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("add")
        .arg("go")
        .arg(&sdk)
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("uninstall")
        .arg("go@1.22.5")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed by DevEnv"))
        .stderr(predicate::str::contains("devenv remove go <path>"));

    assert!(sdk.exists());
}

#[test]
fn install_flutter_from_fixture_archive_lists_and_activates_installed_sdk() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_flutter_archive(temp.path());
    let metadata = write_flutter_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\nflutter = \"3.24\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_FLUTTER_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("flutter@3.24")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed flutter 3.24.0"))
        .stdout(predicate::str::contains("installs/flutter/3.24.0"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("flutter")
        .assert()
        .success()
        .stdout(predicate::str::contains("flutter 3.24.0 installed"))
        .stdout(predicate::str::contains("installs/flutter/3.24.0"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$FLUTTER_ROOT\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/flutter/3.24.0"))
        .stdout(predicate::str::contains("bin"));
}

#[test]
fn install_terraform_from_fixture_binary_uses_single_binary_pipeline() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let binary = write_fake_iac_binary(temp.path(), "terraform.fixture");
    let metadata = write_iac_release_metadata_fixture(temp.path(), &binary, "terraform");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\nterraform = \"1.8\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_TERRAFORM_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("terraform@1.8")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed terraform 1.8.5"))
        .stdout(predicate::str::contains("installs/terraform/1.8.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("terraform")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5 installed"))
        .stdout(predicate::str::contains("installs/terraform/1.8.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/terraform/1.8.5"));
}

#[test]
fn install_opentofu_from_fixture_binary_uses_single_binary_pipeline() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let binary = write_fake_iac_binary(temp.path(), "tofu.fixture");
    let metadata = write_iac_release_metadata_fixture(temp.path(), &binary, "tofu");
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\nopentofu = \"1.8\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_OPENTOFU_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("opentofu@1.8")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed opentofu 1.8.5"))
        .stdout(predicate::str::contains("installs/opentofu/1.8.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("opentofu")
        .assert()
        .success()
        .stdout(predicate::str::contains("opentofu 1.8.5 installed"))
        .stdout(predicate::str::contains("installs/opentofu/1.8.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/opentofu/1.8.5"));
}

#[test]
fn install_node_from_fixture_archive_lists_and_activates_installed_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_node_archive(temp.path());
    let metadata = write_node_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\nnode = \"20\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_NODE_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("node@20")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed node 20.11.1"))
        .stdout(predicate::str::contains("installs/node/20.11.1"));

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/node/20.11.1")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("tool = \"node\""));
    assert!(install_metadata.contains("version = \"20.11.1\""));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1 installed"))
        .stdout(predicate::str::contains("installs/node/20.11.1"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/node/20.11.1"))
        .stdout(predicate::str::contains("bin"));
}

#[test]
fn install_python_from_fixture_archive_lists_and_activates_installed_runtime() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_python_archive(temp.path());
    let metadata = write_python_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\npython = \"3.12\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_PYTHON_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("python@3.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed python 3.12.2"))
        .stdout(predicate::str::contains("implementation=cpython"))
        .stdout(predicate::str::contains("installs/python/3.12.2"));

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/python/3.12.2")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("tool = \"python\""));
    assert!(install_metadata.contains("version = \"3.12.2\""));
    assert!(install_metadata.contains("implementation = \"cpython\""));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("python")
        .assert()
        .success()
        .stdout(predicate::str::contains("python 3.12.2 installed"))
        .stdout(predicate::str::contains("implementation=cpython"))
        .stdout(predicate::str::contains("installs/python/3.12.2"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s' \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/python/3.12.2"))
        .stdout(predicate::str::contains("bin"));
}

#[test]
fn install_go_accepts_official_top_level_go_archive_layout() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_nested_go_archive(temp.path());
    let metadata = write_nested_go_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(
        temp.path().join("devenv.toml"),
        "[tools]\ngo = \"1.22.5\"\n",
    )
    .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("go@1.22.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed go 1.22.5"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 installed"))
        .stdout(predicate::str::contains("installs/go/1.22.5"))
        .stdout(predicate::str::contains("/go"));
}

#[test]
fn install_java_from_fixture_archive_lists_and_activates_installed_jdk() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");
    fs::write(temp.path().join("devenv.toml"), "[tools]\njava = \"17\"\n")
        .expect("project config should be written");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_RELEASE_METADATA", &metadata)
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("install")
        .arg("java@17")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed java 17.0.11-temurin"))
        .stdout(predicate::str::contains("installs/java/17.0.11-temurin"))
        .stdout(predicate::str::contains("distribution=temurin"));

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/java/17.0.11-temurin")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("distribution = \"temurin\""));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("list")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17.0.11-temurin installed"))
        .stdout(predicate::str::contains("installs/java/17.0.11-temurin"))
        .stdout(predicate::str::contains("distribution=temurin"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("exec")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("printf '%s|%s' \"$JAVA_HOME\" \"$PATH\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("installs/java/17.0.11-temurin"))
        .stdout(predicate::str::contains("bin"));
}

fn create_fake_jdk(parent: &Path, name: &str, version: &str) -> PathBuf {
    let jdk = parent.join(name);
    fs::create_dir_all(jdk.join("bin")).expect("JDK bin should be created");
    fs::write(jdk.join("bin/java"), "").expect("java binary should be written");
    fs::write(jdk.join("bin/javac"), "").expect("javac binary should be written");
    fs::write(jdk.join("release"), format!("JAVA_VERSION=\"{version}\"\n"))
        .expect("release metadata should be written");
    jdk
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("executable should be written");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .expect("executable metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("executable permissions should be set");
    }
}

fn create_fake_go_sdk(parent: &Path, name: &str, version: &str) -> PathBuf {
    let sdk = parent.join(name);
    fs::create_dir_all(sdk.join("bin")).expect("Go SDK bin should be created");
    fs::write(sdk.join("bin/go"), "").expect("go binary should be written");
    fs::write(sdk.join("bin/gofmt"), "").expect("gofmt binary should be written");
    fs::write(sdk.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    sdk
}

fn create_fake_flutter_sdk(parent: &Path, name: &str, version: &str) -> PathBuf {
    let sdk = parent.join(name);
    fs::create_dir_all(sdk.join("bin")).expect("Flutter bin should be created");
    fs::write(sdk.join("bin/flutter"), "").expect("flutter binary should be written");
    fs::write(sdk.join("bin/dart"), "").expect("dart binary should be written");
    fs::write(sdk.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    sdk
}

fn create_fake_iac_runtime(parent: &Path, name: &str, binary_name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(&runtime).expect("IaC runtime dir should be created");
    fs::write(runtime.join(binary_name), "").expect("IaC binary should be written");
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn create_fake_node_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Node.js bin should be created");
    for binary in ["node", "npm", "npx", "corepack"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn create_fake_python_runtime(
    parent: &Path,
    name: &str,
    version: &str,
    implementation: &str,
) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Python bin should be created");
    for binary in ["python", "python3", "pip"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    fs::write(
        runtime.join("IMPLEMENTATION"),
        format!("{implementation}\n"),
    )
    .expect("implementation metadata should be written");
    runtime
}

fn create_fake_ruby_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("Ruby bin should be created");
    for binary in ["ruby", "gem", "bundle"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn create_fake_php_runtime(parent: &Path, name: &str, version: &str) -> PathBuf {
    let runtime = parent.join(name);
    fs::create_dir_all(runtime.join("bin")).expect("PHP bin should be created");
    for binary in ["php", "phpize", "php-config"] {
        fs::write(runtime.join("bin").join(binary), "").expect("binary should be written");
    }
    fs::write(runtime.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    runtime
}

fn create_fake_rust_toolchain(parent: &Path, name: &str, version: &str) -> PathBuf {
    let toolchain = create_fake_rust_toolchain_without_version(parent, name);
    fs::write(toolchain.join("VERSION"), format!("{version}\n"))
        .expect("version metadata should be written");
    toolchain
}

fn create_fake_rust_toolchain_without_version(parent: &Path, name: &str) -> PathBuf {
    let toolchain = parent.join(name);
    fs::create_dir_all(toolchain.join("bin")).expect("Rust bin should be created");
    fs::write(toolchain.join("bin/rustc"), "").expect("rustc should be written");
    fs::write(toolchain.join("bin/cargo"), "").expect("cargo should be written");
    toolchain
}

fn write_fake_go_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("go1.22.5.fixture.archive");
    fs::write(&archive, "VERSION\tgo1.22.5\nbin/go\nbin/gofmt\n")
        .expect("archive should be written");
    archive
}

fn write_fake_flutter_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("flutter-3.24.0.fixture.archive");
    fs::write(
        &archive,
        "flutter/VERSION\tFlutter 3.24.0\nflutter/bin/flutter\nflutter/bin/dart\n",
    )
    .expect("archive should be written");
    archive
}

fn write_fake_iac_binary(parent: &Path, filename: &str) -> PathBuf {
    let binary = parent.join(filename);
    fs::write(&binary, "binary").expect("binary artifact should be written");
    binary
}

fn write_fake_nested_go_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("go1.22.5.nested.fixture.archive");
    fs::write(&archive, "go/VERSION\tgo1.22.5\ngo/bin/go\ngo/bin/gofmt\n")
        .expect("archive should be written");
    archive
}

fn write_fake_node_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("node-v20.11.1.fixture.archive");
    fs::write(
        &archive,
        "node-v20.11.1/VERSION\tv20.11.1\nnode-v20.11.1/bin/node\nnode-v20.11.1/bin/npm\nnode-v20.11.1/bin/npx\nnode-v20.11.1/bin/corepack\n",
    )
    .expect("archive should be written");
    archive
}

fn write_fake_python_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("cpython-3.12.2.fixture.archive");
    fs::write(
        &archive,
        "cpython-3.12.2/VERSION\tPython 3.12.2\ncpython-3.12.2/IMPLEMENTATION\tcpython\ncpython-3.12.2/bin/python\ncpython-3.12.2/bin/python3\ncpython-3.12.2/bin/pip\n",
    )
    .expect("archive should be written");
    archive
}

fn write_fake_java_archive(parent: &Path) -> PathBuf {
    let archive = parent.join("OpenJDK17U.fixture.archive");
    fs::write(
        &archive,
        "release\tJAVA_VERSION=\"17.0.11\"\nbin/java\nbin/javac\n",
    )
    .expect("archive should be written");
    archive
}

fn write_go_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    write_go_release_metadata_fixture_with_artifact(
        parent,
        archive,
        "d74a2ec88c1dad4f8baf6385452a7d65889cd95121c057a77ac743b54a40b40a",
        34,
        "go-releases.toml",
    )
}

fn write_nested_go_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    write_go_release_metadata_fixture_with_artifact(
        parent,
        archive,
        "a04ea9c8a40a86c043551351eec1d2e0bae7852b13d02e9257e2f816ed639185",
        43,
        "go-releases-nested.toml",
    )
}

fn write_go_release_metadata_fixture_with_artifact(
    parent: &Path,
    archive: &Path,
    sha256: &str,
    size: u64,
    filename: &str,
) -> PathBuf {
    let metadata = parent.join(filename);
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
            r#"
[[release]]
version = "go1.22.5"
stable = true

[[release.file]]
filename = "go1.22.5.darwin-arm64.tar.gz"
os = "darwin"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "{sha256}"
size = {size}

[[release.file]]
filename = "go1.22.5.darwin-amd64.tar.gz"
os = "darwin"
arch = "amd64"
kind = "archive"
url = "{url}"
sha256 = "{sha256}"
size = {size}

[[release.file]]
filename = "go1.22.5.linux-amd64.tar.gz"
os = "linux"
arch = "amd64"
kind = "archive"
url = "{url}"
sha256 = "{sha256}"
size = {size}

[[release.file]]
filename = "go1.22.5.windows-amd64.zip"
os = "windows"
arch = "amd64"
kind = "archive"
url = "{url}"
sha256 = "{sha256}"
size = {size}
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn write_flutter_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("flutter-releases.toml");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
            r#"
[[release]]
version = "Flutter 3.24.0"
channel = "stable"
stable = true

[[release.file]]
filename = "flutter_macos_arm64_3.24.0-stable.zip"
os = "macos"
arch = "arm64"
kind = "archive"
url = "{url}"

[[release.file]]
filename = "flutter_macos_x64_3.24.0-stable.zip"
os = "macos"
arch = "x64"
kind = "archive"
url = "{url}"

[[release.file]]
filename = "flutter_linux_x64_3.24.0-stable.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
url = "{url}"

[[release.file]]
filename = "flutter_linux_arm64_3.24.0-stable.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
url = "{url}"

[[release.file]]
filename = "flutter_windows_x64_3.24.0-stable.zip"
os = "windows"
arch = "x64"
kind = "archive"
url = "{url}"

[[release.file]]
filename = "flutter_windows_arm64_3.24.0-stable.zip"
os = "windows"
arch = "arm64"
kind = "archive"
url = "{url}"
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn write_iac_release_metadata_fixture(parent: &Path, binary: &Path, binary_name: &str) -> PathBuf {
    let metadata = parent.join(format!("{binary_name}-releases.toml"));
    let url = binary
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
            r#"
[[release]]
version = "v1.8.5"
stable = true

[[release.file]]
filename = "{binary_name}_1.8.5_darwin_arm64"
os = "darwin"
arch = "arm64"
kind = "binary"
url = "{url}"

[[release.file]]
filename = "{binary_name}_1.8.5_darwin_amd64"
os = "darwin"
arch = "amd64"
kind = "binary"
url = "{url}"

[[release.file]]
filename = "{binary_name}_1.8.5_linux_amd64"
os = "linux"
arch = "amd64"
kind = "binary"
url = "{url}"

[[release.file]]
filename = "{binary_name}_1.8.5_linux_arm64"
os = "linux"
arch = "arm64"
kind = "binary"
url = "{url}"

[[release.file]]
filename = "{binary_name}_1.8.5_windows_amd64.exe"
os = "windows"
arch = "amd64"
kind = "binary"
url = "{url}"

[[release.file]]
filename = "{binary_name}_1.8.5_windows_arm64.exe"
os = "windows"
arch = "arm64"
kind = "binary"
url = "{url}"
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn write_node_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("node-releases.toml");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
            r#"
[[release]]
version = "v20.11.1"
stable = true

[[release.file]]
filename = "node-v20.11.1-darwin-arm64.tar.gz"
os = "darwin"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125

[[release.file]]
filename = "node-v20.11.1-darwin-x64.tar.gz"
os = "darwin"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125

[[release.file]]
filename = "node-v20.11.1-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125

[[release.file]]
filename = "node-v20.11.1-linux-arm64.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125

[[release.file]]
filename = "node-v20.11.1-win-x64.zip"
os = "win"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125

[[release.file]]
filename = "node-v20.11.1-win-arm64.zip"
os = "win"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "169c8ee6987374c542ffefcace8f7651979748c72c75302b0701ead46a820ac5"
size = 125
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn write_python_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("python-releases.toml");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
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
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release.file]]
filename = "cpython-3.12.2-macos-x64.tar.gz"
os = "macos"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release.file]]
filename = "cpython-3.12.2-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release.file]]
filename = "cpython-3.12.2-linux-arm64.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release.file]]
filename = "cpython-3.12.2-windows-x64.zip"
os = "windows"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release.file]]
filename = "cpython-3.12.2-windows-arm64.zip"
os = "windows"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151

[[release]]
version = "3.10.14"
implementation = "pypy"
stable = true

[[release.file]]
filename = "pypy3.10-v7.3.15-linux-x64.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "3b9fe9f24224889fc0d3c1cf62299b0a12d32b54a4de5142ae050c32c3841066"
size = 151
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn write_java_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("java-releases.toml");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        &metadata,
        format!(
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
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50

[[release.file]]
filename = "OpenJDK17U-jdk_x64_mac_hotspot_17.0.11_9.tar.gz"
os = "macos"
arch = "x64"
kind = "jdk"
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50

[[release.file]]
filename = "OpenJDK17U-jdk_aarch64_linux_hotspot_17.0.11_9.tar.gz"
os = "linux"
arch = "arm64"
kind = "jdk"
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50

[[release.file]]
filename = "OpenJDK17U-jdk_x64_linux_hotspot_17.0.11_9.tar.gz"
os = "linux"
arch = "x64"
kind = "jdk"
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50

[[release.file]]
filename = "OpenJDK17U-jdk_aarch64_windows_hotspot_17.0.11_9.zip"
os = "windows"
arch = "arm64"
kind = "jdk"
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50

[[release.file]]
filename = "OpenJDK17U-jdk_x64_windows_hotspot_17.0.11_9.zip"
os = "windows"
arch = "x64"
kind = "jdk"
url = "{url}"
sha256 = "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068"
size = 50
"#
        ),
    )
    .expect("release metadata should be written");
    metadata
}

fn find_single_install_root(devenv_home: &Path, tool: &str, version: &str) -> PathBuf {
    let version_dir = devenv_home.join("installs").join(tool).join(version);
    let entries = fs::read_dir(&version_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", version_dir.display()))
        .map(|entry| entry.expect("install entry should be readable").path())
        .filter(|path| path.join("devenv-install.toml").is_file())
        .collect::<Vec<_>>();

    assert_eq!(entries.len(), 1, "expected exactly one install root");
    entries[0].clone()
}

fn current_platform_id_for_test() -> &'static str {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    match (os, arch) {
        ("macos", "arm64") => "macos-arm64",
        ("macos", "x64") => "macos-x64",
        ("linux", "arm64") => "linux-arm64",
        ("linux", "x64") => "linux-x64",
        ("windows", "arm64") => "windows-arm64",
        ("windows", "x64") => "windows-x64",
        _ => unreachable!("test platform should be mapped"),
    }
}
