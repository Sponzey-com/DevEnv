use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use devenv_core::{
    CoreError, CoreResult, InstallStore, Installation, InstallationMetadata, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolName, Version,
};

pub const DEVENV_HOME_ENV: &str = "DEVENV_HOME";
const REGISTRY_FILE: &str = "external-runtimes.toml";
const INSTALL_METADATA_FILE: &str = "devenv-install.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevEnvHome {
    root: PathBuf,
}

impl DevEnvHome {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn resolve_from_env(environment: &BTreeMap<String, String>) -> CoreResult<Self> {
        if let Some(root) = environment
            .get(DEVENV_HOME_ENV)
            .filter(|value| !value.is_empty())
        {
            return Ok(Self::new(root));
        }

        Ok(Self::new(default_data_dir(environment)?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn installs_dir(&self) -> PathBuf {
        self.root.join("installs")
    }

    pub fn registry_dir(&self) -> PathBuf {
        self.root.join("registry")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn metadata_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("metadata")
    }

    pub fn download_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("downloads")
    }

    pub fn shims_dir(&self) -> PathBuf {
        self.root.join("shims")
    }

    pub fn global_config_file(&self) -> PathBuf {
        self.root.join("devenv.toml")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn external_registry_file(&self) -> PathBuf {
        self.registry_dir().join(REGISTRY_FILE)
    }

    pub fn create_layout(&self) -> CoreResult<()> {
        for directory in [
            self.installs_dir(),
            self.registry_dir(),
            self.metadata_cache_dir(),
            self.download_cache_dir(),
            self.downloads_dir(),
            self.shims_dir(),
            self.state_dir(),
            self.logs_dir(),
        ] {
            std::fs::create_dir_all(&directory).map_err(|error| {
                CoreError::message(format!(
                    "failed to create DevEnv directory `{}`: {error}",
                    directory.display()
                ))
            })?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FileRuntimeRegistry {
    path: PathBuf,
}

impl FileRuntimeRegistry {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn at_home(home: &DevEnvHome) -> Self {
        Self::new(home.external_registry_file())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list_all(&self) -> CoreResult<Vec<RegisteredRuntime>> {
        read_registered_runtimes(&self.path)
    }
}

impl RuntimeRegistry for FileRuntimeRegistry {
    fn add_registered_runtime(&mut self, runtime: RegisteredRuntime) -> CoreResult<()> {
        let mut runtimes = self.list_all()?;
        runtimes.retain(|existing| !same_runtime(existing, &runtime));
        runtimes.push(runtime);
        runtimes.sort_by_key(runtime_sort_key);
        write_registered_runtimes(&self.path, &runtimes)
    }

    fn remove_registered_runtime(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
        root: Option<&Path>,
    ) -> CoreResult<Vec<RegisteredRuntime>> {
        let mut removed = Vec::new();
        let mut runtimes = self.list_all()?;
        runtimes.retain(|runtime| {
            let matches = runtime.tool() == tool
                && runtime.version() == version
                && runtime.platform() == platform
                && root.is_none_or(|root| runtime.root() == root);

            if matches {
                removed.push(runtime.clone());
            }

            !matches
        });

        write_registered_runtimes(&self.path, &runtimes)?;
        Ok(removed)
    }

    fn list_registered_runtimes(&self, tool: &ToolName) -> Vec<RegisteredRuntime> {
        self.list_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|runtime| runtime.tool() == tool)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct FileInstallStore {
    installs_dir: PathBuf,
}

impl FileInstallStore {
    pub fn new(installs_dir: impl Into<PathBuf>) -> Self {
        Self {
            installs_dir: installs_dir.into(),
        }
    }

    pub fn at_home(home: &DevEnvHome) -> Self {
        Self::new(home.installs_dir())
    }

    pub fn installs_dir(&self) -> &Path {
        &self.installs_dir
    }

    pub fn install_root(&self, tool: &ToolName, version: &Version, platform: Platform) -> PathBuf {
        self.installs_dir
            .join(tool.as_str())
            .join(version.raw())
            .join(platform_id(platform))
    }

    pub fn add_installation_metadata(&mut self, metadata: InstallationMetadata) -> CoreResult<()> {
        let root = metadata.installation().root();
        self.ensure_owned_install_root(root)?;

        std::fs::create_dir_all(root).map_err(|error| {
            CoreError::message(format!(
                "failed to create install root `{}`: {error}",
                root.display()
            ))
        })?;
        write_install_metadata(&metadata)
    }

    pub fn read_installation_metadata(
        &self,
        root: impl AsRef<Path>,
    ) -> CoreResult<InstallationMetadata> {
        read_install_metadata(root.as_ref().join(INSTALL_METADATA_FILE))
    }

    pub fn remove_installation_root(&mut self, root: impl AsRef<Path>) -> CoreResult<()> {
        let root = root.as_ref();

        if root.exists() {
            let canonical_root = self.ensure_owned_existing_install_root(root)?;
            std::fs::remove_dir_all(&canonical_root).map_err(|error| {
                CoreError::message(format!(
                    "failed to remove owned install `{}`: {error}",
                    canonical_root.display()
                ))
            })?;
        } else {
            self.ensure_owned_install_root(root)?;
        }

        Ok(())
    }

    fn ensure_owned_install_root(&self, root: &Path) -> CoreResult<()> {
        if root.starts_with(&self.installs_dir) && root != self.installs_dir {
            return Ok(());
        }

        let installs_dir = canonical_owned_boundary(&self.installs_dir)?;
        let root = canonical_for_safety(root);

        if !root.starts_with(&installs_dir) || root == installs_dir {
            return Err(CoreError::message(format!(
                "refusing to modify `{}` because it is outside DevEnv-owned installs `{}`",
                root.display(),
                installs_dir.display()
            )));
        }

        Ok(())
    }

    fn ensure_owned_existing_install_root(&self, root: &Path) -> CoreResult<PathBuf> {
        let installs_dir = canonical_owned_boundary(&self.installs_dir)?;
        let root = root.canonicalize().map_err(|error| {
            CoreError::message(format!(
                "failed to resolve owned install `{}` before removal: {error}",
                root.display()
            ))
        })?;

        if !root.starts_with(&installs_dir) || root == installs_dir {
            return Err(CoreError::message(format!(
                "refusing to modify `{}` because it is outside DevEnv-owned installs `{}`",
                root.display(),
                installs_dir.display()
            )));
        }

        Ok(root)
    }
}

impl InstallStore for FileInstallStore {
    fn add_installation(&mut self, installation: Installation) -> CoreResult<()> {
        let metadata = InstallationMetadata::new(installation, "unknown", None, "unknown");
        FileInstallStore::add_installation_metadata(self, metadata)
    }

    fn list_installations(&self, tool: &ToolName) -> Vec<Installation> {
        FileInstallStore::list_installation_metadata(self, tool)
            .unwrap_or_default()
            .into_iter()
            .map(|metadata| metadata.installation().clone())
            .collect()
    }

    fn add_installation_metadata(&mut self, metadata: InstallationMetadata) -> CoreResult<()> {
        FileInstallStore::add_installation_metadata(self, metadata)
    }

    fn list_installation_metadata(&self, tool: &ToolName) -> Vec<InstallationMetadata> {
        FileInstallStore::list_installation_metadata(self, tool).unwrap_or_default()
    }

    fn remove_installation_metadata(
        &mut self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Option<InstallationMetadata>> {
        let Some(metadata) = FileInstallStore::list_installation_metadata(self, tool)?
            .into_iter()
            .find(|metadata| {
                let installation = metadata.installation();
                installation.version() == version && installation.platform() == platform
            })
        else {
            return Ok(None);
        };

        self.remove_installation_root(metadata.installation().root())?;
        Ok(Some(metadata))
    }
}

impl FileInstallStore {
    pub fn list_installation_metadata(
        &self,
        tool: &ToolName,
    ) -> CoreResult<Vec<InstallationMetadata>> {
        let tool_dir = self.installs_dir.join(tool.as_str());
        if !tool_dir.exists() {
            return Ok(Vec::new());
        }

        let mut metadata = Vec::new();
        for version_entry in read_dir(&tool_dir)? {
            let version_entry = version_entry.map_err(|error| {
                CoreError::message(format!(
                    "failed to read install directory `{}`: {error}",
                    tool_dir.display()
                ))
            })?;
            if !version_entry.path().is_dir() {
                continue;
            }
            for platform_entry in read_dir(&version_entry.path())? {
                let platform_entry = platform_entry.map_err(|error| {
                    CoreError::message(format!(
                        "failed to read install directory `{}`: {error}",
                        version_entry.path().display()
                    ))
                })?;
                let metadata_path = platform_entry.path().join(INSTALL_METADATA_FILE);
                if metadata_path.is_file() {
                    metadata.push(read_install_metadata(metadata_path)?);
                }
            }
        }

        Ok(metadata)
    }
}

fn default_data_dir(environment: &BTreeMap<String, String>) -> CoreResult<PathBuf> {
    if cfg!(target_os = "windows") {
        return environment
            .get("LOCALAPPDATA")
            .map(|path| PathBuf::from(path).join("devenv"))
            .ok_or_else(|| {
                CoreError::message("cannot resolve DevEnv home: set DEVENV_HOME or LOCALAPPDATA")
            });
    }

    if cfg!(target_os = "macos") {
        return environment
            .get("HOME")
            .map(|path| PathBuf::from(path).join("Library/Application Support/devenv"))
            .ok_or_else(|| {
                CoreError::message("cannot resolve DevEnv home: set DEVENV_HOME or HOME")
            });
    }

    if let Some(data_home) = environment
        .get("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(data_home).join("devenv"));
    }

    environment
        .get("HOME")
        .map(|path| PathBuf::from(path).join(".local/share/devenv"))
        .ok_or_else(|| CoreError::message("cannot resolve DevEnv home: set DEVENV_HOME or HOME"))
}

fn read_registered_runtimes(path: &Path) -> CoreResult<Vec<RegisteredRuntime>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read runtime registry `{}`: {error}",
            path.display()
        ))
    })?;
    let document = contents.parse::<toml::Value>().map_err(|error| {
        CoreError::message(format!(
            "failed to parse runtime registry `{}`: {error}",
            path.display()
        ))
    })?;
    let Some(entries) = document.get("runtime").and_then(toml::Value::as_array) else {
        return Ok(Vec::new());
    };

    entries
        .iter()
        .map(parse_registered_runtime)
        .collect::<CoreResult<Vec<_>>>()
}

fn parse_registered_runtime(value: &toml::Value) -> CoreResult<RegisteredRuntime> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid runtime registry: expected [[runtime]] table")
    })?;
    let tool = ToolName::new(required_string(table, "tool")?).map_err(CoreError::from)?;
    let version = Version::new(required_string(table, "version")?).map_err(CoreError::from)?;
    let platform = required_string(table, "platform").and_then(parse_platform_id)?;
    let root = required_string(table, "root")?;

