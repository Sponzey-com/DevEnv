use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, CoreError, CoreResult, InstallStore, Installation, RegisteredRuntime,
    RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version, VersionMatcher,
    VersionRequirement, VersionScheme,
};

#[derive(Debug, Clone)]
pub struct RustToolAdapter {
    metadata: ToolMetadata,
}

impl RustToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                rust_tool_name(),
                VersionScheme::Custom("rust".to_owned()),
                vec!["rustc".to_owned(), "cargo".to_owned()],
            ),
        }
    }
}

impl Default for RustToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for RustToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_rust_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RustVersionMatcher;

impl VersionMatcher for RustVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_rust_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustRuntimeSource {
    Registered,
    Installed,
    Rustup,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustRuntime {
    version: Version,
    root: PathBuf,
    source: RustRuntimeSource,
}

impl RustRuntime {
    pub fn new(version: Version, root: impl Into<PathBuf>, source: RustRuntimeSource) -> Self {
        Self {
            version,
            root: root.into(),
            source,
        }
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> &RustRuntimeSource {
        &self.source
    }

    fn with_source(mut self, source: RustRuntimeSource) -> Self {
        self.source = source;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct RustRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
    rustup_homes: Vec<PathBuf>,
}

impl RustRuntimeDiscovery {
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

    pub fn with_rustup_home(mut self, root: impl Into<PathBuf>) -> Self {
        self.rustup_homes.push(root.into());
        self
    }

    pub fn with_rustup_homes<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.rustup_homes.extend(roots.into_iter().map(Into::into));
        self
    }

    pub fn discover(
        &self,
        registry: &dyn RuntimeRegistry,
        install_store: &dyn InstallStore,
    ) -> CoreResult<Vec<RustRuntime>> {
        let rust = rust_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&rust) {
            runtimes.push(rust_runtime_from_registered(runtime)?);
        }

        for installation in install_store.list_installations(&rust) {
            runtimes.push(rust_runtime_from_installation(installation)?);
        }

        for candidate in &self.candidate_roots {
            runtimes.extend(discover_candidate_root(
                candidate,
                RustRuntimeSource::CandidatePath,
            )?);
        }

        for rustup_home in &self.rustup_homes {
            runtimes.extend(discover_rustup_home(rustup_home)?);
        }

        runtimes.sort_by(runtime_sort);
        runtimes.dedup_by(|left, right| left.root == right.root);

        Ok(runtimes)
    }
}

pub fn validate_rust_toolchain_home(root: impl AsRef<Path>) -> CoreResult<RustRuntime> {
    let root = canonical_rust_toolchain_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Rust toolchain `{}`: expected a Rust toolchain directory",
            root.display()
        )));
    }

    for binary in ["rustc", "cargo"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Rust toolchain `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_rust_version(&root)?;

    Ok(RustRuntime::new(
        Version::new(version)?,
        root,
        RustRuntimeSource::CandidatePath,
    ))
}

pub fn match_rust_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [RustRuntime],
) -> CoreResult<Option<&'a RustRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_rust_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_rust_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = RustVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = RustVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_rust_version(value: &str) -> CoreResult<String> {
    Ok(RustVersionKey::parse(value)?.to_normalized_string())
}

fn discover_candidate_root(root: &Path, source: RustRuntimeSource) -> CoreResult<Vec<RustRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_rust_toolchain_home(root) {
        return Ok(vec![runtime.with_source(source)]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();

    let toolchains = root.join("toolchains");
    if toolchains.is_dir() {
        runtimes.extend(discover_toolchain_dir(
            &toolchains,
            RustRuntimeSource::Rustup,
        )?);
    }

    runtimes.extend(discover_toolchain_dir(root, source)?);

    Ok(runtimes)
}

fn discover_rustup_home(root: &Path) -> CoreResult<Vec<RustRuntime>> {
    discover_toolchain_dir(&root.join("toolchains"), RustRuntimeSource::Rustup)
}

fn discover_toolchain_dir(root: &Path, source: RustRuntimeSource) -> CoreResult<Vec<RustRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Rust toolchain directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Rust toolchain directory `{}`: {error}",
                root.display()
            ))
        })?;
        if let Ok(runtime) = validate_rust_toolchain_home(entry.path()) {
            runtimes.push(runtime.with_source(source.clone()));
        }
    }

    Ok(runtimes)
}

fn rust_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<RustRuntime> {
    let root = canonical_rust_toolchain_home(runtime.root())
        .unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(RustRuntime::new(
        runtime.version().clone(),
        root,
        RustRuntimeSource::Registered,
    ))
}

fn rust_runtime_from_installation(installation: Installation) -> CoreResult<RustRuntime> {
    let root = canonical_rust_toolchain_home(installation.root())?;
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_rust_toolchain_home(&root)?.version().clone()
    };

    Ok(RustRuntime::new(
        version,
        root,
        RustRuntimeSource::Installed,
    ))
}

fn canonical_rust_toolchain_home(root: &Path) -> CoreResult<PathBuf> {
    if looks_like_rust_toolchain(root) {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Rust toolchain `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Rust toolchain `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if looks_like_rust_toolchain(&path) {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn looks_like_rust_toolchain(root: &Path) -> bool {
    root.join("bin/rustc").is_file() && root.join("bin/cargo").is_file()
}

fn read_rust_version(root: &Path) -> CoreResult<String> {
    let version_path = root.join("VERSION");
    if version_path.is_file() {
        let version = std::fs::read_to_string(&version_path).map_err(|error| {
            CoreError::message(format!(
                "invalid Rust toolchain `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                version_path.display()
            ))
        })?;
        return first_version_line(root, &version_path, &version);
    }

    if let Some(name) = root.file_name().and_then(|value| value.to_str()) {
        if let Ok(version) = normalize_rust_version(name) {
            return Ok(version);
        }
        let name = name.to_ascii_lowercase();
        if name.starts_with("stable-") || name.starts_with("beta-") || name.starts_with("nightly-")
        {
            return Err(CoreError::message(format!(
                "unsupported rustup toolchain `{}`: channel-style toolchains need explicit version metadata because DevEnv does not execute rustup or rustc during default discovery. Add a VERSION file or register a versioned toolchain path.",
                root.display()
            )));
        }
    }

    Err(CoreError::message(format!(
        "invalid Rust toolchain `{}`: missing version metadata. Expected `VERSION` or a versioned rustup toolchain directory name such as `1.85.0-aarch64-apple-darwin`.",
        root.display()
    )))
}

