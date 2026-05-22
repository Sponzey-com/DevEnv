use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::{
    ActivationPlan, ArchiveExtractor, ArtifactResolver, ChecksumVerifier, Clock, CommandInvocation,
    CommandOutput, CommandRunner, CoreError, CoreResult, Downloader, ExtractionManifest,
    InstallPlan, InstallStore, InstallTransaction, InstallTransactionManager, Installation,
    InstallationMetadata, InstalledRuntimeValidator, LockKey, LockManager, Platform,
    RegisteredRuntime, RuntimeRegistry, ShimSpec, ShimWriter, ToolAdapter, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionSource,
};

pub const ACTIVE_SHIM_ENV: &str = "DEVENV_ACTIVE_SHIM";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecCommand {
    command: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    activation: ActivationPlan,
}

impl ExecCommand {
    pub fn new(command: impl Into<String>, activation: ActivationPlan) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            cwd: None,
            activation,
        }
    }

    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    pub fn activation(&self) -> &ActivationPlan {
        &self.activation
    }

    pub fn execute(
        &self,
        environment: &BTreeMap<String, String>,
        runner: &mut dyn CommandRunner,
    ) -> CoreResult<CommandOutput> {
        let mut invocation =
            CommandInvocation::new(self.command.clone()).with_activation(self.activation.clone());
        for arg in &self.args {
            invocation = invocation.with_arg(arg.clone());
        }
        if let Some(cwd) = &self.cwd {
            invocation = invocation.with_cwd(cwd.clone());
        }
        invocation = invocation.with_env_delta(self.activation.env_delta(environment));

        runner.run(invocation)
    }
}

pub fn activation_plan_for_selected_runtime(
    tool: &ToolName,
    requirement: &VersionRequirement,
    platform: Platform,
    install_store: &dyn InstallStore,
    registry: &dyn RuntimeRegistry,
    adapter: &dyn ToolAdapter,
) -> CoreResult<ActivationPlan> {
    let Some(runtime_root) =
        selected_runtime_root(tool, requirement, platform, install_store, registry)
    else {
        return Err(CoreError::message(format!(
            "{}@{} is selected but not installed or registered.\nRun `devenv add {} <path>` for an existing runtime, `devenv install {}@{}` for a DevEnv-owned runtime, or `devenv list {}` to inspect known runtimes.",
            tool,
            requirement.raw(),
            tool,
            tool,
            requirement.raw(),
            tool
        )));
    };

    adapter.activation_plan(&runtime_root)
}

pub fn add_external_runtime(
    registry: &mut dyn RuntimeRegistry,
    runtime: RegisteredRuntime,
) -> CoreResult<()> {
    registry.add_registered_runtime(runtime)
}

pub fn remove_external_runtime(
    registry: &mut dyn RuntimeRegistry,
    tool: &ToolName,
    version: &Version,
    platform: Platform,
    root: Option<&Path>,
) -> CoreResult<Vec<RegisteredRuntime>> {
    registry.remove_registered_runtime(tool, version, platform, root)
}

pub fn uninstall_runtime(
    install_store: &mut dyn InstallStore,
    tool: &ToolName,
    requirement: &VersionRequirement,
    platform: Platform,
    matcher: &dyn VersionMatcher,
) -> CoreResult<Option<InstallationMetadata>> {
    let versions = install_store
        .list_installation_metadata(tool)
        .into_iter()
        .filter(|metadata| metadata.installation().platform() == platform)
        .map(|metadata| metadata.installation().version().clone())
        .collect::<Vec<_>>();
    let Some(version) = matcher.match_version(requirement, &versions)? else {
        return Ok(None);
    };

    install_store.remove_installation_metadata(tool, &version, platform)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRuntimeRequest {
    tool: ToolName,
    version: Version,
    platform: Platform,
    metadata_fields: BTreeMap<String, String>,
}

impl InstallRuntimeRequest {
    pub fn new(tool: ToolName, version: Version, platform: Platform) -> Self {
        Self {
            tool,
            version,
            platform,
            metadata_fields: BTreeMap::new(),
        }
    }

    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn metadata_fields(&self) -> &BTreeMap<String, String> {
        &self.metadata_fields
    }

    pub fn with_metadata_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata_fields.insert(key.into(), value.into());
        self
    }
}

