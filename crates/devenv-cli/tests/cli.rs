use assert_cmd::Command;
use devenv_adapters::checksum::hex_sha256;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

#[test]
fn version_command_prints_binary_name_and_version() {
    let mut cmd = Command::cargo_bin("devenv").expect("devenv binary should build");
    let expected_version = format!("devenv {}", env!("CARGO_PKG_VERSION"));

    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected_version))
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
            "Usage: devenv install <tool> <version>",
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
            "Usage: devenv uninstall <tool> <version>",
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
fn python_install_strategy_adr_records_deferred_live_provider() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adr_path = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate should live under crates/devenv-cli")
        .join("docs/adr/0008-python-install-strategy.md");

    let adr = std::fs::read_to_string(&adr_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", adr_path.display()));

    assert!(adr.contains("CPython source build"));
    assert!(adr.contains("python-build"));
    assert!(adr.contains("uv managed Python"));
    assert!(adr.contains("LocalOnly"));
    assert!(adr.contains("defer live Python Direct install"));
}

#[test]
fn local_command_writes_project_devenv_toml() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env("SHELL", "/bin/zsh")
        .arg("local")
        .arg("java")
        .arg("17")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 local"))
        .stdout(predicate::str::contains("activate zsh"))
        .stdout(predicate::str::contains("new sessions"))
        .stdout(predicate::str::contains("~/.zshrc"));

    let contents =
        fs::read_to_string(temp.path().join("devenv.toml")).expect("config should be readable");
    assert!(contents.contains("[tools]"));
    assert!(contents.contains("java = \"17\""));
}

#[test]
fn local_command_refreshes_shims_when_activation_path_is_present() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let shim_dir = devenv_home.join("shims");
    fs::create_dir_all(&shim_dir).expect("shim dir should be created");
    let path = std::env::join_paths(std::iter::once(shim_dir.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("PATH should join");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("PATH", path)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("local")
        .arg("java")
        .arg("17")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 local"))
        .stdout(predicate::str::contains("devenv activate").not());

    assert!(
        shim_dir.join("java").is_file(),
        "active sessions should refresh shims when local selection changes"
    );
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
        .arg("go")
        .arg("1.22.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 global"));

    let contents = fs::read_to_string(&global_config).expect("global config should be readable");
    assert!(contents.contains("[tools]"));
    assert!(contents.contains("go = \"1.22.5\""));
}

#[test]
fn global_command_writes_default_global_config_when_env_is_not_set() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let global_config = devenv_home.join("devenv.toml");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("global")
        .arg("go")
        .arg("1.22.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5 global"))
        .stdout(predicate::str::contains(global_config.to_string_lossy()));

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
        .arg("java")
        .arg("17")
        .assert()
        .success()
        .stdout(predicate::str::contains("export DEVENV_TOOL_JAVA='17'"));

    assert!(!temp.path().join("devenv.toml").exists());
    assert!(!global_config.exists());
}

#[test]
fn use_without_scope_defaults_to_local() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .arg("use")
        .arg("java")
        .arg("17")
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
        .arg("go")
        .arg("1.22.5")
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
        .arg("java")
        .arg("22")
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
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env_remove("DEVENV_TOOL_JAVA")
        .arg("current")
        .arg("java")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no version selected for java"))
        .stderr(predicate::str::contains("devenv local java <version>"));
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
            "java 17 is selected but not installed or registered",
        ))
        .stderr(predicate::str::contains("devenv add java <path>"))
        .stderr(predicate::str::contains("devenv install java 17"))
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
fn list_remote_go_fixture_override_wins_over_official_fixture() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);
    let official = write_go_official_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .env("DEVENV_GO_OFFICIAL_RELEASE_METADATA", &official)
        .arg("list-remote")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5"))
        .stdout(predicate::str::contains("go 1.23.4").not());
}

#[test]
fn list_remote_go_refresh_writes_official_cache_and_offline_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let official = write_go_official_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_OFFICIAL_RELEASE_METADATA", &official)
        .arg("list-remote")
        .arg("go")
        .arg("--refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.23.4"))
        .stdout(predicate::str::contains("go1.23.4").not())
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
        .arg("list-remote")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.23.4"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
        .arg("list-remote")
        .arg("go")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.23.4"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn list_remote_go_reports_missing_metadata_source() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
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
fn list_remote_flutter_rejects_unsupported_channel() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("list-remote")
        .arg("flutter")
        .arg("--channel")
        .arg("beta")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported channel `beta`"))
        .stderr(predicate::str::contains("supported channels: stable"))
        .stderr(predicate::str::contains("not implemented yet"))
        .stderr(predicate::str::contains("devenv provider info flutter"));
}

#[test]
fn list_remote_flutter_refresh_writes_official_http_cache_and_offline_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_flutter_official_metadata();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_FLUTTER_OFFICIAL_BASE_URL", &base_url)
        .arg("list-remote")
        .arg("flutter")
        .arg("--refresh")
        .arg("--channel")
        .arg("stable")
        .assert()
        .success()
        .stdout(predicate::str::contains("flutter 3.24.0 stable"))
        .stdout(predicate::str::contains("3.25.0").not())
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_FLUTTER_OFFICIAL_BASE_URL")
        .arg("list-remote")
        .arg("flutter")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("flutter 3.24.0 stable"))
        .stderr(predicate::str::is_empty());
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
fn list_remote_terraform_refresh_writes_official_http_cache_and_offline_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_iac_official_metadata(IacFixtureTool::Terraform);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_TERRAFORM_OFFICIAL_BASE_URL", &base_url)
        .arg("list-remote")
        .arg("terraform")
        .arg("--refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_TERRAFORM_OFFICIAL_BASE_URL")
        .arg("list-remote")
        .arg("terraform")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn list_remote_opentofu_refresh_writes_official_http_cache_and_offline_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_iac_official_metadata(IacFixtureTool::OpenTofu);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_OPENTOFU_OFFICIAL_BASE_URL", &base_url)
        .arg("list-remote")
        .arg("opentofu")
        .arg("--refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("opentofu 1.8.5"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_OPENTOFU_OFFICIAL_BASE_URL")
        .arg("list-remote")
        .arg("opentofu")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("opentofu 1.8.5"))
        .stderr(predicate::str::is_empty());
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
fn list_remote_node_fixture_override_wins_over_official_fixture() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_node_archive(temp.path());
    let metadata = write_node_release_metadata_fixture(temp.path(), &archive);
    let official_index = write_node_official_index_fixture(temp.path());
    let shasums_dir = write_node_official_shasums_fixtures(temp.path());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_NODE_RELEASE_METADATA", &metadata)
        .env("DEVENV_NODE_OFFICIAL_RELEASE_INDEX", &official_index)
        .env("DEVENV_NODE_OFFICIAL_SHASUMS_DIR", &shasums_dir)
        .arg("list-remote")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1"))
        .stdout(predicate::str::contains("node 21.2.0").not());
}

#[test]
fn list_remote_node_refresh_writes_official_http_cache_and_offline_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_node_official_metadata();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_NODE_OFFICIAL_BASE_URL", &base_url)
        .arg("list-remote")
        .arg("node")
        .arg("--refresh")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 21.2.0"))
        .stdout(predicate::str::contains("node 20.11.1"))
        .stdout(predicate::str::contains("node v20.11.1").not())
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_NODE_OFFICIAL_BASE_URL")
        .arg("list-remote")
        .arg("node")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 21.2.0"))
        .stdout(predicate::str::contains("node 20.11.1"))
        .stderr(predicate::str::is_empty());
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
fn list_remote_uses_manifest_known_versions_for_seeded_tools() {
    for (tool, suffix, manifest) in [
        (
            "python",
            "cpython",
            include_str!("../../../metadata/providers/python/cpython/manifest.json"),
        ),
        (
            "rust",
            "rustup",
            include_str!("../../../metadata/providers/rust/rustup/manifest.json"),
        ),
        (
            "ruby",
            "local",
            include_str!("../../../metadata/providers/ruby/local/manifest.json"),
        ),
        (
            "php",
            "local",
            include_str!("../../../metadata/providers/php/local/manifest.json"),
        ),
    ] {
        let expected = format!("{tool} {} {suffix}", first_known_manifest_version(manifest));
        Command::cargo_bin("devenv")
            .expect("devenv binary should build")
            .arg("list-remote")
            .arg(tool)
            .assert()
            .success()
            .stdout(predicate::str::contains(expected))
            .stderr(predicate::str::is_empty());
    }
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
        .arg("--distribution")
        .arg("temurin")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 17 temurin"))
        .stdout(predicate::str::contains("java 17.0.11 temurin"));
}