fn first_version_line(root: &Path, path: &Path, input: &str) -> CoreResult<String> {
    let version = input
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Rust toolchain `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_rust_version(version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RustVersionKey {
    components: Vec<u32>,
}

impl RustVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let Some(version) = extract_version_pattern(value) else {
            return Err(CoreError::message(format!(
                "invalid Rust version `{}`: expected a numeric version such as 1.85.0",
                value.trim()
            )));
        };
        let components = version
            .split('.')
            .map(|component| {
                component.parse::<u32>().map_err(|error| {
                    CoreError::message(format!("invalid Rust version `{version}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(Self { components })
    }

    fn matches_requirement(&self, requirement: &RustVersionKey) -> bool {
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

impl Ord for RustVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components.cmp(&other.components)
    }
}

impl PartialOrd for RustVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn extract_version_pattern(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    if value
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
        && value.chars().any(|character| character.is_ascii_digit())
    {
        return Some(value.to_owned());
    }

    let chars = value.chars().collect::<Vec<_>>();
    for start in 0..chars.len() {
        if !chars[start].is_ascii_digit() {
            continue;
        }
        let mut end = start;
        let mut dot_count = 0;
        while end < chars.len() && (chars[end].is_ascii_digit() || chars[end] == '.') {
            if chars[end] == '.' {
                dot_count += 1;
            }
            end += 1;
        }
        if dot_count > 0 {
            let candidate = chars[start..end].iter().collect::<String>();
            if candidate.split('.').all(|component| {
                !component.is_empty() && component.chars().all(|c| c.is_ascii_digit())
            }) {
                return Some(candidate);
            }
        }
    }

    None
}

fn runtime_sort(left: &RustRuntime, right: &RustRuntime) -> Ordering {
    left.root.cmp(&right.root)
}

fn rust_tool_name() -> ToolName {
    ToolName::new("rust").expect("built-in Rust tool name should be valid")
}