    Ok(RegisteredRuntime::new(tool, version, platform, root))
}

fn write_registered_runtimes(path: &Path, runtimes: &[RegisteredRuntime]) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CoreError::message(format!(
                "failed to create runtime registry directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    let mut output = String::new();
    for runtime in runtimes {
        output.push_str("[[runtime]]\n");
        output.push_str(&format!(
            "tool = \"{}\"\n",
            escape_toml(runtime.tool().as_str())
        ));
        output.push_str(&format!(
            "version = \"{}\"\n",
            escape_toml(runtime.version().raw())
        ));
        output.push_str(&format!(
            "platform = \"{}\"\n",
            escape_toml(platform_id(runtime.platform()))
        ));
        output.push_str(&format!(
            "root = \"{}\"\n\n",
            escape_toml(&runtime.root().to_string_lossy())
        ));
    }

    std::fs::write(path, output).map_err(|error| {
        CoreError::message(format!(
            "failed to write runtime registry `{}`: {error}",
            path.display()
        ))
    })
}

fn write_install_metadata(metadata: &InstallationMetadata) -> CoreResult<()> {
    let installation = metadata.installation();
    let path = installation.root().join(INSTALL_METADATA_FILE);
    let mut output = String::new();
    output.push_str("[runtime]\n");
    output.push_str(&format!(
        "tool = \"{}\"\n",
        escape_toml(installation.tool().as_str())
    ));
    output.push_str(&format!(
        "version = \"{}\"\n",
        escape_toml(installation.version().raw())
    ));
    output.push_str(&format!(
        "platform = \"{}\"\n",
        escape_toml(platform_id(installation.platform()))
    ));
    output.push_str(&format!(
        "root = \"{}\"\n",
        escape_toml(&installation.root().to_string_lossy())
    ));
    output.push_str("\n[metadata]\n");
    output.push_str(&format!(
        "source = \"{}\"\n",
        escape_toml(metadata.source())
    ));
    if let Some(checksum) = metadata.checksum() {
        output.push_str(&format!("checksum = \"{}\"\n", escape_toml(checksum)));
    }
    output.push_str(&format!(
        "installed_at = \"{}\"\n",
        escape_toml(metadata.installed_at())
    ));
    for (key, value) in metadata.metadata_fields() {
        output.push_str(&format!("{} = \"{}\"\n", toml_key(key), escape_toml(value)));
    }

    std::fs::write(path, output).map_err(|error| {
        CoreError::message(format!(
            "failed to write install metadata for `{}`: {error}",
            installation.root().display()
        ))
    })
}

fn read_install_metadata(path: impl AsRef<Path>) -> CoreResult<InstallationMetadata> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read install metadata `{}`: {error}",
            path.display()
        ))
    })?;
    let document = contents.parse::<toml::Value>().map_err(|error| {
        CoreError::message(format!(
            "failed to parse install metadata `{}`: {error}",
            path.display()
        ))
    })?;
    let runtime = document
        .get("runtime")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| CoreError::message("invalid install metadata: missing [runtime]"))?;
    let metadata = document
        .get("metadata")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| CoreError::message("invalid install metadata: missing [metadata]"))?;
    let tool = ToolName::new(required_string(runtime, "tool")?).map_err(CoreError::from)?;
    let version = Version::new(required_string(runtime, "version")?).map_err(CoreError::from)?;
    let platform = required_string(runtime, "platform").and_then(parse_platform_id)?;
    let root = required_string(runtime, "root")?;
    let source = required_string(metadata, "source")?;
    let checksum = metadata
        .get("checksum")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    let installed_at = required_string(metadata, "installed_at")?;

    let mut installation_metadata = InstallationMetadata::new(
        Installation::new(tool, version, platform, root),
        source,
        checksum,
        installed_at,
    );
    for (key, value) in metadata {
        if matches!(key.as_str(), "source" | "checksum" | "installed_at") {
            continue;
        }
        if let Some(value) = value.as_str() {
            installation_metadata =
                installation_metadata.with_metadata_field(key.clone(), value.to_owned());
        }
    }

    Ok(installation_metadata)
}