#[test]
fn list_remote_java_uses_temurin_api_fixture_metadata() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_temurin_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_RELEASE_METADATA", &metadata)
        .arg("list-remote")
        .arg("java")
        .arg("--distribution")
        .arg("temurin")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21 temurin"))
        .stdout(predicate::str::contains("java 21.0.2 temurin"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn list_remote_java_reads_provider_manifest_metadata_when_cache_is_missing() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_java_temurin_manifest_metadata();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_API_BASE_URL", base_url)
        .arg("list-remote")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21 temurin"))
        .stdout(predicate::str::contains("java 21.0.2 temurin"))
        .stdout(predicate::str::contains("java 17 temurin"))
        .stdout(predicate::str::contains("java 17.0.11 temurin"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list-remote")
        .arg("java")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21.0.2 temurin"))
        .stdout(predicate::str::contains("java 17.0.11 temurin"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn list_remote_java_uses_manifest_known_features_when_release_index_is_missing() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let base_url = serve_java_temurin_manifest_feature_metadata_without_available_index();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env("DEVENV_JAVA_TEMURIN_API_BASE_URL", base_url)
        .arg("list-remote")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21.0.2 temurin"))
        .stdout(predicate::str::contains("java 17.0.11 temurin"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn list_remote_java_rejects_unsupported_distribution() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("list-remote")
        .arg("java")
        .arg("--distribution")
        .arg("zulu")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported distribution `zulu`"))
        .stderr(predicate::str::contains("supported distributions: temurin"))
        .stderr(predicate::str::contains("not implemented yet"))
        .stderr(predicate::str::contains("devenv provider info java"));
}

#[test]
fn metadata_help_command_prints_usage() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("metadata")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Usage: devenv metadata <status|update|verify-catalog> [args]",
        ))
        .stdout(predicate::str::contains("status [tool]"))
        .stdout(predicate::str::contains("update <tool|--all>"))
        .stdout(predicate::str::contains("verify-catalog [tool]"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn catalog_diagnostics_metadata_help_mentions_verify_catalog() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("metadata")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "verify-catalog [tool] [--catalog <path-or-url>] [--source catalog|file]",
        ))
        .stdout(predicate::str::contains(
            "devenv metadata verify-catalog go --catalog ./v1 --source file",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_status_reports_all_provider_cache_status() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("metadata")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Metadata status"))
        .stdout(predicate::str::contains("go official support=Direct"))
        .stdout(predicate::str::contains("rust rustup support=Delegated"))
        .stdout(predicate::str::contains("ruby local support=LocalOnly"))
        .stdout(predicate::str::contains("cache=missing"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_status_go_reports_go_provider_and_cache_state() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("metadata")
        .arg("status")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official support=Direct"))
        .stdout(predicate::str::contains("source=official-api"))
        .stdout(predicate::str::contains("checksum=required"))
        .stdout(predicate::str::contains("cache=missing"))
        .stdout(predicate::str::contains("metadata_source=missing"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_status_python_reports_deferred_live_provider_strategy() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("metadata")
        .arg("status")
        .arg("python")
        .assert()
        .success()
        .stdout(predicate::str::contains("python cpython support=Direct"))
        .stdout(predicate::str::contains("source=local-fixture"))
        .stdout(predicate::str::contains("cache=missing"))
        .stdout(predicate::str::contains(
            "Live CPython direct provider is deferred",
        ))
        .stdout(predicate::str::contains(
            "docs/adr/0008-python-install-strategy.md",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_update_go_writes_fixture_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive(temp.path());
    let metadata = write_go_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_RELEASE_METADATA", &metadata)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official updated cache=fresh"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("metadata")
        .arg("status")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official support=Direct"))
        .stdout(predicate::str::contains("cache=fresh"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_update_go_writes_official_fixture_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let official = write_go_official_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_OFFICIAL_RELEASE_METADATA", &official)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official updated cache=fresh"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
        .arg("list-remote")
        .arg("go")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.23.4"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn go_catalog_metadata_update_writes_cache_and_status_digests() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official updated cache=fresh"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("metadata")
        .arg("status")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("go official support=Direct"))
        .stdout(predicate::str::contains("cache=fresh"))
        .stdout(predicate::str::contains("metadata_source=catalog"))
        .stdout(predicate::str::contains("cache_source=catalog"))
        .stdout(predicate::str::contains("catalog_version=2026.05.22.1"))
        .stdout(predicate::str::contains(format!(
            "manifest_sha256={}",
            catalog.manifest_sha256
        )))
        .stdout(predicate::str::contains(format!(
            "payload_sha256={}",
            catalog.payload_sha256
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn go_catalog_list_remote_offline_reads_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list-remote")
        .arg("go")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.23.4"))
        .stdout(predicate::str::contains("go 1.23.5").not())
        .stderr(predicate::str::is_empty());
}

#[test]
fn go_catalog_install_dry_run_resolves_minor_from_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("go@1.23")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("requested go@1.23"))
        .stdout(predicate::str::contains("resolved 1.23.4"))
        .stdout(predicate::str::contains("provider official"))
        .stdout(predicate::str::contains(archive.to_string_lossy()))
        .stderr(predicate::str::is_empty());
}

#[test]
fn go_catalog_env_fixture_override_wins_over_catalog_source() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let fixture_archive = write_fake_go_archive(temp.path());
    let fixture_metadata = write_go_release_metadata_fixture(temp.path(), &fixture_archive);
    let catalog_archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &catalog_archive, false);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_RELEASE_METADATA", &fixture_metadata)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .arg("list-remote")
        .arg("go")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("go 1.22.5"))
        .stdout(predicate::str::contains("go 1.23.4").not())
        .stderr(predicate::str::is_empty());
}

#[test]
fn go_catalog_payload_checksum_mismatch_blocks_cache_write() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, true);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .arg("--source")
        .arg("catalog")
        .assert()
        .failure()
        .stderr(predicate::str::contains("catalog trust failure"))
        .stderr(predicate::str::contains("checksum mismatch"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("metadata")
        .arg("status")
        .arg("go")
        .assert()
        .success()
        .stdout(predicate::str::contains("cache=missing"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn catalog_diagnostics_verify_catalog_local_file_succeeds() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .arg("metadata")
        .arg("verify-catalog")
        .arg("go")
        .arg("--catalog")
        .arg(&catalog.root_path)
        .arg("--source")
        .arg("file")
        .assert()
        .success()
        .stdout(predicate::str::contains("Catalog verification"))
        .stdout(predicate::str::contains("source file"))
        .stdout(predicate::str::contains("catalog_version 2026.05.22.1"))
        .stdout(predicate::str::contains(format!(
            "manifest_sha256 {}",
            catalog.manifest_sha256
        )))
        .stdout(predicate::str::contains(
            "entry go official path=tools/go/official/releases.json",
        ))
        .stdout(predicate::str::contains("status=verified"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn catalog_diagnostics_unavailable_error_has_next_action() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env_remove("DEVENV_CATALOG_BASE_URL")
        .arg("metadata")
        .arg("verify-catalog")
        .arg("go")
        .assert()
        .failure()
        .stderr(predicate::str::contains("catalog unavailable"))
        .stderr(predicate::str::contains("DEVENV_CATALOG_BASE_URL"))
        .stderr(predicate::str::contains("--catalog <path-or-url>"))
        .stderr(predicate::str::contains("next:"));
}

#[test]
fn catalog_diagnostics_signature_failure_error_has_next_action() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);
    fs::write(catalog.root_path.join("manifest.sig"), "bad-signature")
        .expect("signature should be overwritten");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .arg("metadata")
        .arg("verify-catalog")
        .arg("go")
        .arg("--catalog")
        .arg(&catalog.root_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("catalog trust failure"))
        .stderr(predicate::str::contains("catalog signature mismatch"))
        .stderr(predicate::str::contains("next:"))
        .stderr(predicate::str::contains(
            "do not ignore catalog trust failures",
        ));
}

#[test]
fn catalog_diagnostics_expired_catalog_error_has_next_action() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let catalog = write_go_catalog_fixture(temp.path(), &archive, false);
    expire_catalog_fixture(&catalog);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .arg("metadata")
        .arg("verify-catalog")
        .arg("go")
        .arg("--catalog")
        .arg(&catalog.root_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("catalog trust failure"))
        .stderr(predicate::str::contains("expired at"))
        .stderr(predicate::str::contains("next:"));
}

#[test]
fn metadata_update_java_writes_temurin_fixture_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_java_archive(temp.path());
    let temurin = write_java_temurin_release_metadata_fixture(temp.path(), &archive);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_RELEASE_METADATA", &temurin)
        .arg("metadata")
        .arg("update")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("java temurin updated cache=fresh"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_JAVA_TEMURIN_RELEASE_METADATA")
        .arg("list-remote")
        .arg("java")
        .arg("--distribution")
        .arg("temurin")
        .assert()
        .success()
        .stdout(predicate::str::contains("java 21.0.2 temurin"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_update_node_writes_official_fixture_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let official_index = write_node_official_index_fixture(temp.path());
    let shasums_dir = write_node_official_shasums_fixtures(temp.path());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_NODE_OFFICIAL_RELEASE_INDEX", &official_index)
        .env("DEVENV_NODE_OFFICIAL_SHASUMS_DIR", &shasums_dir)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "node official updated cache=fresh",
        ))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_NODE_OFFICIAL_RELEASE_INDEX")
        .env_remove("DEVENV_NODE_OFFICIAL_SHASUMS_DIR")
        .arg("list-remote")
        .arg("node")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn node_catalog_metadata_update_writes_cache_and_status_digests() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(temp.path(), &archive, NodeCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "node official updated cache=fresh",
        ))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("metadata")
        .arg("status")
        .arg("node")
        .assert()
        .success()
        .stdout(predicate::str::contains("node official support=Direct"))
        .stdout(predicate::str::contains("cache=fresh"))
        .stdout(predicate::str::contains("cache_source=catalog"))
        .stdout(predicate::str::contains("catalog_version=2026.05.22.1"))
        .stdout(predicate::str::contains(format!(
            "manifest_sha256={}",
            catalog.manifest_sha256
        )))
        .stdout(predicate::str::contains(format!(
            "payload_sha256={}",
            catalog.payload_sha256
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn node_catalog_list_remote_offline_reads_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(temp.path(), &archive, NodeCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list-remote")
        .arg("node")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.12.0"))
        .stdout(predicate::str::contains("node 20.11.1"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn node_catalog_install_dry_run_resolves_major_from_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(temp.path(), &archive, NodeCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("node@20")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("requested node@20"))
        .stdout(predicate::str::contains("resolved 20.12.0"))
        .stdout(predicate::str::contains("provider official"))
        .stdout(predicate::str::contains(archive.to_string_lossy()))
        .stderr(predicate::str::is_empty());
}

#[test]
fn node_catalog_missing_checksum_is_not_installable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(
        temp.path(),
        &archive,
        NodeCatalogFixtureMode::MissingCurrentChecksum,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("node@20.12.0")
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not provide an archive"));
}

#[test]
fn node_catalog_env_fixture_override_wins_over_catalog_source() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let fixture_archive = write_fake_node_archive(temp.path());
    let fixture_metadata = write_node_release_metadata_fixture(temp.path(), &fixture_archive);
    let catalog_archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(
        temp.path(),
        &catalog_archive,
        NodeCatalogFixtureMode::Normal,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_NODE_RELEASE_METADATA", &fixture_metadata)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_NODE_RELEASE_METADATA")
        .arg("list-remote")
        .arg("node")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("node 20.11.1"))
        .stdout(predicate::str::contains("node 20.12.0").not())
        .stderr(predicate::str::is_empty());
}

#[test]
fn node_catalog_unsupported_platform_error_is_actionable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let archive = write_fake_node_archive(temp.path());
    let catalog = write_node_catalog_fixture(
        temp.path(),
        &archive,
        NodeCatalogFixtureMode::UnsupportedCurrentPlatform,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("node")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("node@20.12.0")
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not provide an archive"))
        .stderr(predicate::str::contains(current_platform_id_for_test()));
}

#[test]
fn iac_catalog_metadata_update_writes_cache_and_status_digests() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog =
        write_terraform_catalog_fixture(temp.path(), &binary, TerraformCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "terraform hashicorp updated cache=fresh",
        ))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("metadata")
        .arg("status")
        .arg("terraform")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "terraform hashicorp support=Direct",
        ))
        .stdout(predicate::str::contains("cache=fresh"))
        .stdout(predicate::str::contains("cache_source=catalog"))
        .stdout(predicate::str::contains("catalog_version=2026.05.22.1"))
        .stdout(predicate::str::contains(format!(
            "manifest_sha256={}",
            catalog.manifest_sha256
        )))
        .stdout(predicate::str::contains(format!(
            "payload_sha256={}",
            catalog.payload_sha256
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn iac_catalog_list_remote_offline_reads_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog =
        write_terraform_catalog_fixture(temp.path(), &binary, TerraformCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("list-remote")
        .arg("terraform")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.6"))
        .stdout(predicate::str::contains("terraform 1.8.5"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn iac_catalog_install_dry_run_resolves_single_binary_from_catalog_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog =
        write_terraform_catalog_fixture(temp.path(), &binary, TerraformCatalogFixtureMode::Normal);

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("terraform@1.8")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("requested terraform@1.8"))
        .stdout(predicate::str::contains("resolved 1.8.6"))
        .stdout(predicate::str::contains("provider hashicorp"))
        .stdout(predicate::str::contains(binary.to_string_lossy()))
        .stdout(predicate::str::contains("checksum sha256:"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn iac_catalog_missing_checksum_is_not_installable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog = write_terraform_catalog_fixture(
        temp.path(),
        &binary,
        TerraformCatalogFixtureMode::MissingCurrentChecksum,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("terraform@1.8.6")
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not provide a binary"));
}

#[test]
fn iac_catalog_unsupported_platform_error_is_actionable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog = write_terraform_catalog_fixture(
        temp.path(),
        &binary,
        TerraformCatalogFixtureMode::UnsupportedCurrentPlatform,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("install")
        .arg("terraform@1.8.6")
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not provide a binary"))
        .stderr(predicate::str::contains(current_platform_id_for_test()));
}

#[test]
fn iac_catalog_env_fixture_override_wins_over_catalog_source() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let fixture_binary = write_fake_iac_binary(temp.path(), "terraform.fixture");
    let fixture_metadata =
        write_iac_release_metadata_fixture(temp.path(), &fixture_binary, "terraform");
    let catalog_binary = write_fake_iac_binary(temp.path(), "terraform.catalog.fixture");
    let catalog = write_terraform_catalog_fixture(
        temp.path(),
        &catalog_binary,
        TerraformCatalogFixtureMode::Normal,
    );

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_TERRAFORM_RELEASE_METADATA", &fixture_metadata)
        .env("DEVENV_CATALOG_BASE_URL", &catalog.root_url)
        .arg("metadata")
        .arg("update")
        .arg("terraform")
        .arg("--source")
        .arg("catalog")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_TERRAFORM_RELEASE_METADATA")
        .arg("list-remote")
        .arg("terraform")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform 1.8.5"))
        .stdout(predicate::str::contains("terraform 1.8.6").not())
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_update_all_skips_missing_and_local_only_sources() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
        .arg("metadata")
        .arg("update")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "go official skipped support=Direct reason=\"missing DEVENV_GO_RELEASE_METADATA",
        ))
        .stdout(predicate::str::contains(
            "ruby local skipped support=LocalOnly",
        ))
        .stdout(predicate::str::contains("devenv add ruby <path>"))
        .stdout(predicate::str::contains(
            "php local skipped support=LocalOnly",
        ))
        .stdout(predicate::str::contains("devenv add php <path>"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn metadata_update_ruby_explains_local_only_support() {
    let temp = tempfile::tempdir().expect("tempdir should be created");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", temp.path().join("devenv-home"))
        .arg("metadata")
        .arg("update")
        .arg("ruby")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "metadata update is not supported for ruby",
        ))
        .stderr(predicate::str::contains("LocalOnly"))
        .stderr(predicate::str::contains("devenv add ruby <path>"));
}

#[test]
fn install_ruby_reports_local_only_next_action() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("install")
        .arg("ruby@3.3")
        .assert()
        .failure()
        .stderr(predicate::str::contains("support=LocalOnly"))
        .stderr(predicate::str::contains("Ruby remote install is deferred"))
        .stderr(predicate::str::contains("devenv add ruby <path>"));
}

#[test]
fn install_php_reports_local_only_next_action() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("install")
        .arg("php@8.3")
        .assert()
        .failure()
        .stderr(predicate::str::contains("support=LocalOnly"))
        .stderr(predicate::str::contains("PHP remote install is deferred"))
        .stderr(predicate::str::contains("devenv add php <path>"));
}

#[test]
fn install_rust_reports_rustup_delegation() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("install")
        .arg("rust@1.85")
        .assert()
        .failure()
        .stderr(predicate::str::contains("support=Delegated"))
        .stderr(predicate::str::contains("rustup"))
        .stderr(predicate::str::contains("RUSTUP_HOME"));
}

#[test]
fn provider_list_reports_support_levels() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Providers"))
        .stdout(predicate::str::contains("go official support=Direct"))
        .stdout(predicate::str::contains("rust rustup support=Delegated"))
        .stdout(predicate::str::contains("ruby local support=LocalOnly"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn provider_info_java_reports_selectors_and_checksum_policy() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("info")
        .arg("java")
        .assert()
        .success()
        .stdout(predicate::str::contains("Provider info for java"))
        .stdout(predicate::str::contains("provider temurin"))
        .stdout(predicate::str::contains("checksum required"))
        .stdout(predicate::str::contains(
            "selectors distribution,image-type,package-type",
        ))
        .stderr(predicate::str::is_empty());
}

#[test]
fn catalog_diagnostics_provider_info_go_reports_catalog_availability() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("info")
        .arg("go")
        .arg("official")
        .assert()
        .success()
        .stdout(predicate::str::contains("Provider info for go"))
        .stdout(predicate::str::contains("provider official"))
        .stdout(predicate::str::contains("support Direct"))
        .stdout(predicate::str::contains("catalog_availability available"))
        .stdout(predicate::str::contains("catalog_status experimental"))
        .stdout(predicate::str::contains("DEVENV_CATALOG_BASE_URL"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn provider_info_rust_reports_rustup_delegation() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("info")
        .arg("rust")
        .assert()
        .success()
        .stdout(predicate::str::contains("Provider info for rust"))
        .stdout(predicate::str::contains("support Delegated"))
        .stdout(predicate::str::contains(
            "Rust installation is delegated to rustup",
        ))
        .stdout(predicate::str::contains("next_action"))
        .stdout(predicate::str::contains("RUSTUP_HOME"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn provider_info_unknown_provider_id_is_actionable() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("info")
        .arg("java")
        .arg("zulu")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown provider `zulu` for java"))
        .stderr(predicate::str::contains("supported providers: temurin"))
        .stderr(predicate::str::contains("devenv provider info java"));
}

#[test]
fn provider_info_unknown_tool_is_actionable() {
    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .arg("provider")
        .arg("info")
        .arg("zig")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown tool `zig` for provider metadata",
        ))
        .stderr(predicate::str::contains("supported provider tools:"))
        .stderr(predicate::str::contains("devenv provider list"));
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

    let node_shim =
        fs::read_to_string(devenv_home.join("shims/node")).expect("node shim should be readable");
    assert!(node_shim.contains("shim dispatch"));
    assert!(
        !node_shim.contains("exec 'devenv' shim dispatch"),
        "shims must call the native executable path, not resolve `devenv` through PATH"
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
fn shim_dispatch_falls_back_to_system_command_when_no_version_is_selected() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let shim_dir = devenv_home.join("shims");
    let system_bin = temp.path().join("system-bin");
    fs::create_dir_all(&shim_dir).expect("shim dir should be created");
    fs::create_dir_all(&system_bin).expect("system bin should be created");
    write_executable(
        &system_bin.join("npm"),
        "#!/bin/sh\nprintf 'system-npm:%s:%s' \"$1\" \"$2\"\n",
    );
    let path =
        std::env::join_paths([shim_dir.as_path(), system_bin.as_path()]).expect("PATH should join");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GLOBAL_CONFIG")
        .env("PATH", path)
        .arg("shim")
        .arg("dispatch")
        .arg("npm")
        .arg("--")
        .arg("install")
        .arg("-g")
        .assert()
        .success()
        .stdout(predicate::str::contains("system-npm:install:-g"))
        .stderr(predicate::str::is_empty());
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
        .stdout(predicate::str::contains("PATH"))
        .stdout(predicate::str::contains("devenv()"));

    assert!(
        devenv_home.join("shims/java").is_file(),
        "activate should generate shims so selected tools apply immediately after activation"
    );

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
        .stdout(predicate::str::contains("shims"))
        .stdout(predicate::str::contains("devenv()"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("activate")
        .arg("fish")
        .assert()
        .success()
        .stdout(predicate::str::contains("set -gx DEVENV_HOME"))
        .stdout(predicate::str::contains("set -gx PATH"))
        .stdout(predicate::str::contains("function devenv"));

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .arg("activate")
        .arg("powershell")
        .assert()
        .success()
        .stdout(predicate::str::contains("$env:DEVENV_HOME"))
        .stdout(predicate::str::contains("$env:PATH"))
        .stdout(predicate::str::contains("function devenv"));
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
fn install_go_dry_run_prints_resolved_plan_without_writing_install_or_download_cache() {
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
        .arg("go@1.22")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install plan"))
        .stdout(predicate::str::contains("tool go"))
        .stdout(predicate::str::contains("requested go@1.22"))
        .stdout(predicate::str::contains("resolved 1.22.5"))
        .stdout(predicate::str::contains("provider official"))
        .stdout(predicate::str::contains("url "))
        .stdout(predicate::str::contains("checksum "))
        .stdout(predicate::str::contains("install_path "))
        .stdout(predicate::str::contains("dry_run true"))
        .stderr(predicate::str::is_empty());

    assert!(
        !devenv_home.join("installs").exists(),
        "dry-run must not write install store"
    );
    assert!(
        !devenv_home.join("cache/downloads").exists(),
        "dry-run must not write download cache"
    );
}

#[test]
fn install_java_dry_run_prints_temurin_plan_without_writing_install_or_download_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_temurin_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("java@21")
        .arg("--dry-run")
        .arg("--distribution")
        .arg("temurin")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install plan"))
        .stdout(predicate::str::contains("tool java"))
        .stdout(predicate::str::contains("requested java@21"))
        .stdout(predicate::str::contains("resolved 21.0.2-temurin"))
        .stdout(predicate::str::contains("provider temurin"))
        .stdout(predicate::str::contains("checksum sha256:"))
        .stdout(predicate::str::contains("install_path "))
        .stdout(predicate::str::contains("dry_run true"))
        .stderr(predicate::str::is_empty());

    assert!(
        !devenv_home.join("installs").exists(),
        "dry-run must not write install store"
    );
    assert!(
        !devenv_home.join("cache/downloads").exists(),
        "dry-run must not write download cache"
    );
}

#[test]
fn install_accepts_space_separated_java_version_and_distribution() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_temurin_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_RELEASE_METADATA", &metadata)
        .arg("install")
        .arg("java")
        .arg("21")
        .arg("temurin")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install plan"))
        .stdout(predicate::str::contains("tool java"))
        .stdout(predicate::str::contains("requested java@21"))
        .stdout(predicate::str::contains("resolved 21.0.2-temurin"))
        .stdout(predicate::str::contains("provider temurin"))
        .stdout(predicate::str::contains("dry_run true"))
        .stderr(predicate::str::is_empty());

    assert!(
        !devenv_home.join("installs").exists(),
        "dry-run must not write install store"
    );
    assert!(
        !devenv_home.join("cache/downloads").exists(),
        "dry-run must not write download cache"
    );
}

#[test]
fn install_java_dry_run_reads_provider_manifest_metadata_when_cache_is_missing() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let devenv_home = temp.path().join("devenv-home");
    let base_url = serve_java_temurin_manifest_metadata();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_TEMURIN_API_BASE_URL", base_url)
        .arg("install")
        .arg("java@21")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install plan"))
        .stdout(predicate::str::contains("tool java"))
        .stdout(predicate::str::contains("requested java@21"))
        .stdout(predicate::str::contains("resolved 21.0.2-temurin"))
        .stdout(predicate::str::contains("provider temurin"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn install_go_from_cached_official_metadata_resolves_requested_minor() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_go_archive_with_version(temp.path(), "1.23.4");
    let official = write_go_official_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_GO_OFFICIAL_RELEASE_METADATA", &official)
        .arg("metadata")
        .arg("update")
        .arg("go")
        .assert()
        .success();

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env_remove("DEVENV_GO_RELEASE_METADATA")
        .env_remove("DEVENV_GO_OFFICIAL_RELEASE_METADATA")
        .arg("install")
        .arg("go@1.23")
        .assert()
        .success()
        .stdout(predicate::str::contains("installed go 1.23.4"))
        .stdout(predicate::str::contains("installs/go/1.23.4"));

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/go/1.23.4")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("requested_spec = \"go@1.23\""));
    assert!(install_metadata.contains("resolved_version = \"1.23.4\""));
    assert!(install_metadata.contains("provider = \"official\""));
    assert!(install_metadata.contains(
        "checksum = \"sha256:8c788a765d2f6f52b0e300efd4d1495e305c1a558058d7c2e92b1793c2f315e9\""
    ));
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
fn uninstall_accepts_space_separated_java_version() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let archive = write_fake_java_archive(temp.path());
    let metadata = write_java_release_metadata_fixture(temp.path(), &archive);
    let devenv_home = temp.path().join("devenv-home");

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_RELEASE_METADATA", &metadata)
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("install")
        .arg("java@17")
        .assert()
        .success();

    let install_root = find_single_install_root(&devenv_home, "java", "17.0.11-temurin");
    assert!(install_root.join("devenv-install.toml").is_file());

    Command::cargo_bin("devenv")
        .expect("devenv binary should build")
        .current_dir(temp.path())
        .env("DEVENV_HOME", &devenv_home)
        .env("DEVENV_JAVA_CANDIDATE_PATHS", "")
        .arg("uninstall")
        .arg("java")
        .arg("17.0.11-temurin")
        .assert()
        .success()
        .stdout(predicate::str::contains("uninstalled java 17.0.11-temurin"))
        .stdout(predicate::str::contains(install_root.to_string_lossy()));

    assert!(!install_root.exists());
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

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/flutter/3.24.0")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("requested_spec = \"flutter@3.24\""));
    assert!(install_metadata.contains("resolved_version = \"3.24.0\""));
    assert!(install_metadata.contains("provider = \"stable\""));
    assert!(install_metadata.contains("channel = \"stable\""));
    assert!(install_metadata.contains(
        "checksum = \"f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536\""
    ));

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

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/terraform/1.8.5")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("provider = \"hashicorp\""));

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

    let install_metadata = fs::read_to_string(
        devenv_home
            .join("installs/opentofu/1.8.5")
            .join(current_platform_id_for_test())
            .join("devenv-install.toml"),
    )
    .expect("install metadata should be readable");
    assert!(install_metadata.contains("provider = \"opentofu\""));

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
    assert!(install_metadata.contains("provider = \"official\""));

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
    assert!(install_metadata.contains("provider = \"temurin\""));

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

fn write_fake_go_archive_with_version(parent: &Path, version: &str) -> PathBuf {
    let archive = parent.join(format!("go{version}.fixture.archive"));
    fs::write(
        &archive,
        format!("VERSION\tgo{version}\nbin/go\nbin/gofmt\n"),
    )
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

fn write_go_official_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("go-official-releases.json");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let (go_os, go_arch) = current_go_official_platform_for_test();
    let extension = if go_os == "windows" { "zip" } else { "tar.gz" };
    fs::write(
        &metadata,
        format!(
            r#"
[
  {{
    "version": "go1.23.4",
    "stable": true,
    "files": [
      {{
        "filename": "go1.23.4.{go_os}-{go_arch}.{extension}",
        "os": "{go_os}",
        "arch": "{go_arch}",
        "version": "go1.23.4",
        "kind": "archive",
        "url": "{url}",
        "sha256": "8c788a765d2f6f52b0e300efd4d1495e305c1a558058d7c2e92b1793c2f315e9",
        "size": 34
      }}
    ]
  }},
  {{
    "version": "go1.23.3",
    "stable": false,
    "files": []
  }}
]
"#
        ),
    )
    .expect("official release metadata should be written");
    metadata
}

#[derive(Debug)]
struct GoCatalogFixture {
    root_path: PathBuf,
    root_url: String,
    manifest_sha256: String,
    payload_sha256: String,
}

fn write_go_catalog_fixture(
    parent: &Path,
    archive: &Path,
    corrupt_payload_sha: bool,
) -> GoCatalogFixture {
    let root = parent.join("catalog-v1");
    let payload_path = root.join("tools/go/official/releases.json");
    fs::create_dir_all(payload_path.parent().expect("payload parent should exist"))
        .expect("catalog payload directory should be created");
    let (go_os, go_arch) = current_go_official_platform_for_test();
    let extension = if go_os == "windows" { "zip" } else { "tar.gz" };
    let archive_url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let archive_bytes = fs::read(archive).expect("archive should be readable");
    let archive_checksum = format!("sha256:{}", hex_sha256(&archive_bytes));
    let archive_size = archive_bytes.len();
    let payload = format!(
        r#"{{
  "schema_version": 1,
  "tool": "go",
  "provider": "official",
  "releases": [
    {{
      "version": "go1.23.5",
      "stable": true,
      "yanked": true,
      "yanked_reason": "test yanked release",
      "artifacts": [
        {{
          "filename": "go1.23.5.{go_os}-{go_arch}.{extension}",
          "os": "{go_os}",
          "arch": "{go_arch}",
          "kind": "archive",
          "url": "{archive_url}",
          "checksum": "{archive_checksum}",
          "size": {archive_size}
        }}
      ]
    }},
    {{
      "version": "go1.23.4",
      "stable": true,
      "artifacts": [
        {{
          "filename": "go1.23.4.{go_os}-{go_arch}.{extension}",
          "os": "{go_os}",
          "arch": "{go_arch}",
          "kind": "archive",
          "url": "{archive_url}",
          "checksum": "{archive_checksum}",
          "size": {archive_size}
        }}
      ]
    }}
  ]
}}"#
    );
    fs::write(&payload_path, &payload).expect("catalog payload should be written");
    let payload_sha256 = format!("sha256:{}", hex_sha256(payload.as_bytes()));
    let manifest_payload_sha256 = if corrupt_payload_sha {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_owned()
    } else {
        payload_sha256.clone()
    };
    let manifest = format!(
        r#"{{
  "schema_version": 1,
  "catalog_id": "dev.devenv.catalog",
  "generated_at": "2026-05-22T00:00:00Z",
  "expires_at": "2099-01-01T00:00:00Z",
  "catalog_version": "2026.05.22.1",
  "min_devenv_version": "0.1.0",
  "sequence": 1,
  "entries": [
    {{
      "tool": "go",
      "provider": "official",
      "path": "tools/go/official/releases.json",
      "sha256": "{manifest_payload_sha256}",
      "payload_kind": "normalized-release-index",
      "ttl_seconds": 86400
    }}
  ]
}}"#
    );
    fs::write(root.join("manifest.json"), &manifest).expect("catalog manifest should be written");
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest.as_bytes()));
    fs::write(root.join("manifest.sig"), &manifest_sha256)
        .expect("catalog manifest signature should be written");

    GoCatalogFixture {
        root_path: root.clone(),
        root_url: format!("file://{}", root.display()),
        manifest_sha256,
        payload_sha256,
    }
}

fn expire_catalog_fixture(catalog: &GoCatalogFixture) {
    let manifest_path = catalog.root_path.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest should be readable");
    let expired = manifest.replace("2099-01-01T00:00:00Z", "2000-01-01T00:00:00Z");
    fs::write(&manifest_path, &expired).expect("expired manifest should be written");
    let manifest_sha256 = format!("sha256:{}", hex_sha256(expired.as_bytes()));
    fs::write(catalog.root_path.join("manifest.sig"), manifest_sha256)
        .expect("expired manifest signature should be written");
}

#[derive(Debug)]
struct NodeCatalogFixture {
    root_url: String,
    manifest_sha256: String,
    payload_sha256: String,
}

#[derive(Debug, Clone, Copy)]
enum NodeCatalogFixtureMode {
    Normal,
    MissingCurrentChecksum,
    UnsupportedCurrentPlatform,
}

fn write_node_catalog_fixture(
    parent: &Path,
    archive: &Path,
    mode: NodeCatalogFixtureMode,
) -> NodeCatalogFixture {
    let root = parent.join("catalog-node-v1");
    let payload_path = root.join("tools/node/official/releases.json");
    fs::create_dir_all(payload_path.parent().expect("payload parent should exist"))
        .expect("catalog payload directory should be created");
    let (current_os, current_arch, current_extension) = current_node_catalog_platform_for_test();
    let (release_os, release_arch, release_extension) = match mode {
        NodeCatalogFixtureMode::UnsupportedCurrentPlatform => {
            alternate_node_catalog_platform_for_test()
        }
        NodeCatalogFixtureMode::Normal | NodeCatalogFixtureMode::MissingCurrentChecksum => {
            (current_os, current_arch, current_extension)
        }
    };
    let archive_url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let archive_bytes = fs::read(archive).expect("archive should be readable");
    let archive_checksum = format!("sha256:{}", hex_sha256(&archive_bytes));
    let archive_size = archive_bytes.len();
    let latest_checksum_field = match mode {
        NodeCatalogFixtureMode::MissingCurrentChecksum => String::new(),
        NodeCatalogFixtureMode::Normal | NodeCatalogFixtureMode::UnsupportedCurrentPlatform => {
            format!(
                r#",
          "checksum": "{archive_checksum}""#
            )
        }
    };
    let payload = format!(
        r#"{{
  "schema_version": 1,
  "tool": "node",
  "provider": "official",
  "releases": [
    {{
      "version": "v20.12.0",
      "stable": true,
      "artifacts": [
        {{
          "filename": "node-v20.12.0-{release_os}-{release_arch}.{release_extension}",
          "os": "{release_os}",
          "arch": "{release_arch}",
          "kind": "archive",
          "url": "{archive_url}",
          "size": {archive_size}{latest_checksum_field}
        }}
      ]
    }},
    {{
      "version": "v20.11.1",
      "stable": true,
      "artifacts": [
        {{
          "filename": "node-v20.11.1-{current_os}-{current_arch}.{current_extension}",
          "os": "{current_os}",
          "arch": "{current_arch}",
          "kind": "archive",
          "url": "{archive_url}",
          "checksum": "{archive_checksum}",
          "size": {archive_size}
        }}
      ]
    }}
  ]
}}"#
    );
    fs::write(&payload_path, &payload).expect("catalog payload should be written");
    let payload_sha256 = format!("sha256:{}", hex_sha256(payload.as_bytes()));
    let manifest = format!(
        r#"{{
  "schema_version": 1,
  "catalog_id": "dev.devenv.catalog",
  "generated_at": "2026-05-22T00:00:00Z",
  "expires_at": "2099-01-01T00:00:00Z",
  "catalog_version": "2026.05.22.1",
  "min_devenv_version": "0.1.0",
  "sequence": 1,
  "entries": [
    {{
      "tool": "node",
      "provider": "official",
      "path": "tools/node/official/releases.json",
      "sha256": "{payload_sha256}",
      "payload_kind": "normalized-release-index",
      "ttl_seconds": 86400
    }}
  ]
}}"#
    );
    fs::write(root.join("manifest.json"), &manifest).expect("catalog manifest should be written");
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest.as_bytes()));
    fs::write(root.join("manifest.sig"), &manifest_sha256)
        .expect("catalog manifest signature should be written");

    NodeCatalogFixture {
        root_url: format!("file://{}", root.display()),
        manifest_sha256,
        payload_sha256,
    }
}

#[derive(Debug)]
struct TerraformCatalogFixture {
    root_url: String,
    manifest_sha256: String,
    payload_sha256: String,
}

#[derive(Debug, Clone, Copy)]
enum TerraformCatalogFixtureMode {
    Normal,
    MissingCurrentChecksum,
    UnsupportedCurrentPlatform,
}

fn write_terraform_catalog_fixture(
    parent: &Path,
    binary: &Path,
    mode: TerraformCatalogFixtureMode,
) -> TerraformCatalogFixture {
    let root = parent.join("catalog-terraform-v1");
    let payload_path = root.join("tools/terraform/hashicorp/releases.json");
    fs::create_dir_all(payload_path.parent().expect("payload parent should exist"))
        .expect("catalog payload directory should be created");
    let (current_os, current_arch, current_filename) = current_iac_catalog_platform_for_test();
    let (release_os, release_arch, release_filename) = match mode {
        TerraformCatalogFixtureMode::UnsupportedCurrentPlatform => {
            alternate_iac_catalog_platform_for_test()
        }
        TerraformCatalogFixtureMode::Normal
        | TerraformCatalogFixtureMode::MissingCurrentChecksum => {
            (current_os, current_arch, current_filename)
        }
    };
    let binary_url = binary
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let binary_bytes = fs::read(binary).expect("binary should be readable");
    let binary_checksum = format!("sha256:{}", hex_sha256(&binary_bytes));
    let binary_size = binary_bytes.len();
    let latest_checksum_field = match mode {
        TerraformCatalogFixtureMode::MissingCurrentChecksum => String::new(),
        TerraformCatalogFixtureMode::Normal
        | TerraformCatalogFixtureMode::UnsupportedCurrentPlatform => {
            format!(
                r#",
          "checksum": "{binary_checksum}""#
            )
        }
    };
    let payload = format!(
        r#"{{
  "schema_version": 1,
  "tool": "terraform",
  "provider": "hashicorp",
  "releases": [
    {{
      "version": "1.8.6",
      "stable": true,
      "artifacts": [
        {{
          "filename": "{release_filename}",
          "os": "{release_os}",
          "arch": "{release_arch}",
          "kind": "single-binary",
          "url": "{binary_url}",
          "size": {binary_size}{latest_checksum_field}
        }}
      ]
    }},
    {{
      "version": "1.8.5",
      "stable": true,
      "artifacts": [
        {{
          "filename": "{current_filename}",
          "os": "{current_os}",
          "arch": "{current_arch}",
          "kind": "single-binary",
          "url": "{binary_url}",
          "checksum": "{binary_checksum}",
          "size": {binary_size}
        }}
      ]
    }}
  ]
}}"#
    );
    fs::write(&payload_path, &payload).expect("catalog payload should be written");
    let payload_sha256 = format!("sha256:{}", hex_sha256(payload.as_bytes()));
    let manifest = format!(
        r#"{{
  "schema_version": 1,
  "catalog_id": "dev.devenv.catalog",
  "generated_at": "2026-05-22T00:00:00Z",
  "expires_at": "2099-01-01T00:00:00Z",
  "catalog_version": "2026.05.22.1",
  "min_devenv_version": "0.1.0",
  "sequence": 1,
  "entries": [
    {{
      "tool": "terraform",
      "provider": "hashicorp",
      "path": "tools/terraform/hashicorp/releases.json",
      "sha256": "{payload_sha256}",
      "payload_kind": "normalized-release-index",
      "ttl_seconds": 86400
    }}
  ]
}}"#
    );
    fs::write(root.join("manifest.json"), &manifest).expect("catalog manifest should be written");
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest.as_bytes()));
    fs::write(root.join("manifest.sig"), &manifest_sha256)
        .expect("catalog manifest signature should be written");

    TerraformCatalogFixture {
        root_url: format!("file://{}", root.display()),
        manifest_sha256,
        payload_sha256,
    }
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
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68

