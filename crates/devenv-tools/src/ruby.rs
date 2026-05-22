use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use devenv_core::{
    ActivationPlan, CoreError, CoreResult, InstallStore, Installation, Platform, RegisteredRuntime,
    RuntimeRegistry, ToolAdapter, ToolMetadata, ToolName, Version, VersionMatcher,
    VersionRequirement, VersionScheme,
};

#[derive(Debug, Clone)]
pub struct RubyToolAdapter {
    metadata: ToolMetadata,
}

impl RubyToolAdapter {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata::new(
                ruby_tool_name(),
                VersionScheme::Custom("ruby".to_owned()),
                vec!["ruby".to_owned(), "gem".to_owned(), "bundle".to_owned()],
            ),
        }
    }
}

impl Default for RubyToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAdapter for RubyToolAdapter {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn resolve_version(&self, requirement: &VersionRequirement) -> CoreResult<Option<Version>> {
        Ok(Some(Version::new(normalize_ruby_version(
            requirement.raw(),
        )?)?))
    }

    fn activation_plan(&self, runtime_root: &Path) -> CoreResult<ActivationPlan> {
        Ok(ActivationPlan::new().prepend_path(runtime_root.join("bin")))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RubyVersionMatcher;

impl VersionMatcher for RubyVersionMatcher {
    fn match_version(
        &self,
        requirement: &VersionRequirement,
        candidates: &[Version],
    ) -> CoreResult<Option<Version>> {
        match_ruby_version(requirement, candidates)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyRuntimeSource {
    Registered,
    Installed,
    CandidatePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubyRuntime {
    version: Version,
    root: PathBuf,
    source: RubyRuntimeSource,
    platform: Option<Platform>,
}

impl RubyRuntime {
    pub fn new(
        version: Version,
        root: impl Into<PathBuf>,
        source: RubyRuntimeSource,
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

    pub fn source(&self) -> &RubyRuntimeSource {
        &self.source
    }

    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }
}

#[derive(Debug, Clone, Default)]
pub struct RubyRuntimeDiscovery {
    candidate_roots: Vec<PathBuf>,
}

impl RubyRuntimeDiscovery {
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
    ) -> CoreResult<Vec<RubyRuntime>> {
        let ruby = ruby_tool_name();
        let mut runtimes = Vec::new();

        for runtime in registry.list_registered_runtimes(&ruby) {
            if runtime.platform() == platform {
                runtimes.push(ruby_runtime_from_registered(runtime)?);
            }
        }

        for installation in install_store.list_installations(&ruby) {
            if installation.platform() == platform {
                runtimes.push(ruby_runtime_from_installation(installation)?);
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

pub fn validate_ruby_home(root: impl AsRef<Path>) -> CoreResult<RubyRuntime> {
    let root = canonical_ruby_home(root.as_ref())?;

    if !root.is_dir() {
        return Err(CoreError::message(format!(
            "invalid Ruby runtime `{}`: expected a Ruby runtime directory",
            root.display()
        )));
    }

    for binary in ["ruby", "gem"] {
        let path = root.join("bin").join(binary);
        if !path.is_file() {
            return Err(CoreError::message(format!(
                "invalid Ruby runtime `{}`: missing `{}`",
                root.display(),
                path.display()
            )));
        }
    }

    let version = read_ruby_version(&root)?;

    Ok(RubyRuntime::new(
        Version::new(version)?,
        root,
        RubyRuntimeSource::CandidatePath,
        None,
    ))
}

pub fn match_ruby_runtime<'a>(
    requirement: &VersionRequirement,
    runtimes: &'a [RubyRuntime],
) -> CoreResult<Option<&'a RubyRuntime>> {
    let versions = runtimes
        .iter()
        .map(|runtime| runtime.version().clone())
        .collect::<Vec<_>>();
    let Some(version) = match_ruby_version(requirement, &versions)? else {
        return Ok(None);
    };

    Ok(runtimes
        .iter()
        .find(|runtime| runtime.version().raw() == version.raw()))
}

pub fn match_ruby_version(
    requirement: &VersionRequirement,
    candidates: &[Version],
) -> CoreResult<Option<Version>> {
    if let Some(exact) = candidates
        .iter()
        .find(|candidate| candidate.raw() == requirement.raw())
    {
        return Ok(Some(exact.clone()));
    }

    let requirement = RubyVersionKey::parse(requirement.raw())?;
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let candidate_key = RubyVersionKey::parse(candidate.raw()).ok()?;
            candidate_key
                .matches_requirement(&requirement)
                .then_some((candidate, candidate_key))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|(_, left), (_, right)| right.cmp(left));

    Ok(matches.first().map(|(version, _)| (*version).clone()))
}

pub fn normalize_ruby_version(value: &str) -> CoreResult<String> {
    Ok(RubyVersionKey::parse(value)?.normalized)
}

fn discover_candidate_root(root: &Path) -> CoreResult<Vec<RubyRuntime>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    if let Ok(runtime) = validate_ruby_home(root) {
        return Ok(vec![runtime]);
    }

    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Ruby candidate directory `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Ruby candidate directory `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();

        if let Ok(runtime) = validate_ruby_home(&path) {
            runtimes.push(runtime);
        }
    }

    Ok(runtimes)
}

fn ruby_runtime_from_registered(runtime: RegisteredRuntime) -> CoreResult<RubyRuntime> {
    let root = canonical_ruby_home(runtime.root()).unwrap_or_else(|_| runtime.root().to_path_buf());
    Ok(RubyRuntime::new(
        runtime.version().clone(),
        root,
        RubyRuntimeSource::Registered,
        Some(runtime.platform()),
    ))
}

fn ruby_runtime_from_installation(installation: Installation) -> CoreResult<RubyRuntime> {
    let root = canonical_ruby_home(installation.root())?;
    let version = if root.as_path() == installation.root() {
        installation.version().clone()
    } else {
        validate_ruby_home(&root)?.version().clone()
    };

    Ok(RubyRuntime::new(
        version,
        root,
        RubyRuntimeSource::Installed,
        Some(installation.platform()),
    ))
}

fn canonical_ruby_home(root: &Path) -> CoreResult<PathBuf> {
    if root.join("bin/ruby").is_file() {
        return Ok(root.to_path_buf());
    }

    if !root.is_dir() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Ruby runtime `{}`: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Ruby runtime `{}`: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.join("bin/ruby").is_file() {
            candidates.push(path);
        }
    }

    Ok(candidates.pop().unwrap_or_else(|| root.to_path_buf()))
}

fn read_ruby_version(root: &Path) -> CoreResult<String> {
    for relative in ["VERSION", ".ruby-version"] {
        let path = root.join(relative);
        if path.is_file() {
            let version = std::fs::read_to_string(&path).map_err(|error| {
                CoreError::message(format!(
                    "invalid Ruby runtime `{}`: failed to read `{}` for version metadata: {error}",
                    root.display(),
                    path.display()
                ))
            })?;
            return first_version_line(root, &path, &version);
        }
    }

    if let Some(version) = read_ruby_version_header(root)? {
        return Ok(version);
    }

    if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
        if let Ok(version) = normalize_ruby_version(name) {
            return Ok(version);
        }
    }

    Err(CoreError::message(format!(
        "invalid Ruby runtime `{}`: missing version metadata. Expected VERSION, .ruby-version, include/ruby/version.h, or a versioned runtime directory name.",
        root.display()
    )))
}

fn read_ruby_version_header(root: &Path) -> CoreResult<Option<String>> {
    let include = root.join("include");
    if !include.is_dir() {
        return Ok(None);
    }

    for entry in std::fs::read_dir(&include).map_err(|error| {
        CoreError::message(format!(
            "failed to scan Ruby include directory `{}`: {error}",
            include.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            CoreError::message(format!(
                "failed to scan Ruby include directory `{}`: {error}",
                include.display()
            ))
        })?;
        let path = entry.path().join("ruby/version.h");
        if !path.is_file() {
            continue;
        }
        let header = std::fs::read_to_string(&path).map_err(|error| {
            CoreError::message(format!(
                "invalid Ruby runtime `{}`: failed to read `{}` for version metadata: {error}",
                root.display(),
                path.display()
            ))
        })?;
        for line in header.lines() {
            if line.contains("RUBY_VERSION") {
                let Some((_, value)) = line.split_once('"') else {
                    continue;
                };
                let Some((version, _)) = value.split_once('"') else {
                    continue;
                };
                return Ok(Some(normalize_ruby_version(version)?));
            }
        }
    }

    Ok(None)
}

fn first_version_line(root: &Path, path: &Path, contents: &str) -> CoreResult<String> {
    let version = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid Ruby runtime `{}`: missing version in `{}`",
                root.display(),
                path.display()
            ))
        })?;

    normalize_ruby_version(version)
}