fn required_string(table: &toml::map::Map<String, toml::Value>, key: &str) -> CoreResult<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CoreError::message(format!("invalid metadata: missing string `{key}`")))
}

fn same_runtime(left: &RegisteredRuntime, right: &RegisteredRuntime) -> bool {
    left.tool() == right.tool()
        && left.version() == right.version()
        && left.platform() == right.platform()
        && left.root() == right.root()
}

fn runtime_sort_key(runtime: &RegisteredRuntime) -> (String, String, String, String) {
    (
        runtime.tool().as_str().to_owned(),
        runtime.version().raw().to_owned(),
        platform_id(runtime.platform()).to_owned(),
        runtime.root().to_string_lossy().into_owned(),
    )
}

fn platform_id(platform: Platform) -> &'static str {
    use devenv_core::{Architecture, OperatingSystem};

    match (platform.os(), platform.architecture()) {
        (OperatingSystem::Macos, Architecture::Arm64) => "macos-arm64",
        (OperatingSystem::Macos, Architecture::X64) => "macos-x64",
        (OperatingSystem::Linux, Architecture::Arm64) => "linux-arm64",
        (OperatingSystem::Linux, Architecture::X64) => "linux-x64",
        (OperatingSystem::Windows, Architecture::Arm64) => "windows-arm64",
        (OperatingSystem::Windows, Architecture::X64) => "windows-x64",
    }
}