[[release.file]]
filename = "flutter_macos_x64_3.24.0-stable.zip"
os = "macos"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68

[[release.file]]
filename = "flutter_linux_x64_3.24.0-stable.tar.gz"
os = "linux"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68

[[release.file]]
filename = "flutter_linux_arm64_3.24.0-stable.tar.gz"
os = "linux"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68

[[release.file]]
filename = "flutter_windows_x64_3.24.0-stable.zip"
os = "windows"
arch = "x64"
kind = "archive"
url = "{url}"
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68

[[release.file]]
filename = "flutter_windows_arm64_3.24.0-stable.zip"
os = "windows"
arch = "arm64"
kind = "archive"
url = "{url}"
sha256 = "f5b1e0e36334ce2143fad93073cb8f333d53a54fcd7bee85133344a81e0da536"
size = 68
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

#[derive(Debug, Clone, Copy)]
enum IacFixtureTool {
    Terraform,
    OpenTofu,
}

fn serve_iac_official_metadata(tool: IacFixtureTool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut request = [0_u8; 2048];
            let read = stream.read(&mut request).expect("request should read");
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, reason, body) = match (tool, path) {
                (IacFixtureTool::Terraform, "/index.json") => {
                    (200, "OK", terraform_official_index_fixture_body())
                }
                (IacFixtureTool::Terraform, "/1.8.5/terraform_1.8.5_SHA256SUMS") => {
                    (200, "OK", terraform_official_sha256s_185())
                }
                (IacFixtureTool::OpenTofu, "/releases.json") => {
                    (200, "OK", opentofu_official_releases_fixture_body())
                }
                (IacFixtureTool::OpenTofu, "/v1.8.5/tofu_1.8.5_SHA256SUMS") => {
                    (200, "OK", opentofu_official_sha256s_185())
                }
                _ => (404, "Not Found", "not found"),
            };
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("response should write");
        }
    });
    format!("http://{address}")
}

