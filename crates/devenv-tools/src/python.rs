use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, Architecture, ArchiveType, Artifact, ArtifactResolver, CoreError, CoreResult,
    InstallStore, Installation, InstalledRuntimeValidator, OperatingSystem, Platform,
    RegisteredRuntime, RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version,
    VersionMatcher, VersionRequirement, VersionScheme, VersionSource,
};

#[derive(Debug, Clone)]
pub struct PythonToolAdapter {
    metadata: ToolMetadata,
}

impl PythonToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                python_tool_name(),
                VersionScheme::Custom("python".to_owned()),
                vec!["python".to_owned(), "python3".to_owned(), "pip".to_owned()],
            ),
        }
    }
}

impl Default for PythonToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for PythonToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_python_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct PythonVersionMatcher;

impl VersionMatcher for PythonVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_python_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonImplementation {
    Cpython,
    Pypy,
    Unknown,
}

impl PythonImplementation {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "cpython" | "python" => Self::Cpython,
            "pypy" | "pypy3" => Self::Pypy,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpython => "cpython",
            Self::Pypy => "pypy",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonRuntime {
    version: Version,
    implementation: PythonImplementation,
    root: PathBuf,
    source: PythonRuntimeSource,
    platform: Option<Platform>,
}

impl PythonRuntime {
    pub fn new(
        version: Version,
        implementation: PythonImplementation,
        root: impl Into<PathBuf>,
        source: PythonRuntimeSource,
        platform: Option<Platform>,
    ) -> Self {
        Self {
            version,
            implementation,
            root: root.into(),
            source,
            platform,
        }
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn implementation(&self) -> PythonImplementation {
        self.implementation
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> &PythonRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct PythonRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl PythonRuntimeDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_candidate_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.candidate_roots.push(root.into());
        self
    }

    pub fn with_candidate_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.candidate_roots
            .extend(roots.into_iter().map(Into::into));
        self
    }

    pub fn discover(
        &self,
        platform: Platform,
        registry: &dyn RuntimeRegistry,
        install_store: &dyn InstallStore,
    ) -> CoreResult<Vec<PythonRuntime>> {
        let python = python_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&python) {
            if runtime.platform() == platform {
                runtimes.push(python_runtime_from_registered(runtime)?);
            }
        }

        for installation in install_store.list_installations(&python) {
            if installation.platform() == platform {
                runtimes.push(python_runtime_from_installation(installation)?);
            }
        }

        for candidate in &self.candidate_roots {
            runtimes.extend(discover_candidate_root(candidate)?);
        }

        runtimes.sort_by(runtime_sort);
        runtimes.dedup_by(|left, right| left.root == right.root);

        Ok(runtimes)
    }
}

pub fn validate_python_home(root: impl AsRef<Path>) -> CoreResult<PythonRuntime> {
    let root = canonical_python_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Python runtime `{}`: expected a Python runtime directory",
            root.display()
        )));
    }

    reject_virtual_environment(&root)?;

    for binary in ["python", "python3", "pip"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Python runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_python_version(&root)?;
    let implementation = read_python_implementation(&root)?;

    Ok(PythonRuntime::new(
        Version::new(version)?,
        implementation,
        root,
        PythonRuntimeSource::CandidatePath,
        None,
    ))
}

#[derive(Debug, Clone, Default)]
pub struct PythonInstalledRuntimeValidator;

impl InstalledRuntimeValidator for PythonInstalledRuntimeValidator {
    fn validate(&self, root: &Path) -> CoreResult<()> {
        validate_python_home(root).map(|_| ())
    }
}