pub struct InstallRuntimePorts<'a> {
    pub artifact_resolver: &'a dyn ArtifactResolver,
    pub downloader: &'a mut dyn Downloader,
    pub checksum_verifier: &'a dyn ChecksumVerifier,
    pub extractor: &'a mut dyn ArchiveExtractor,
    pub transactions: &'a mut dyn InstallTransactionManager,
    pub install_store: &'a mut dyn InstallStore,
    pub lock_manager: &'a mut dyn LockManager,
    pub clock: &'a dyn Clock,
    pub installed_runtime_validator: Option<&'a dyn InstalledRuntimeValidator>,
}

pub fn list_remote_versions(
    tool: &ToolName,
    version_source: &dyn VersionSource,
) -> CoreResult<Vec<Version>> {
    version_source.list_versions(tool)
}

pub fn collect_shim_specs(adapters: &[&dyn ToolAdapter]) -> CoreResult<Vec<ShimSpec>> {
    let mut specs = Vec::new();
    let mut binary_owners = BTreeMap::<String, ToolName>::new();

    for adapter in adapters {
        let tool = adapter.metadata().name().clone();
        for binary in adapter.exposed_binaries() {
            let binary = binary.trim();
            if binary.is_empty() {
                return Err(CoreError::message(format!(
                    "tool `{tool}` exposes an empty shim binary name"
                )));
            }
            if let Some(existing_tool) = binary_owners.get(binary) {
                if existing_tool != &tool {
                    return Err(CoreError::message(format!(
                        "shim binary `{binary}` is exposed by both `{existing_tool}` and `{tool}`"
                    )));
                }
                continue;
            }
            binary_owners.insert(binary.to_owned(), tool.clone());
            specs.push(ShimSpec::new(tool.clone(), binary));
        }
    }

    specs.sort();
    Ok(specs)
}

pub fn tool_for_shim_binary(
    binary_name: &str,
    adapters: &[&dyn ToolAdapter],
) -> CoreResult<Option<ToolName>> {
    Ok(collect_shim_specs(adapters)?
        .into_iter()
        .find(|spec| spec.binary_name() == binary_name)
        .map(|spec| spec.tool().clone()))
}

pub fn rehash_shims(
    adapters: &[&dyn ToolAdapter],
    writer: &mut dyn ShimWriter,
) -> CoreResult<Vec<ShimSpec>> {
    let specs = collect_shim_specs(adapters)?;
    for spec in &specs {
        writer.write_shim(spec)?;
    }

    Ok(specs)
}

pub fn dispatch_shim_command(
    binary_name: &str,
    args: &[String],
    activation: ActivationPlan,
    cwd: &Path,
    environment: &BTreeMap<String, String>,
    runner: &mut dyn CommandRunner,
) -> CoreResult<CommandOutput> {
    if environment
        .get(ACTIVE_SHIM_ENV)
        .is_some_and(|active| active == binary_name)
    {
        return Err(CoreError::message(format!(
            "shim recursion detected for `{binary_name}`; ensure the selected runtime bin directory appears before DevEnv shims in PATH"
        )));
    }

    let activation = activation.set_env(ACTIVE_SHIM_ENV, binary_name);
    let command = ExecCommand::new(binary_name.to_owned(), activation)
        .with_args(args.iter().cloned())
        .with_cwd(cwd);

    command.execute(environment, runner)
}

pub fn install_runtime(
    request: InstallRuntimeRequest,
    mut ports: InstallRuntimePorts<'_>,
) -> CoreResult<InstallationMetadata> {
    let lock_key = install_lock_key(request.tool(), request.version(), request.platform());
    if !ports.lock_manager.acquire(lock_key.clone())? {
        return Err(CoreError::message(format!(
            "install for {}@{} on {} is already in progress",
            request.tool(),
            request.version(),
            request.platform().id()
        )));
    }

    let result = install_runtime_with_lock(&request, &mut ports);
    let release_result = ports.lock_manager.release(&lock_key);

    match (result, release_result) {
        (Ok(metadata), Ok(())) => Ok(metadata),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(release_error)) => Err(CoreError::message(format!(
            "{error}; additionally failed to release install lock `{}`: {release_error}",
            lock_key.as_str()
        ))),
    }
}

pub fn install_lock_key(tool: &ToolName, version: &Version, platform: Platform) -> LockKey {
    LockKey::new(format!(
        "install:{}:{}:{}",
        tool.as_str(),
        version.raw(),
        platform.id()
    ))
}