fn terraform_official_index_fixture_body() -> &'static str {
    r#"
{
  "versions": {
    "1.8.5": {
      "builds": [
        {"os": "darwin", "arch": "arm64", "filename": "terraform_1.8.5_darwin_arm64.zip", "url": "https://example.test/terraform_1.8.5_darwin_arm64.zip"},
        {"os": "linux", "arch": "amd64", "filename": "terraform_1.8.5_linux_amd64.zip", "url": "https://example.test/terraform_1.8.5_linux_amd64.zip"},
        {"os": "windows", "arch": "amd64", "filename": "terraform_1.8.5_windows_amd64.zip", "url": "https://example.test/terraform_1.8.5_windows_amd64.zip"}
      ]
    }
  }
}
"#
}

fn opentofu_official_releases_fixture_body() -> &'static str {
    r#"
[
  {
    "tag_name": "v1.8.5",
    "draft": false,
    "prerelease": false,
    "assets": [
      {"name": "tofu_1.8.5_darwin_arm64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_darwin_arm64.zip"},
      {"name": "tofu_1.8.5_linux_amd64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_linux_amd64.zip"},
      {"name": "tofu_1.8.5_windows_amd64.zip", "browser_download_url": "https://example.test/tofu_1.8.5_windows_amd64.zip"},
      {"name": "tofu_1.8.5_SHA256SUMS", "browser_download_url": "https://example.test/tofu_1.8.5_SHA256SUMS"}
    ]
  }
]
"#
}

