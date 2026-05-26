use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, CommandOutput, EnvOperation, ExecCommand, FakeCommandRunner,
    FakeToolAdapter, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore, Installation,
    OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolName, Version,
    VersionRequirement, activation_plan_for_selected_runtime,
};

#[test]
fn activation_prepends_runtime_bin_before_existing_path() {
    let mut environment = BTreeMap::from([("PATH".to_owned(), "/usr/bin".to_owned())]);
    let plan = ActivationPlan::new().prepend_path("/opt/runtime/bin");

    let delta = plan.apply_to(&mut environment);

    assert_eq!(
        environment.get("PATH").map(String::as_str),
        Some("/opt/runtime/bin:/usr/bin")
    );
    assert_eq!(
        delta.sets().get("PATH").map(String::as_str),
        Some("/opt/runtime/bin:/usr/bin")
    );
}

#[test]
fn activation_does_not_duplicate_existing_path_entries() {
    let mut environment =
        BTreeMap::from([("PATH".to_owned(), "/opt/runtime/bin:/usr/bin".to_owned())]);
    let plan = ActivationPlan::new().prepend_path("/opt/runtime/bin");

    plan.apply_to(&mut environment);

    assert_eq!(
        environment.get("PATH").map(String::as_str),
        Some("/opt/runtime/bin:/usr/bin")
    );
}

#[test]
fn activation_can_set_and_unset_environment_variables() {
    let mut environment = BTreeMap::from([
        ("OLD_HOME".to_owned(), "/old".to_owned()),
        ("PATH".to_owned(), "/usr/bin".to_owned()),
    ]);
    let plan = ActivationPlan::new()
        .set_env("FAKE_HOME", "/opt/fake")
        .unset_env("OLD_HOME");

    let delta = plan.apply_to(&mut environment);

    assert_eq!(
        environment.get("FAKE_HOME").map(String::as_str),
        Some("/opt/fake")
    );
    assert!(!environment.contains_key("OLD_HOME"));
    assert!(delta.unsets().contains("OLD_HOME"));
}

#[test]
fn exec_passes_command_args_cwd_activation_and_env_delta_to_runner() {
    let activation = ActivationPlan::new()
        .set_env("FAKE_HOME", "/opt/fake")
        .prepend_path("/opt/fake/bin");
    let environment = BTreeMap::from([("PATH".to_owned(), "/usr/bin".to_owned())]);
    let mut runner = FakeCommandRunner::default().with_output(CommandOutput::new(0, "ok", ""));
    let command = ExecCommand::new("fake", activation.clone())
        .with_arg("--version")
        .with_cwd("/workspace");

    let output = command
        .execute(&environment, &mut runner)
        .expect("exec should succeed");

    assert_eq!(output.stdout(), "ok");
    let invocation = runner
        .invocations()
        .first()
        .expect("runner should record invocation");
    assert_eq!(invocation.command(), "fake");
    assert_eq!(invocation.args(), &["--version"]);
    assert_eq!(invocation.cwd(), Some(Path::new("/workspace")));
    assert_eq!(invocation.activation(), &activation);
    assert_eq!(
        invocation
            .env_delta()
            .sets()
            .get("FAKE_HOME")
            .map(String::as_str),
        Some("/opt/fake")
    );
    assert_eq!(
        invocation
            .env_delta()
            .sets()
            .get("PATH")
            .map(String::as_str),
        Some("/opt/fake/bin:/usr/bin")
    );
}

#[test]
fn selected_runtime_activation_uses_registered_runtime_root() {
    let tool = ToolName::new("fake").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let requirement = VersionRequirement::exact("1.0.0").expect("requirement should be valid");
    let install_store = InMemoryInstallStore::default();
    let mut registry = InMemoryRuntimeRegistry::default();
    registry
        .add_registered_runtime(RegisteredRuntime::new(
            tool.clone(),
            Version::new("1.0.0").expect("version should be valid"),
            platform,
            "/opt/fake",
        ))
        .expect("runtime should be registered");
    let adapter = FakeToolAdapter::new("fake")
        .with_activation_plan(ActivationPlan::new().prepend_path("/opt/fake/bin"));

    let plan = activation_plan_for_selected_runtime(
        &tool,
        &requirement,
        platform,
        &install_store,
        &registry,
        &adapter,
    )
    .expect("activation should resolve");

    assert_eq!(
        plan.operations(),
        &[EnvOperation::PrependPath {
            path: PathBuf::from("/opt/fake/bin")
        }]
    );
}

#[test]
fn selected_runtime_activation_prefers_owned_install_over_registered_runtime() {
    let tool = ToolName::new("fake").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let requirement = VersionRequirement::exact("1.0.0").expect("requirement should be valid");
    let mut install_store = InMemoryInstallStore::default();
    install_store
        .add_installation(Installation::new(
            tool.clone(),
            Version::new("1.0.0").expect("version should be valid"),
            platform,
            "/devenv/fake",
        ))
        .expect("installation should be added");
    let registry = InMemoryRuntimeRegistry::default();
    let adapter = FakeToolAdapter::new("fake")
        .with_activation_plan(ActivationPlan::new().prepend_path("/devenv/fake/bin"));

    let plan = activation_plan_for_selected_runtime(
        &tool,
        &requirement,
        platform,
        &install_store,
        &registry,
        &adapter,
    )
    .expect("activation should resolve");

    assert_eq!(
        plan.operations(),
        &[EnvOperation::PrependPath {
            path: PathBuf::from("/devenv/fake/bin")
        }]
    );
}

#[test]
fn missing_selected_runtime_reports_add_and_install_guidance() {
    let tool = ToolName::new("java").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let requirement = VersionRequirement::exact("17").expect("requirement should be valid");
    let install_store = InMemoryInstallStore::default();
    let registry = InMemoryRuntimeRegistry::default();
    let adapter = FakeToolAdapter::new("java");

    let error = activation_plan_for_selected_runtime(
        &tool,
        &requirement,
        platform,
        &install_store,
        &registry,
        &adapter,
    )
    .expect_err("activation should fail");
    let message = error.to_string();

    assert!(message.contains("java 17 is selected but not installed or registered"));
    assert!(message.contains("devenv add java <path>"));
    assert!(message.contains("devenv install java 17"));
}