fn compare_ruby_version_desc(left: &Version, right: &Version) -> Ordering {
    let left_key = RubyVersionKey::parse(left.raw());
    let right_key = RubyVersionKey::parse(right.raw());

    match (left_key, right_key) {
        (Ok(left), Ok(right)) => right.cmp(&left),
        _ => right.raw().cmp(left.raw()),
    }
}

fn runtime_sort(left: &RubyRuntime, right: &RubyRuntime) -> Ordering {
    compare_ruby_version_desc(left.version(), right.version())
        .then_with(|| left.root().cmp(right.root()))
}

fn ruby_tool_name() -> ToolName {
    ToolName::new("ruby").expect("built-in Ruby tool name should be valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RubyVersionKey {
    normalized: String,
    parts: Vec<u64>,
}

impl RubyVersionKey {
    fn parse(value: &str) -> CoreResult<Self> {
        let normalized = version_token(value).ok_or_else(|| {
            CoreError::message(format!(
                "invalid Ruby version `{}`: expected a numeric version such as 3.3.0",
                value.trim()
            ))
        })?;
        let numeric = normalized
            .split(['-', '+', 'p'])
            .next()
            .unwrap_or(normalized.as_str());
        let parts = numeric
            .split('.')
            .map(|part| {
                part.parse::<u64>().map_err(|error| {
                    CoreError::message(format!("invalid Ruby version `{normalized}`: {error}"))
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        if parts.is_empty() {
            return Err(CoreError::message(format!(
                "invalid Ruby version `{normalized}`: expected a numeric version"
            )));
        }

        Ok(Self { normalized, parts })
    }

    fn matches_requirement(&self, requirement: &RubyVersionKey) -> bool {
        self.normalized == requirement.normalized || self.parts.starts_with(&requirement.parts)
    }
}

impl Ord for RubyVersionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts
            .cmp(&other.parts)
            .then_with(|| self.normalized.cmp(&other.normalized))
    }
}

impl PartialOrd for RubyVersionKey {
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