fn terraform_official_sha256s_185() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  terraform_1.8.5_darwin_arm64.zip
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  terraform_1.8.5_linux_amd64.zip
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  terraform_1.8.5_windows_amd64.zip
"#
}

fn opentofu_official_sha256s_185() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tofu_1.8.5_darwin_arm64.zip
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  tofu_1.8.5_linux_amd64.zip
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  tofu_1.8.5_windows_amd64.zip
"#
}

fn serve_flutter_official_metadata() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        loop {
            let Ok((mut stream, _)) = listener.accept() else {
                continue;
            };
            let mut request = [0_u8; 2048];
            let Ok(read) = stream.read(&mut request) else {
                continue;
            };
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, reason, body) = match path {
                "/releases_macos.json" => (200, "OK", flutter_official_macos_fixture_body()),
                "/releases_linux.json" => (200, "OK", flutter_official_linux_fixture_body()),
                "/releases_windows.json" => (200, "OK", flutter_official_windows_fixture_body()),
                _ => (404, "Not Found", "not found"),
            };
            let _ = write!(
                stream,
                "HTTP/1.0 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{address}")
}

fn flutter_official_macos_fixture_body() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "macos-arm64"},
  "releases": [
    {
      "hash": "macos-arm64",
      "channel": "stable",
      "version": "3.24.0",
      "dart_sdk_arch": "arm64",
      "archive": "stable/macos/flutter_macos_arm64_3.24.0-stable.zip",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "hash": "macos-beta",
      "channel": "beta",
      "version": "3.25.0-0.1.pre",
      "dart_sdk_arch": "arm64",
      "archive": "beta/macos/flutter_macos_arm64_3.25.0-0.1.pre-beta.zip",
      "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
  ]
}
"#
}