fn parse_platform_id(value: String) -> CoreResult<Platform> {
    use devenv_core::{Architecture, OperatingSystem};

    match value.as_str() {
        "macos-arm64" => Ok(Platform::new(OperatingSystem::Macos, Architecture::Arm64)),
        "macos-x64" => Ok(Platform::new(OperatingSystem::Macos, Architecture::X64)),
        "linux-arm64" => Ok(Platform::new(OperatingSystem::Linux, Architecture::Arm64)),
        "linux-x64" => Ok(Platform::new(OperatingSystem::Linux, Architecture::X64)),
        "windows-arm64" => Ok(Platform::new(OperatingSystem::Windows, Architecture::Arm64)),
        "windows-x64" => Ok(Platform::new(OperatingSystem::Windows, Architecture::X64)),
        _ => Err(CoreError::message(format!(
            "invalid platform `{value}` in runtime metadata"
        ))),
    }
}

fn canonical_owned_boundary(path: &Path) -> CoreResult<PathBuf> {
    if path.exists() {
        path.canonicalize().map_err(|error| {
            CoreError::message(format!(
                "failed to canonicalize owned installs `{}`: {error}",
                path.display()
            ))
        })
    } else {
        std::fs::create_dir_all(path).map_err(|error| {
            CoreError::message(format!(
                "failed to create owned installs `{}`: {error}",
                path.display()
            ))
        })?;
        path.canonicalize().map_err(|error| {
            CoreError::message(format!(
                "failed to canonicalize owned installs `{}`: {error}",
                path.display()
            ))
        })
    }
}

fn canonical_for_safety(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn read_dir(path: &Path) -> CoreResult<std::fs::ReadDir> {
    std::fs::read_dir(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read directory `{}`: {error}",
            path.display()
        ))
    })
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn toml_key(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        value.to_owned()
    } else {
        format!("\"{}\"", escape_toml(value))
    }
}
