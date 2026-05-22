use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, CoreError, CoreResult, InstallStore, Installation, Platform, RegisteredRuntime,
    RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version, VersionMatcher,
    VersionRequirement, VersionScheme,
};

#[derive(Debug, Clone)]
pub struct PhpToolAdapter {
    metadata: ToolMetadata,
}

impl PhpToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                php_tool_name(),
                VersionScheme::Custom("php".to_owned()),
                vec![
                    "php".to_owned(),
                    "phpize".to_owned(),
                    "php-config".to_owned(),
                ],
            ),
        }
    }
}

impl Default for PhpToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for PhpToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_php_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhpVersionMatcher;

impl VersionMatcher for PhpVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_php_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhpRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhpRuntime {
    version: Version,
    root: PathBuf,
    source: PhpRuntimeSource,
    platform: Option<Platform>,
}

impl PhpRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        source: PhpRuntimeSource,
        platform: Option<Platform>,
    ) -> Self {
        Self {
            version,
            root: root.into(),
            source,
            platform,
        }
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> &PhpRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhpRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl PhpRuntimeDiscovery {
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
    ) -> CoreResult<Vec<PhpRuntime>> {
        let php = php_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&php) {
            if runtime.platform() == platform {
                runtimes.push(php_runtime_from_registered(runtime)?);
            }
        }

        for installation in install_store.list_installations(&php) {
            if installation.platform() == platform {
                runtimes.push(php_runtime_from_installation(installation)?);
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

pub fn validate_php_home(root: impl AsRef<Path>) -> CoreResult<PhpRuntime> {
    let root = canonical_php_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid PHP runtime `{}`: expected a PHP runtime directory",
            root.display()
        )));
    }

    for binary in ["php", "phpize", "php-config"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid PHP runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_php_version(&root)?;

    Ok(PhpRuntime::new(
        Version::new(version)?,
        root,
        PhpRuntimeSource::CandidatePath,
        None,
    ))
}

pub fn match_php_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [PhpRuntime],
) -> CoreResult<Option<&'a PhpRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_php_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_php_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = PhpVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = PhpVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_php_version(value: &str) -> CoreResult<String> {
    Ok(PhpVersionKey::parse(value)?.normalized)
}

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<PhpRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_php_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan PHP candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan PHP candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_php_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn php_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<PhpRuntime> {
    let root = canonical_php_home(runtime.root()).unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(PhpRuntime::new(
        runtime.version().clone(),
        root,
        PhpRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn php_runtime_from_installation(installation: Installation) -> CoreResult<PhpRuntime> {
    let root = canonical_php_home(installation.root())?;
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_php_home(&root)?.version().clone()
    };

    Ok(PhpRuntime::new(
        version,
        root,
        PhpRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_php_home(root: &Path) -> CoreResult<PathBuf> {
    if root.join("bin/php").is_file() {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan PHP runtime `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan PHP runtime `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.join("bin/php").is_file() {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn read_php_version(root: &Path) -> CoreResult<String> {
    let version_path = root.join("VERSION");
    if version_path.is_file() {
        let version = std::fs::read_to_string(&version_path).map_err(|error| {
            CoreError::message(format!(
                "invalid PHP runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                version_path.display()
            ))
        })?;
        return first_version_line(root, &version_path, &version);
    }

    let header_path = root.join("include/main/php_version.h");
    if header_path.is_file() {
        let header = std::fs::read_to_string(&header_path).map_err(|error| {
            CoreError::message(format!(
                "invalid PHP runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                header_path.display()
            ))
        })?;
        for line in header.lines() {
            if line.contains("PHP_VERSION") {
                let Some((_, value)) = line.split_once('"') else {
                    continue;
                };
                let Some((version, _)) = value.split_once('"') else {
                    continue;
                };
                return normalize_php_version(version);
            }
        }
    }

    if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
        if let Ok(version) = normalize_php_version(name) {
            return Ok(version);
        }
    }

    Err(CoreError::message(format!(
        "invalid PHP runtime `{}`: missing version metadata. Expected VERSION, include/main/php_version.h, or a versioned runtime directory name.",
        root.display()
    )))
}

fn first_version_line(root: &Path, path: &Path, contents: &str) -> CoreResult<String> {
    let version = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid PHP runtime `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_php_version(version)
}

fn compare_php_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = PhpVersionKey::parse(left.raw());
    let right_key = PhpVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left), Ok(right)) => right.cmp(&left),
        _ => right.raw().cmp(left.raw()),
    }
}

fn runtime_sort(left: &PhpRuntime, right: &PhpRuntime) -> Ordering {
    compare_php_version_desc(left.version(), right.version())
        .then_with(|| left.root().cmp(right.root()))
}

fn php_tool_name() -> ToolName {
    ToolName::new("php").expect("built-in PHP tool name should be valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhpVersionKey {
    normalized: String,
    parts: Vec<u64>,
}

impl PhpVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let normalized = version_token(value).ok_or_else(|| {
            CoreError::message(format!(
                "invalid PHP version `{}`: expected a numeric version such as 8.3.7",
                value.trim()
            ))
        })?;
        let numeric = normalized
            .split(['-', '+'])
            .next()
            .unwrap_or(normalized.as_str());
        let parts = numeric
            .split('.')
            .map(|part| {
                part.parse::<u64>().map_err(|error| {
                    CoreError::message(format!("invalid PHP version `{normalized}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        if parts.is_empty() {
            return Err(CoreError::message(format!(
                "invalid PHP version `{normalized}`: expected a numeric version"
            )));
        }

        Ok(Self { normalized, parts })
    }

    fn matches_requirement(&self, requirement: &PhpVersionKey) -> bool {
        self.normalized == requirement.normalized || self.parts.starts_with(&requirement.parts)
    }
}

impl Ord for PhpVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts
            .cmp(&other.parts)
            .then_with(|| self.normalized.cmp(&other.normalized))
    }
}

impl PartialOrd for PhpVersionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn version_token(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('v');
    let start = value
        .char_indices()
        .find_map(|(index, character)| character.is_ascii_digit().then_some(index))?;
    let token = value[start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric()
                || *character == '.'
                || *character == '-'
                || *character == '+'
        })
        .collect::<String>();

    (!token.is_empty()).then_some(token)
}