fn flutter_official_linux_fixture_body() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "linux-x64"},
  "releases": [
    {
      "hash": "linux-x64",
      "channel": "stable",
      "version": "3.24.0",
      "archive": "stable/linux/flutter_linux_3.24.0-stable.tar.xz",
      "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    }
  ]
}
"#
}

fn flutter_official_windows_fixture_body() -> &'static str {
    r#"
{
  "base_url": "https://storage.googleapis.com/flutter_infra_release/releases",
  "current_release": {"stable": "windows-x64"},
  "releases": [
    {
      "hash": "windows-x64",
      "channel": "stable",
      "version": "3.24.0",
      "archive": "stable/windows/flutter_windows_3.24.0-stable.zip",
      "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    }
  ]
}
"#
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

fn write_node_official_index_fixture(parent: &Path) -> PathBuf {
    let metadata = parent.join("node-official-index.json");
    fs::write(&metadata, node_official_index_fixture_body())
        .expect("Node official index fixture should be written");
    metadata
}

fn write_node_official_shasums_fixtures(parent: &Path) -> PathBuf {
    let root = parent.join("node-shasums");
    let v20 = root.join("v20.11.1");
    let v21 = root.join("v21.2.0");
    fs::create_dir_all(&v20).expect("v20 shasums dir should be created");
    fs::create_dir_all(&v21).expect("v21 shasums dir should be created");
    fs::write(v20.join("SHASUMS256.txt"), node_official_shasums_20())
        .expect("v20 shasums should be written");
    fs::write(v21.join("SHASUMS256.txt"), node_official_shasums_21())
        .expect("v21 shasums should be written");
    root
}

fn node_official_index_fixture_body() -> &'static str {
    r#"
[
  {
    "version": "v20.11.1",
    "date": "2024-02-13",
    "files": [
      "osx-arm64-tar",
      "osx-x64-tar",
      "linux-x64",
      "linux-arm64",
      "win-x64-zip",
      "win-arm64-zip",
      "headers"
    ],
    "lts": "Iron"
  },
  {
    "version": "v21.2.0",
    "date": "2023-11-14",
    "files": [
      "linux-x64"
    ],
    "lts": false
  }
]
"#
}