pub fn validate_archive_manifest(manifest: &ExtractionManifest) -> CoreResult<()> {
    for entry in manifest.entries() {
        validate_archive_entry_path(entry)?;
    }

    Ok(())
}

fn selected_runtime_root(
    tool: &ToolName,
    requirement: &VersionRequirement,
    platform: Platform,
    install_store: &dyn InstallStore,
    registry: &dyn RuntimeRegistry,
) -> Option<PathBuf> {
    install_store
        .list_installations(tool)
        .into_iter()
        .find(|runtime| {
            runtime.platform() == platform && runtime.version().raw() == requirement.raw()
        })
        .map(|runtime| runtime.root().to_path_buf())
        .or_else(|| {
            registry
                .list_registered_runtimes(tool)
                .into_iter()
                .find(|runtime| {
                    runtime.platform() == platform && runtime.version().raw() == requirement.raw()
                })
                .map(|runtime| runtime.root().to_path_buf())
        })
}

fn install_runtime_with_lock(
    request: &InstallRuntimeRequest,
    ports: &mut InstallRuntimePorts<'_>,
) -> CoreResult<InstallationMetadata> {
    let artifact = ports.artifact_resolver.resolve_artifact(
        request.tool(),
        request.version(),
        request.platform(),
    )?;
    let install_root =
        ports
            .transactions
            .install_root(request.tool(), request.version(), request.platform());
    let plan = InstallPlan::new(
        request.tool().clone(),
        request.version().clone(),
        request.platform(),
        artifact,
        install_root,
    );
    let transaction = ports.transactions.begin(&plan)?;
    let result = run_install_transaction(request, &plan, &transaction, ports);
    let cleanup_result = ports.transactions.cleanup(&transaction);

    match (result, cleanup_result) {
        (Ok(metadata), Ok(())) => Ok(metadata),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => Err(CoreError::message(format!(
            "{error}; additionally failed to clean install temp `{}`: {cleanup_error}",
            transaction.temp_root().display()
        ))),
    }
}

fn run_install_transaction(
    request: &InstallRuntimeRequest,
    plan: &InstallPlan,
    transaction: &InstallTransaction,
    ports: &mut InstallRuntimePorts<'_>,
) -> CoreResult<InstallationMetadata> {
    let artifact = plan.artifact();
    let downloaded = ports
        .downloader
        .download(artifact, transaction.download_path())?;

    if let Some(expected_size) = artifact.size() {
        if downloaded.size() != expected_size {
            return Err(CoreError::message(format!(
                "downloaded artifact `{}` has size {} but expected {}",
                downloaded.path().display(),
                downloaded.size(),
                expected_size
            )));
        }
    }

    if let Some(expected_checksum) = artifact.checksum() {
        ports
            .checksum_verifier
            .verify(downloaded.path(), expected_checksum)?;
    }

    let manifest = ports.extractor.extract(
        downloaded.path(),
        transaction.extract_root(),
        artifact.archive_type(),
    )?;
    validate_archive_manifest(&manifest)?;
    if let Some(validator) = ports.installed_runtime_validator {
        validator.validate(transaction.extract_root())?;
    }
    ports.transactions.commit(transaction)?;

    let installation = Installation::new(
        request.tool().clone(),
        request.version().clone(),
        request.platform(),
        transaction.install_root(),
    );
    let mut metadata = InstallationMetadata::new(
        installation,
        artifact.url().to_owned(),
        artifact.checksum().map(ToOwned::to_owned),
        ports.clock.now_utc()?,
    );
    for (key, value) in request.metadata_fields() {
        metadata = metadata.with_metadata_field(key.clone(), value.clone());
    }
    ports
        .install_store
        .add_installation_metadata(metadata.clone())?;

    Ok(metadata)
}

fn validate_archive_entry_path(path: &Path) -> CoreResult<()> {
    if path.as_os_str().is_empty() {
        return Err(CoreError::message("unsafe archive entry: empty path"));
    }

    if path.is_absolute() {
        return Err(CoreError::message(format!(
            "unsafe archive entry `{}`: absolute paths are not allowed",
            path.display()
        )));
    }

    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(CoreError::message(format!(
                    "unsafe archive entry `{}`: parent directory components are not allowed",
                    path.display()
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::message(format!(
                    "unsafe archive entry `{}`: absolute paths are not allowed",
                    path.display()
                )));
            }
        }
    }

    Ok(())
}
