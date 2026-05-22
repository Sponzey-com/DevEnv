use devenv_core::{
    Architecture, CoreResult, InMemoryInstallStore, InMemoryRuntimeRegistry, InstallStore,
    Installation, OperatingSystem, Platform, RegisteredRuntime, RuntimeRegistry, ToolName, Version,
    VersionMatcher, VersionRequirement, add_external_runtime, remove_external_runtime,
    uninstall_runtime,
};

#[test]
fn add_external_runtime_records_registry_reference() {
    let tool = ToolName::new("java").expect("tool should be valid");
    let version = Version::new("17").expect("version should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let runtime = RegisteredRuntime::new(tool.clone(), version, platform, "/opt/jdk-17");
    let mut registry = InMemoryRuntimeRegistry::default();

    add_external_runtime(&mut registry, runtime.clone()).expect("runtime should be added");

    assert_eq!(registry.list_registered_runtimes(&tool), vec![runtime]);
}

#[test]
fn remove_external_runtime_removes_registry_reference_only() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let version = Version::new("1.22.5").expect("version should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let runtime = RegisteredRuntime::new(tool.clone(), version.clone(), platform, "/opt/go");
    let mut registry = InMemoryRuntimeRegistry::default();
    add_external_runtime(&mut registry, runtime.clone()).expect("runtime should be added");

    let removed = remove_external_runtime(&mut registry, &tool, &version, platform, None)
        .expect("runtime should be removed");

    assert_eq!(removed, vec![runtime]);
    assert!(registry.list_registered_runtimes(&tool).is_empty());
}

#[test]
fn uninstall_runtime_removes_owned_installation_only() {
    let tool = ToolName::new("go").expect("tool should be valid");
    let version = Version::new("1.22.5").expect("version should be valid");
    let requirement = VersionRequirement::exact("1.22.5").expect("requirement should be valid");
    let platform = Platform::new(OperatingSystem::Linux, Architecture::X64);
    let installation = Installation::new(tool.clone(), version.clone(), platform, "/owned/go");
    let mut store = InMemoryInstallStore::default();
    store
        .add_installation(installation.clone())
        .expect("installation should be added");

    let removed = uninstall_runtime(&mut store, &tool, &requirement, platform, &ExactMatcher)
        .expect("installation should uninstall");

    assert_eq!(
        removed
            .expect("owned installation should be removed")
            .installation(),
        &installation
    );
    assert!(store.list_installations(&tool).is_empty());
}

struct ExactMatcher;

impl VersionMatcher for ExactMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        Ok(candidates
            .iter()
            .find(|version| version.raw() == requirement.raw())
            .cloned())
    }
}