fn node_official_shasums_20() -> &'static str {
    r#"
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  node-v20.11.1-darwin-arm64.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  node-v20.11.1-darwin-x64.tar.gz
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  node-v20.11.1-linux-x64.tar.gz
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  node-v20.11.1-linux-arm64.tar.gz
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  node-v20.11.1-win-x64.zip
ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff  node-v20.11.1-win-arm64.zip
"#
}

fn node_official_shasums_21() -> &'static str {
    r#"
9999999999999999999999999999999999999999999999999999999999999999  node-v21.2.0-linux-x64.tar.gz
"#
}

fn serve_node_official_metadata() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut request = [0_u8; 2048];
            let read = stream.read(&mut request).expect("request should read");
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, reason, body) = match path {
                "/index.json" => (200, "OK", node_official_index_fixture_body()),
                "/v20.11.1/SHASUMS256.txt" => (200, "OK", node_official_shasums_20()),
                "/v21.2.0/SHASUMS256.txt" => (200, "OK", node_official_shasums_21()),
                _ => (404, "Not Found", "not found"),
            };
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("response should write");
        }
    });
    format!("http://{address}")
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

fn write_java_temurin_release_metadata_fixture(parent: &Path, archive: &Path) -> PathBuf {
    let metadata = parent.join("java-temurin-releases.json");
    let url = archive
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let (api_os, api_arch, filename_os, filename_arch, extension) =
        current_temurin_platform_for_test();
    fs::write(
        &metadata,
        format!(
            r#"
[
  {{
    "release_name": "jdk-21.0.2+13",
    "release_type": "ga",
    "version_data": {{
      "major": 21,
      "openjdk_version": "21.0.2+13",
      "semver": "21.0.2+13"
    }},
    "binaries": [
      {{
        "architecture": "{api_arch}",
        "os": "{api_os}",
        "image_type": "jdk",
        "package": {{
          "name": "OpenJDK21U-jdk_{filename_arch}_{filename_os}_hotspot_21.0.2_13.{extension}",
          "link": "{url}",
          "checksum": "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068",
          "size": 50
        }}
      }}
    ]
  }},
  {{
    "release_name": "jdk-21.0.1+12",
    "release_type": "ga",
    "version_data": {{
      "major": 21,
      "openjdk_version": "21.0.1+12"
    }},
    "binaries": []
  }}
]
"#
        ),
    )
    .expect("Temurin release metadata should be written");
    metadata
}

