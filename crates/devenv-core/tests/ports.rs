use std::path::PathBuf;

use devenv_core::{
    ActivationPlan, Architecture, CommandInvocation, CommandRunner, EnvOperation,
    FakeCommandRunner, FakeToolAdapter, InMemoryInstallStore, InMemoryRuntimeRegistry,
    InstallStore, Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry,
    ToolAdapter, ToolName, Version, VersionRequirement,
};

#[test]
fn fake_adapter_resolves_exact_version() {
    let adapter = FakeToolAdapter::new("fake")
        .with_versions(["1.0.0", "1.1.0"])
        .expect("versions should be valid");
    let requirement = VersionRequirement::exact("1.1.0").expect("requirement should be valid");

    let resolved = adapter
        .resolve_version(&requirement)
        .expect("resolution should succeed");

    assert_eq!(resolved.expect("version should match").raw(), "1.1.0");
}

#[test]
fn fake_adapter_reports_no_matching_version() {
    let adapter = FakeToolAdapter::new("fake")
        .with_versions(["1.0.0"])
        .expect("versions should be valid");
    let requirement = VersionRequirement::exact("2.0.0").expect("requirement should be valid");

    let resolved = adapter
        .resolve_version(&requirement)
        .expect("resolution should succeed");

    assert!(resolved.is_none());
}

#[test]
fn installed_and_registered_runtimes_are_distinguishable() {
    let tool = ToolName::new("fake").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let installed = Installation::new(
        tool.clone(),
        Version::new("1.0.0").expect("version should be valid"),
        platform,
        "/devenv/installs/fake/1.0.0",
    );
    let registered = RegisteredRuntime::new(
        tool.clone(),
        Version::new("1.0.0").expect("version should be valid"),
        platform,
        "/opt/fake/1.0.0",
    );
    let mut install_store = InMemoryInstallStore::default();
    let mut registry = InMemoryRuntimeRegistry::default();

    install_store
        .add_installation(installed.clone())
        .expect("installation should be added");
    registry
        .add_registered_runtime(registered.clone())
        .expect("runtime should be registered");

    assert_eq!(install_store.list_installations(&tool), vec![installed]);
    assert_eq!(registry.list_registered_runtimes(&tool), vec![registered]);
    assert_ne!(
        install_store.list_installations(&tool)[0].root(),
        registry.list_registered_runtimes(&tool)[0].root()
    );
}

#[test]
fn runtime_registry_removes_external_reference_without_touching_runtime() {
    let tool = ToolName::new("fake").expect("tool should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let version = Version::new("1.0.0").expect("version should be valid");
    let runtime = RegisteredRuntime::new(tool.clone(), version.clone(), platform, "/opt/fake");
    let mut registry = InMemoryRuntimeRegistry::default();
    registry
        .add_registered_runtime(runtime.clone())
        .expect("runtime should be registered");

    let removed = registry
        .remove_registered_runtime(&tool, &version, platform, None)
        .expect("runtime should be removed");

    assert_eq!(removed, vec![runtime]);
    assert!(registry.list_registered_runtimes(&tool).is_empty());
}

#[test]
fn activation_plan_is_structured_without_shell_rendering() {
    let plan = ActivationPlan::new()
        .with_operation(EnvOperation::Set {
            key: "FAKE_HOME".to_owned(),
            value: "/devenv/installs/fake/1.0.0".to_owned(),
        })
        .with_operation(EnvOperation::PrependPath {
            path: PathBuf::from("/devenv/installs/fake/1.0.0/bin"),
        });

    assert_eq!(plan.operations().len(), 2);
    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "FAKE_HOME" && value == "/devenv/installs/fake/1.0.0"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/devenv/installs/fake/1.0.0/bin")
    ));
}

#[test]
fn fake_command_runner_records_command_without_running_it() {
    let mut runner = FakeCommandRunner::default();
    let invocation = CommandInvocation::new("fake")
        .with_arg("--version")
        .with_cwd("/workspace")
        .with_activation(ActivationPlan::new().with_operation(EnvOperation::Set {
            key: "FAKE_HOME".to_owned(),
            value: "/devenv/installs/fake/1.0.0".to_owned(),
        }));

    let output = runner.run(invocation.clone()).expect("run should succeed");

    assert_eq!(output.status_code(), 0);
    assert_eq!(runner.invocations(), &[invocation]);
}