pub fn match_python_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [PythonRuntime],
) -> CoreResult<Option<&'a PythonRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_python_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_python_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = PythonVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = PythonVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_python_version(value: &str) -> CoreResult<String> {
    let key = PythonVersionKey::parse(value)?;
    Ok(key.to_normalized_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonReleaseMetadata {
    releases: Vec<PythonRelease>,
}

impl PythonReleaseMetadata {
    pub fn parse(input: &str) -> CoreResult<Self> {
        let document = input.parse::<toml::Value>().map_err(|error| {
            CoreError::message(format!(
                "failed to parse Python release metadata fixture: {error}"
            ))
        })?;
        let releases = document
            .get("release")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                CoreError::message("invalid Python release metadata: missing [[release]] entries")
            })?
            .iter()
            .map(parse_python_release)
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { releases })
    }

    pub fn releases(&self) -> &[PythonRelease] {
        &self.releases
    }

    fn cpython_releases(&self) -> impl Iterator<Item = &PythonRelease> {
        self.releases
            .iter()
            .filter(|release| release.implementation() == PythonImplementation::Cpython)
    }

    fn release_for_cpython_version(&self, version: &Version) -> CoreResult<&PythonRelease> {
        if let Some(exact) = self
            .cpython_releases()
            .find(|release| release.version().raw() == version.raw())
        {
            return Ok(exact);
        }

        let versions = self
            .cpython_releases()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        let requirement = VersionRequirement::exact(version.raw()).map_err(CoreError::from)?;
        let Some(matched) = match_python_version(&requirement, &versions)? else {
            return Err(CoreError::message(format!(
                "CPython version `{}` was not found in metadata",
                version
            )));
        };

        self.cpython_releases()
            .find(|release| release.version().raw() == matched.raw())
            .ok_or_else(|| {
                CoreError::message(format!(
                    "CPython version `{}` was not found in metadata",
                    version
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonRelease {
    version: Version,
    implementation: PythonImplementation,
    stable: bool,
    files: Vec<PythonReleaseFile>,
}

impl PythonRelease {
    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn implementation(&self) -> PythonImplementation {
        self.implementation
    }

    pub fn stable(&self) -> bool {
        self.stable
    }

    pub fn files(&self) -> &[PythonReleaseFile] {
        &self.files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

impl PythonReleaseFile {
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn os(&self) -> &str {
        &self.os
    }

    pub fn arch(&self) -> &str {
        &self.arch
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct PythonReleaseVersionSource {
    metadata: PythonReleaseMetadata,
}

impl PythonReleaseVersionSource {
    pub fn new(metadata: PythonReleaseMetadata) -> Self {
        Self { metadata }
    }
}

impl VersionSource for PythonReleaseVersionSource {
    fn list_versions(&self, tool: &ToolName) -> CoreResult<Vec<Version>> {
        if tool.as_str() != "python" {
            return Ok(Vec::new());
        }

        let mut versions = self
            .metadata
            .cpython_releases()
            .filter(|release| release.stable())
            .map(|release| release.version().clone())
            .collect::<Vec<_>>();
        versions.sort_by(compare_python_version_desc);
        versions.dedup_by(|left, right| left.raw() == right.raw());

        Ok(versions)
    }
}

#[derive(Debug, Clone)]
pub struct PythonArtifactResolver {
    metadata: PythonReleaseMetadata,
}

impl PythonArtifactResolver {
    pub fn new(metadata: PythonReleaseMetadata) -> Self {
        Self { metadata }
    }

    pub fn resolve_install_version(&self, requirement: &Version) -> CoreResult<Version> {
        Ok(self
            .metadata
            .release_for_cpython_version(requirement)?
            .version()
            .clone())
    }
}

impl ArtifactResolver for PythonArtifactResolver {
    fn resolve_artifact(
        &self,
        tool: &ToolName,
        version: &Version,
        platform: Platform,
    ) -> CoreResult<Artifact> {
        if tool.as_str() != "python" {
            return Err(CoreError::message(format!(
                "Python artifact resolver cannot resolve `{tool}`"
            )));
        }

        let release = self.metadata.release_for_cpython_version(version)?;
        let os = python_artifact_os(platform);
        let arch = python_artifact_arch(platform);
        let file = release
            .files()
            .iter()
            .find(|file| file.kind() == "archive" && file.os() == os && file.arch() == arch)
            .ok_or_else(|| {
                CoreError::message(format!(
                    "CPython {} does not provide an archive for {}",
                    version,
                    platform.id()
                ))
            })?;
        let archive_type = archive_type_for_python_file(file.filename())?;
        let url = file.url().map(ToOwned::to_owned).unwrap_or_else(|| {
            format!(
                "https://www.python.org/ftp/python/{}/{}",
                release.version().raw(),
                file.filename()
            )
        });
        let mut artifact = Artifact::new(
            url,
            file.filename(),
            archive_type,
            file.sha256().map(ToOwned::to_owned),
        );
        if let Some(size) = file.size() {
            artifact = artifact.with_size(size);
        }

        Ok(artifact)
    }
}

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<PythonRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_python_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Python candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Python candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_python_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn python_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<PythonRuntime> {
    let root =
        canonical_python_home(runtime.root()).unwrap_or_else(|_| runtime.root().to_path_buf());
    let implementation = read_python_implementation(&root).unwrap_or(PythonImplementation::Unknown);
    Ok(PythonRuntime::new(
        runtime.version().clone(),
        implementation,
        root,
        PythonRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn python_runtime_from_installation(installation: Installation) -> CoreResult<PythonRuntime> {
    let root = canonical_python_home(installation.root())?;
    let (version, implementation) = if root.as_path() == installation.root() {
        (
            installation.version().clone(),
            read_python_implementation(&root).unwrap_or(PythonImplementation::Unknown),
        )
    } else {
        let runtime = validate_python_home(&root)?;
        (runtime.version().clone(), runtime.implementation())
    };

    Ok(PythonRuntime::new(
        version,
        implementation,
        root,
        PythonRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_python_home(root: &Path) -> CoreResult<PathBuf> {
    if looks_like_python_home(root) {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Python runtime `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Python runtime `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if looks_like_python_home(&path) {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn looks_like_python_home(root: &Path) -> bool {
    root.join("bin/python").is_file() || root.join("bin/python3").is_file()
}

fn reject_virtual_environment(root: &Path) -> CoreResult<()> {
    let marker = root.join("pyvenv.cfg");
    if marker.is_file() {
        return Err(CoreError::message(format!(
            "invalid Python runtime `{}`: virtual environments are not supported by `devenv add python`; register a CPython or PyPy runtime root instead",
            root.display()
        )));
    }

    Ok(())
}

fn read_python_version(root: &Path) -> CoreResult<String> {
    let version_path = root.join("VERSION");
    if version_path.is_file() {
        let version = std::fs::read_to_string(&version_path).map_err(|error| {
            CoreError::message(format!(
                "invalid Python runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                version_path.display()
            ))
        })?;
        return first_version_line(root, &version_path, &version);
    }

    if let Some(version) = read_patchlevel_version(root)? {
        return Ok(version);
    }

    Err(CoreError::message(format!(
        "invalid Python runtime `{}`: missing version metadata. Expected `VERSION` or `include/python*/patchlevel.h`.",
        root.display()
    )))
}

fn read_patchlevel_version(root: &Path) -> CoreResult<Option<String>> {
    let include = root.join("include");
    if !include.is_dir() {
        return Ok(None);
    }

    for entry in std::fs::read_dir(&include).map_err(|error| {
        CoreError::message(format!(
            "invalid Python runtime `{}`: failed to scan `{}` for version metadata: {error}",
            root.display(),
            include.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "invalid Python runtime `{}`: failed to scan `{}` for version metadata: {error}",
                root.display(),
                include.display()
            ))
        })?;
        let path = entry.path().join("patchlevel.h");
        if !path.is_file() {
            continue;
        }
        let header = std::fs::read_to_string(&path).map_err(|error| {
            CoreError::message(format!(
                "invalid Python runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                path.display()
            ))
        })?;
        for line in header.lines() {
            if line.contains("PY_VERSION") {
                let Some((_, value)) = line.split_once('"') else {
                    continue;
                };
                let Some((version, _)) = value.split_once('"') else {
                    continue;
                };
                return Ok(Some(normalize_python_version(version)?));
            }
        }
    }

    Ok(None)
}

fn read_python_implementation(root: &Path) -> CoreResult<PythonImplementation> {
    for filename in ["IMPLEMENTATION", "PYTHON_IMPLEMENTATION"] {
        let path = root.join(filename);
        if !path.is_file() {
            continue;
        }
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            CoreError::message(format!(
                "invalid Python runtime `{}`: failed to read `{}` for implementation metadata: {error}",
                root.display(),
                path.display()
            ))
        })?;
        let implementation = contents.lines().next().map(str::trim).unwrap_or("");
        return Ok(PythonImplementation::parse(implementation));
    }

    let Some(name) = root.file_name().and_then(|value| value.to_str()) else {
        return Ok(PythonImplementation::Unknown);
    };
    let name = name.to_ascii_lowercase();
    if name.contains("pypy") {
        Ok(PythonImplementation::Pypy)
    } else if name.contains("cpython") || name.contains("python") {
        Ok(PythonImplementation::Cpython)
    } else {
        Ok(PythonImplementation::Unknown)
    }
}

fn first_version_line(root: &Path, path: &Path, input: &str) -> CoreResult<String> {
    let version = input
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Python runtime `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_python_version(version)
}

fn parse_python_release(value: &toml::Value) -> CoreResult<PythonRelease> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Python release metadata: release must be a table")
    })?;
    let version = normalize_python_version(required_string(table, "version")?)?;
    let implementation = table
        .get("implementation")
        .and_then(toml::Value::as_str)
        .map(PythonImplementation::parse)
        .unwrap_or(PythonImplementation::Cpython);
    let stable = table
        .get("stable")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    let files = table
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Python release metadata: release `{version}` has no [[release.file]] entries"
            ))
        })?
        .iter()
        .map(parse_python_release_file)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(PythonRelease {
        version: Version::new(version)?,
        implementation,
        stable,
        files,
    })
}

fn parse_python_release_file(value: &toml::Value) -> CoreResult<PythonReleaseFile> {
    let table = value.as_table().ok_or_else(|| {
        CoreError::message("invalid Python release metadata: release file must be a table")
    })?;
    let filename = required_string(table, "filename")?.to_owned();
    let os = required_string(table, "os")?.to_owned();
    let arch = required_string(table, "arch")?.to_owned();
    let kind = table
        .get("kind")
        .and_then(toml::Value::as_str)
        .unwrap_or("archive")
        .to_owned();
    let sha256 = table
        .get("sha256")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);
    let size = table
        .get("size")
        .and_then(toml::Value::as_integer)
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                CoreError::message(format!(
                    "invalid Python release metadata: size for `{filename}` must be non-negative"
                ))
            })
        })
        .transpose()?;
    let url = table
        .get("url")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);

    Ok(PythonReleaseFile {
        filename,
        os,
        arch,
        kind,
        sha256,
        size,
        url,
    })
}

fn required_string<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
) -> CoreResult<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CoreError::message(format!("invalid Python release metadata: missing `{key}`"))
        })
}

fn python_artifact_os(platform: Platform) -> &'static str {
    match platform.os() {
        OperatingSystem::Macos => "macos",
        OperatingSystem::Linux => "linux",
        OperatingSystem::Windows => "windows",
    }
}

fn python_artifact_arch(platform: Platform) -> &'static str {
    match platform.architecture() {
        Architecture::X64 => "x64",
        Architecture::Arm64 => "arm64",
    }
}

fn archive_type_for_python_file(filename: &str) -> CoreResult<ArchiveType> {
    if filename.ends_with(".tar.gz") {
        Ok(ArchiveType::TarGz)
    } else if filename.ends_with(".zip") {
        Ok(ArchiveType::Zip)
    } else {
        Err(CoreError::message(format!(
            "unsupported Python archive `{filename}`: expected .tar.gz or .zip"
        )))
    }
}

fn compare_python_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = PythonVersionKey::parse(left.raw());
    let right_key = PythonVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left_key), Ok(right_key)) => right_key.cmp(&left_key),
        _ => right.raw().cmp(left.raw()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonVersionKey {
    components: Vec<u32>,
}

impl PythonVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let mut value = value.trim();
        if let Some(stripped) = value.strip_prefix('v') {
            value = stripped;
        }

        let mut components = Vec::new();
        let mut current = String::new();
        let mut seen_digit = false;
        for character in value.chars() {
            if character.is_ascii_digit() {
                seen_digit = true;
                current.push(character);
            } else if !current.is_empty() {
                components.push(current.parse::<u32>().map_err(|error| {
                    CoreError::message(format!("invalid Python version `{value}`: {error}"))
                })?);
                current.clear();
                if character == '-' || character == '+' || character.is_ascii_alphabetic() {
                    break;
                }
            } else if seen_digit && (character == '-' || character == '+') {
                break;
            }
        }

        if !current.is_empty() {
            components.push(current.parse::<u32>().map_err(|error| {
                CoreError::message(format!("invalid Python version `{value}`: {error}"))
            })?);
        }

        if components.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Python version `{value}`: expected a numeric version"
            )));
        }

        Ok(Self { components })
    }

    fn matches_requirement(&self, requirement: &PythonVersionKey) -> bool {
        self.components.starts_with(&requirement.components)
    }

    fn to_normalized_string(&self) -> String {
        self.components
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl Ord for PythonVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components.cmp(&other.components)
    }
}

impl PartialOrd for PythonVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn runtime_sort(left: &PythonRuntime, right: &PythonRuntime) -> Ordering {
    left.root.cmp(&right.root)
}

fn python_tool_name() -> ToolName {
    ToolName::new("python").expect("built-in Python tool name should be valid")
}