fn serve_java_temurin_manifest_metadata() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        loop {
            let Ok((mut stream, _)) = listener.accept() else {
                continue;
            };
            let mut request = [0_u8; 2048];
            let Ok(read) = stream.read(&mut request) else {
                continue;
            };
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, reason, body) = if path == "/v3/info/available_releases" {
                (200, "OK", r#"{"available_releases":[21,17]}"#.to_owned())
            } else if path
                == "/v3/assets/feature_releases/21/ga?image_type=jdk&project=jdk&page=0&page_size=20"
            {
                (
                    200,
                    "OK",
                    java_temurin_manifest_feature_body(21, "21.0.2+13"),
                )
            } else if path
                == "/v3/assets/feature_releases/17/ga?image_type=jdk&project=jdk&page=0&page_size=20"
            {
                (
                    200,
                    "OK",
                    java_temurin_manifest_feature_body(17, "17.0.11+9"),
                )
            } else if path.starts_with("/v3/assets/feature_releases/") {
                (200, "OK", "[]".to_owned())
            } else {
                (404, "Not Found", "not found".to_owned())
            };
            let _ = write!(
                stream,
                "HTTP/1.0 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{address}")
}

fn serve_java_temurin_manifest_feature_metadata_without_available_index() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        loop {
            let Ok((mut stream, _)) = listener.accept() else {
                continue;
            };
            let mut request = [0_u8; 2048];
            let Ok(read) = stream.read(&mut request) else {
                continue;
            };
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, reason, body) = if path == "/v3/info/available_releases" {
                (404, "Not Found", "not found".to_owned())
            } else if path
                == "/v3/assets/feature_releases/21/ga?image_type=jdk&project=jdk&page=0&page_size=20"
            {
                (
                    200,
                    "OK",
                    java_temurin_manifest_feature_body(21, "21.0.2+13"),
                )
            } else if path
                == "/v3/assets/feature_releases/17/ga?image_type=jdk&project=jdk&page=0&page_size=20"
            {
                (
                    200,
                    "OK",
                    java_temurin_manifest_feature_body(17, "17.0.11+9"),
                )
            } else if path.starts_with("/v3/assets/feature_releases/") {
                (200, "OK", "[]".to_owned())
            } else {
                (404, "Not Found", "not found".to_owned())
            };
            let _ = write!(
                stream,
                "HTTP/1.0 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{address}")
}

fn java_temurin_manifest_feature_body(major: u32, version: &str) -> String {
    let (api_os, api_arch, filename_os, filename_arch, extension) =
        current_temurin_platform_for_test();
    let filename_version = version.replace('+', "_").replace('.', ".");
    format!(
        r#"
[
  {{
    "release_name": "jdk-{version}",
    "release_type": "ga",
    "version_data": {{
      "major": {major},
      "openjdk_version": "{version}",
      "semver": "{version}"
    }},
    "binaries": [
      {{
        "architecture": "{api_arch}",
        "os": "{api_os}",
        "image_type": "jdk",
        "package": {{
          "name": "OpenJDK{major}U-jdk_{filename_arch}_{filename_os}_hotspot_{filename_version}.{extension}",
          "link": "https://example.test/temurin/OpenJDK{major}U-jdk_{filename_arch}_{filename_os}_hotspot_{filename_version}.{extension}",
          "checksum": "c3718be02942e7077e764bc77d2775235613c9a59935ed767b3ca7448dfde068",
          "size": 50
        }}
      }}
    ]
  }}
]
"#
    )
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

fn current_go_official_platform_for_test() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };

    (os, arch)
}

fn current_node_catalog_platform_for_test() -> (&'static str, &'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let extension = if os == "win" { "zip" } else { "tar.gz" };

    (os, arch, extension)
}

fn alternate_node_catalog_platform_for_test() -> (&'static str, &'static str, &'static str) {
    let (current_os, current_arch, _) = current_node_catalog_platform_for_test();
    if current_os == "linux" && current_arch == "x64" {
        ("darwin", "arm64", "tar.gz")
    } else {
        ("linux", "x64", "tar.gz")
    }
}

fn current_iac_catalog_platform_for_test() -> (&'static str, &'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let filename = if os == "windows" {
        "terraform.exe"
    } else {
        "terraform"
    };

    (os, arch, filename)
}

fn alternate_iac_catalog_platform_for_test() -> (&'static str, &'static str, &'static str) {
    let (current_os, current_arch, _) = current_iac_catalog_platform_for_test();
    if current_os == "linux" && current_arch == "amd64" {
        ("darwin", "arm64", "terraform")
    } else {
        ("linux", "amd64", "terraform")
    }
}

fn first_known_manifest_version(manifest: &str) -> String {
    let manifest: Value =
        serde_json::from_str(manifest).expect("provider manifest should parse as JSON");
    manifest
        .get("version")
        .and_then(Value::as_object)
        .and_then(|version| version.get("known_versions"))
        .and_then(Value::as_object)
        .and_then(|known_versions| known_versions.get("versions"))
        .and_then(Value::as_array)
        .and_then(|versions| versions.first())
        .and_then(Value::as_str)
        .expect("provider manifest should include at least one known version")
        .to_owned()
}

fn current_temurin_platform_for_test() -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    let (api_os, filename_os) = if cfg!(target_os = "macos") {
        ("mac", "mac")
    } else if cfg!(target_os = "windows") {
        ("windows", "windows")
    } else {
        ("linux", "linux")
    };
    let (api_arch, filename_arch) = if cfg!(target_arch = "aarch64") {
        ("aarch64", "aarch64")
    } else {
        ("x64", "x64")
    };
    let extension = if api_os == "windows" { "zip" } else { "tar.gz" };

    (api_os, api_arch, filename_os, filename_arch, extension)
}
