use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use devenv_adapters::checksum::hex_sha256;
use serde::{Deserialize, Serialize};

pub type CatalogResult<T> = Result<T, CatalogError>;

#[derive(Debug)]
pub struct CatalogError {
    message: String,
}

impl CatalogError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CatalogError {}

impl From<std::io::Error> for CatalogError {
    fn from(error: std::io::Error) -> Self {
        Self::message(error.to_string())
    }
}

impl From<serde_json::Error> for CatalogError {
    fn from(error: serde_json::Error) -> Self {
        Self::message(error.to_string())
    }
}

impl From<toml::de::Error> for CatalogError {
    fn from(error: toml::de::Error) -> Self {
        Self::message(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub source_dir: PathBuf,
    pub output_dir: PathBuf,
    pub generated_at: String,
    pub expires_at: String,
    pub catalog_version: String,
    pub sequence: u64,
    pub min_devenv_version: String,
    pub overrides_path: Option<PathBuf>,
}

impl GenerateOptions {
    pub fn new(
        source_dir: impl Into<PathBuf>,
        output_dir: impl Into<PathBuf>,
        generated_at: impl Into<String>,
        expires_at: impl Into<String>,
        catalog_version: impl Into<String>,
    ) -> Self {
        Self {
            source_dir: source_dir.into(),
            output_dir: output_dir.into(),
            generated_at: generated_at.into(),
            expires_at: expires_at.into(),
            catalog_version: catalog_version.into(),
            sequence: 1,
            min_devenv_version: "0.1.0".to_owned(),
            overrides_path: None,
        }
    }

    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    pub fn with_min_devenv_version(mut self, version: impl Into<String>) -> Self {
        self.min_devenv_version = version.into();
        self
    }

    pub fn with_overrides_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.overrides_path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateSummary {
    pub manifest_path: PathBuf,
    pub entries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifySummary {
    pub entries: usize,
}

pub fn generate_catalog(options: &GenerateOptions) -> CatalogResult<GenerateSummary> {
    let overrides = match &options.overrides_path {
        Some(path) => CatalogOverrides::read(path)?,
        None => CatalogOverrides::default(),
    };
    fs::create_dir_all(&options.output_dir)?;

    let mut entries = Vec::new();
    if options
        .source_dir
        .join("go/official/releases.json")
        .is_file()
    {
        let payload = generate_go_payload(options, &overrides)?;
        let path = PathBuf::from("tools/go/official/releases.json");
        let bytes = write_json(&options.output_dir.join(&path), &payload)?;
        entries.push(manifest_entry_from_payload(
            "go",
            "official",
            path,
            bytes.as_slice(),
            payload.platforms(),
            BTreeMap::from([
                ("channel".to_owned(), "stable".to_owned()),
                ("distribution".to_owned(), "go".to_owned()),
            ]),
        ));
    }
    if options
        .source_dir
        .join("node/official/index.json")
        .is_file()
    {
        let payload = generate_node_payload(options, &overrides)?;
        let path = PathBuf::from("tools/node/official/releases.json");
        let bytes = write_json(&options.output_dir.join(&path), &payload)?;
        entries.push(manifest_entry_from_payload(
            "node",
            "official",
            path,
            bytes.as_slice(),
            payload.platforms(),
            BTreeMap::from([
                ("channel".to_owned(), "stable".to_owned()),
                ("distribution".to_owned(), "node".to_owned()),
                ("implementation".to_owned(), "node".to_owned()),
            ]),
        ));
    }
    if entries.is_empty() {
        return Err(CatalogError::message(format!(
            "no supported upstream metadata found under `{}`",
            options.source_dir.display()
        )));
    }
    entries.sort_by(|left, right| {
        left.tool
            .cmp(&right.tool)
            .then_with(|| left.provider.cmp(&right.provider))
    });

    let manifest = CatalogManifestPayload {
        schema_version: 1,
        catalog_id: "dev.devenv.catalog".to_owned(),
        generated_at: options.generated_at.clone(),
        expires_at: options.expires_at.clone(),
        catalog_version: options.catalog_version.clone(),
        min_devenv_version: options.min_devenv_version.clone(),
        sequence: options.sequence,
        entries,
        metadata: BTreeMap::from([
            (
                "publisher".to_owned(),
                "DevEnv catalog generator".to_owned(),
            ),
            (
                "generator".to_owned(),
                format!("devenv-catalog/{}", env!("CARGO_PKG_VERSION")),
            ),
        ]),
    };
    let manifest_path = options.output_dir.join("manifest.json");
    let manifest_bytes = write_json(&manifest_path, &manifest)?;
    fs::write(
        options.output_dir.join("manifest.sig"),
        format!("sha256:{}\n", hex_sha256(&manifest_bytes)),
    )?;

    Ok(GenerateSummary {
        manifest_path,
        entries: manifest.entries.len(),
    })
}

pub fn verify_catalog(catalog_dir: impl AsRef<Path>) -> CatalogResult<VerifySummary> {
    let catalog_dir = catalog_dir.as_ref();
    let manifest_path = catalog_dir.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        CatalogError::message(format!(
            "failed to read manifest `{}`: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: CatalogManifestPayload =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            CatalogError::message(format!(
                "failed to parse manifest `{}`: {error}",
                manifest_path.display()
            ))
        })?;
    if manifest.schema_version != 1 {
        return Err(CatalogError::message(format!(
            "unsupported catalog manifest schema version {}",
            manifest.schema_version
        )));
    }

    let sig_path = catalog_dir.join("manifest.sig");
    if sig_path.is_file() {
        let signature = fs::read_to_string(&sig_path)?;
        let expected = format!("sha256:{}", hex_sha256(&manifest_bytes));
        if signature.trim() != expected {
            return Err(CatalogError::message(format!(
                "manifest signature mismatch: expected `{expected}`"
            )));
        }
    }

    for entry in &manifest.entries {
        if entry.payload_kind != "normalized-release-index" {
            return Err(CatalogError::message(format!(
                "unsupported payload kind `{}` for {}/{}",
                entry.payload_kind, entry.tool, entry.provider
            )));
        }
        let payload_path = catalog_dir.join(&entry.path);
        let payload_bytes = fs::read(&payload_path).map_err(|error| {
            CatalogError::message(format!(
                "failed to read payload `{}`: {error}",
                payload_path.display()
            ))
        })?;
        let actual_sha = format!("sha256:{}", hex_sha256(&payload_bytes));
        if actual_sha != entry.sha256 {
            return Err(CatalogError::message(format!(
                "payload checksum mismatch for `{}`: expected {}, got {}",
                entry.path, entry.sha256, actual_sha
            )));
        }
        let payload: CatalogToolPayload =
            serde_json::from_slice(&payload_bytes).map_err(|error| {
                CatalogError::message(format!(
                    "failed to parse payload `{}`: {error}",
                    payload_path.display()
                ))
            })?;
        if payload.schema_version != 1 {
            return Err(CatalogError::message(format!(
                "unsupported payload schema version {} for `{}`",
                payload.schema_version, entry.path
            )));
        }
        if payload.tool != entry.tool || payload.provider != entry.provider {
            return Err(CatalogError::message(format!(
                "payload `{}` tool/provider mismatch: manifest has {}/{}, payload has {}/{}",
                entry.path, entry.tool, entry.provider, payload.tool, payload.provider
            )));
        }
    }

    Ok(VerifySummary {
        entries: manifest.entries.len(),
    })
}

fn generate_go_payload(
    options: &GenerateOptions,
    overrides: &CatalogOverrides,
) -> CatalogResult<CatalogToolPayload> {
    let path = options.source_dir.join("go/official/releases.json");
    let input = fs::read_to_string(&path)?;
    let mut releases = serde_json::from_str::<Vec<GoOfficialRelease>>(&input)?;
    releases.sort_by(|left, right| {
        compare_versions_desc(
            &normalize_go_version(&left.version),
            &normalize_go_version(&right.version),
        )
    });

    let mut normalized_releases = Vec::new();
    for release in releases {
        let version = normalize_go_version(&release.version);
        let override_entry = overrides.find("go", &version);
        let stable = override_entry
            .and_then(|entry| entry.stable)
            .unwrap_or(release.stable);
        let yanked = override_entry.map(|entry| entry.yanked).unwrap_or(false);
        let deprecated = override_entry
            .map(|entry| entry.deprecated)
            .unwrap_or(false);
        let reason = override_entry.and_then(|entry| entry.reason.clone());
        let mut artifacts = release
            .files
            .into_iter()
            .filter(|file| file.kind.as_deref().unwrap_or("archive") == "archive")
            .filter_map(|file| go_artifact_from_official_file(&version, file))
            .collect::<Vec<_>>();
        artifacts.sort_by(|left, right| left.filename.cmp(&right.filename));
        normalized_releases.push(CatalogReleasePayload {
            version: version.clone(),
            normalized_version: version.clone(),
            aliases: version_aliases(&version),
            release_date: None,
            selectors: BTreeMap::from([
                ("channel".to_owned(), "stable".to_owned()),
                ("distribution".to_owned(), "go".to_owned()),
                ("stable".to_owned(), stable.to_string()),
            ]),
            stable,
            yanked,
            deprecated,
            reason,
            yanked_reason: override_entry.and_then(|entry| entry.reason.clone()),
            notes_url: Some(format!("https://go.dev/doc/devel/release#go{}", version)),
            upstream_version: Some(format!("go{version}")),
            artifacts,
        });
    }

    Ok(CatalogToolPayload::new(
        "go",
        "official",
        &options.generated_at,
        "official-api",
        vec!["https://go.dev/dl/?mode=json".to_owned()],
        normalized_releases,
    ))
}

fn generate_node_payload(
    options: &GenerateOptions,
    overrides: &CatalogOverrides,
) -> CatalogResult<CatalogToolPayload> {
    let path = options.source_dir.join("node/official/index.json");
    let input = fs::read_to_string(&path)?;
    let mut releases = serde_json::from_str::<Vec<NodeOfficialRelease>>(&input)?;
    releases.sort_by(|left, right| {
        compare_versions_desc(
            &normalize_node_version(&left.version),
            &normalize_node_version(&right.version),
        )
    });

    let mut normalized_releases = Vec::new();
    let mut source_urls = vec!["https://nodejs.org/dist/index.json".to_owned()];
    for release in releases {
        let version = normalize_node_version(&release.version);
        let checksums = read_node_checksums(&options.source_dir, &version)?;
        if !checksums.is_empty() {
            source_urls.push(format!("https://nodejs.org/dist/v{version}/SHASUMS256.txt"));
        }
        let override_entry = overrides.find("node", &version);
        let stable = override_entry
            .and_then(|entry| entry.stable)
            .unwrap_or(true);
        let yanked = override_entry.map(|entry| entry.yanked).unwrap_or(false);
        let deprecated = override_entry
            .map(|entry| entry.deprecated)
            .unwrap_or(false);
        let reason = override_entry.and_then(|entry| entry.reason.clone());
        let mut artifacts = release
            .files
            .iter()
            .filter_map(|token| node_artifact_from_token(&version, token, &checksums))
            .collect::<Vec<_>>();
        artifacts.sort_by(|left, right| left.filename.cmp(&right.filename));
        let mut aliases = version_aliases(&version);
        if release.lts.is_some() {
            aliases.push("lts".to_owned());
        }
        aliases.sort();
        aliases.dedup();
        normalized_releases.push(CatalogReleasePayload {
            version: version.clone(),
            normalized_version: version.clone(),
            aliases,
            release_date: release.date,
            selectors: BTreeMap::from([
                (
                    "channel".to_owned(),
                    if release.lts.is_some() {
                        "lts"
                    } else {
                        "current"
                    }
                    .to_owned(),
                ),
                ("distribution".to_owned(), "node".to_owned()),
                ("implementation".to_owned(), "node".to_owned()),
                ("stable".to_owned(), stable.to_string()),
            ]),
            stable,
            yanked,
            deprecated,
            reason,
            yanked_reason: override_entry.and_then(|entry| entry.reason.clone()),
            notes_url: Some(format!("https://nodejs.org/en/blog/release/v{version}")),
            upstream_version: Some(format!("v{version}")),
            artifacts,
        });
    }
    source_urls.sort();
    source_urls.dedup();

    Ok(CatalogToolPayload::new(
        "node",
        "official",
        &options.generated_at,
        "official-index",
        source_urls,
        normalized_releases,
    ))
}

fn go_artifact_from_official_file(
    _release_version: &str,
    file: GoOfficialFile,
) -> Option<CatalogArtifactPayload> {
    let platform = platform_from_go_fields(&file.os, &file.arch)?;
    let filename = file.filename;
    let checksum = file
        .sha256
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("sha256:{value}"));
    let installable = checksum.is_some();
    let archive_type = archive_type_for_filename(&filename);
    Some(CatalogArtifactPayload {
        platform: platform.clone(),
        os: platform.provider_os.clone(),
        arch: platform.provider_arch.clone(),
        url: file
            .url
            .unwrap_or_else(|| format!("https://go.dev/dl/{filename}")),
        filename,
        archive_type: archive_type.to_owned(),
        checksum_algorithm: checksum.as_ref().map(|_| "sha256".to_owned()),
        checksum,
        installable,
        install_block_reason: (!installable).then(|| "missing checksum".to_owned()),
        size: file.size,
        kind: "archive".to_owned(),
        metadata: BTreeMap::from([
            ("go_os".to_owned(), platform.provider_os),
            ("go_arch".to_owned(), platform.provider_arch),
            ("kind".to_owned(), "archive".to_owned()),
        ]),
    })
}

fn node_artifact_from_token(
    version: &str,
    token: &str,
    checksums: &BTreeMap<String, String>,
) -> Option<CatalogArtifactPayload> {
    let (filename, os, arch) = node_file_from_token(version, token)?;
    let platform = platform_from_node_fields(os, arch)?;
    let checksum = checksums.get(&filename).cloned();
    let installable = checksum.is_some();
    let archive_type = archive_type_for_filename(&filename);
    Some(CatalogArtifactPayload {
        platform: platform.clone(),
        os: platform.provider_os.clone(),
        arch: platform.provider_arch.clone(),
        url: format!("https://nodejs.org/dist/v{version}/{filename}"),
        filename,
        archive_type: archive_type.to_owned(),
        checksum_algorithm: checksum.as_ref().map(|_| "sha256".to_owned()),
        checksum,
        installable,
        install_block_reason: (!installable).then(|| "missing checksum".to_owned()),
        size: None,
        kind: "archive".to_owned(),
        metadata: BTreeMap::from([
            ("node_os".to_owned(), platform.provider_os),
            ("node_arch".to_owned(), platform.provider_arch),
            ("kind".to_owned(), "archive".to_owned()),
        ]),
    })
}

fn read_node_checksums(
    source_dir: &Path,
    version: &str,
) -> CatalogResult<BTreeMap<String, String>> {
    let candidates = [
        source_dir
            .join("node/official/shasums")
            .join(format!("v{version}"))
            .join("SHASUMS256.txt"),
        source_dir
            .join("node/official")
            .join(format!("v{version}"))
            .join("SHASUMS256.txt"),
    ];
    let Some(path) = candidates.into_iter().find(|path| path.is_file()) else {
        return Ok(BTreeMap::new());
    };
    parse_shasums256(&fs::read_to_string(path)?)
}

fn parse_shasums256(input: &str) -> CatalogResult<BTreeMap<String, String>> {
    let mut checksums = BTreeMap::new();
    for (index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let checksum = parts.next().ok_or_else(|| {
            CatalogError::message(format!(
                "invalid checksum line {}: missing checksum",
                index + 1
            ))
        })?;
        let filename = parts.next().ok_or_else(|| {
            CatalogError::message(format!(
                "invalid checksum line {}: missing filename",
                index + 1
            ))
        })?;
        if checksum.len() != 64
            || !checksum
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(CatalogError::message(format!(
                "invalid checksum line {}: expected sha256 hex digest",
                index + 1
            )));
        }
        checksums.insert(filename.to_owned(), format!("sha256:{checksum}"));
    }
    Ok(checksums)
}

fn write_json(path: &Path, value: &impl Serialize) -> CatalogResult<Vec<u8>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, &bytes)?;
    Ok(bytes)
}

fn manifest_entry_from_payload(
    tool: &str,
    provider: &str,
    path: PathBuf,
    payload_bytes: &[u8],
    platforms: Vec<String>,
    selector: BTreeMap<String, String>,
) -> ManifestEntryPayload {
    ManifestEntryPayload {
        tool: tool.to_owned(),
        provider: provider.to_owned(),
        path: path.to_string_lossy().replace('\\', "/"),
        sha256: format!("sha256:{}", hex_sha256(payload_bytes)),
        payload_kind: "normalized-release-index".to_owned(),
        ttl_seconds: 86_400,
        platforms,
        selector,
    }
}

fn compare_versions_desc(left: &str, right: &str) -> std::cmp::Ordering {
    version_key(right)
        .cmp(&version_key(left))
        .then_with(|| right.cmp(left))
}

fn version_key(value: &str) -> Vec<u64> {
    value
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn normalize_go_version(value: &str) -> String {
    value.trim().trim_start_matches("go").to_owned()
}

fn normalize_node_version(value: &str) -> String {
    value.trim().trim_start_matches('v').to_owned()
}

fn version_aliases(version: &str) -> Vec<String> {
    let parts = version.split('.').collect::<Vec<_>>();
    let mut aliases = Vec::new();
    if let Some(major) = parts.first() {
        aliases.push((*major).to_owned());
    }
    if parts.len() >= 2 {
        aliases.push(format!("{}.{}", parts[0], parts[1]));
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn archive_type_for_filename(filename: &str) -> &'static str {
    if filename.ends_with(".tar.gz") {
        "tar.gz"
    } else if filename.ends_with(".tar.xz") {
        "tar.xz"
    } else if filename.ends_with(".zip") {
        "zip"
    } else {
        "plain-file"
    }
}

fn platform_from_go_fields(os: &str, arch: &str) -> Option<CatalogPlatformPayload> {
    let normalized_os = match os {
        "darwin" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };
    let normalized_arch = match arch {
        "amd64" => "x64",
        "arm64" => "arm64",
        _ => return None,
    };
    Some(CatalogPlatformPayload {
        os: normalized_os.to_owned(),
        arch: normalized_arch.to_owned(),
        provider_os: os.to_owned(),
        provider_arch: arch.to_owned(),
    })
}

fn platform_from_node_fields(os: &str, arch: &str) -> Option<CatalogPlatformPayload> {
    let normalized_os = match os {
        "darwin" => "macos",
        "linux" => "linux",
        "win" => "windows",
        _ => return None,
    };
    let normalized_arch = match arch {
        "x64" => "x64",
        "arm64" => "arm64",
        _ => return None,
    };
    Some(CatalogPlatformPayload {
        os: normalized_os.to_owned(),
        arch: normalized_arch.to_owned(),
        provider_os: os.to_owned(),
        provider_arch: arch.to_owned(),
    })
}

fn node_file_from_token(
    version: &str,
    token: &str,
) -> Option<(String, &'static str, &'static str)> {
    match token {
        "linux-x64" => Some((format!("node-v{version}-linux-x64.tar.gz"), "linux", "x64")),
        "linux-arm64" => Some((
            format!("node-v{version}-linux-arm64.tar.gz"),
            "linux",
            "arm64",
        )),
        "osx-x64-tar" | "darwin-x64" => Some((
            format!("node-v{version}-darwin-x64.tar.gz"),
            "darwin",
            "x64",
        )),
        "osx-arm64-tar" | "darwin-arm64" => Some((
            format!("node-v{version}-darwin-arm64.tar.gz"),
            "darwin",
            "arm64",
        )),
        "win-x64-zip" => Some((format!("node-v{version}-win-x64.zip"), "win", "x64")),
        "win-arm64-zip" => Some((format!("node-v{version}-win-arm64.zip"), "win", "arm64")),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CatalogOverrides {
    #[serde(default)]
    release: Vec<ReleaseOverride>,
}

impl CatalogOverrides {
    fn read(path: &Path) -> CatalogResult<Self> {
        toml::from_str(&fs::read_to_string(path)?).map_err(CatalogError::from)
    }

    fn find(&self, tool: &str, version: &str) -> Option<&ReleaseOverride> {
        self.release
            .iter()
            .find(|entry| entry.tool == tool && normalize_node_version(&entry.version) == version)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseOverride {
    tool: String,
    version: String,
    #[serde(default)]
    stable: Option<bool>,
    #[serde(default)]
    yanked: bool,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoOfficialRelease {
    version: String,
    #[serde(default = "default_true")]
    stable: bool,
    #[serde(default)]
    files: Vec<GoOfficialFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoOfficialFile {
    filename: String,
    os: String,
    arch: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NodeOfficialRelease {
    version: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    lts: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
struct CatalogPlatformPayload {
    os: String,
    arch: String,
    provider_os: String,
    provider_arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogManifestPayload {
    schema_version: u32,
    catalog_id: String,
    generated_at: String,
    expires_at: String,
    catalog_version: String,
    min_devenv_version: String,
    sequence: u64,
    entries: Vec<ManifestEntryPayload>,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntryPayload {
    tool: String,
    provider: String,
    path: String,
    sha256: String,
    payload_kind: String,
    ttl_seconds: u64,
    platforms: Vec<String>,
    selector: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogToolPayload {
    schema_version: u32,
    tool: String,
    provider: String,
    generated_at: String,
    source: CatalogSourcePayload,
    releases: Vec<CatalogReleasePayload>,
}

impl CatalogToolPayload {
    fn new(
        tool: &str,
        provider: &str,
        generated_at: &str,
        source_kind: &str,
        source_urls: Vec<String>,
        releases: Vec<CatalogReleasePayload>,
    ) -> Self {
        Self {
            schema_version: 1,
            tool: tool.to_owned(),
            provider: provider.to_owned(),
            generated_at: generated_at.to_owned(),
            source: CatalogSourcePayload {
                kind: source_kind.to_owned(),
                urls: source_urls,
                retrieved_at: generated_at.to_owned(),
                generator: format!("devenv-catalog/{}", env!("CARGO_PKG_VERSION")),
            },
            releases,
        }
    }

    fn platforms(&self) -> Vec<String> {
        let mut platforms = self
            .releases
            .iter()
            .flat_map(|release| release.artifacts.iter())
            .filter(|artifact| artifact.installable)
            .map(|artifact| format!("{}-{}", artifact.platform.os, artifact.platform.arch))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        platforms.sort();
        platforms
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogSourcePayload {
    kind: String,
    urls: Vec<String>,
    retrieved_at: String,
    generator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogReleasePayload {
    version: String,
    normalized_version: String,
    aliases: Vec<String>,
    release_date: Option<String>,
    selectors: BTreeMap<String, String>,
    stable: bool,
    yanked: bool,
    deprecated: bool,
    reason: Option<String>,
    yanked_reason: Option<String>,
    notes_url: Option<String>,
    upstream_version: Option<String>,
    artifacts: Vec<CatalogArtifactPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogArtifactPayload {
    platform: CatalogPlatformPayload,
    url: String,
    filename: String,
    archive_type: String,
    checksum: Option<String>,
    checksum_algorithm: Option<String>,
    installable: bool,
    install_block_reason: Option<String>,
    os: String,
    arch: String,
    kind: String,
    size: Option<u64>,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogPlatformForWire {
    os: String,
    arch: String,
}

impl From<CatalogPlatformPayload> for CatalogPlatformForWire {
    fn from(value: CatalogPlatformPayload) -> Self {
        Self {
            os: value.os,
            arch: value.arch,
        }
    }
}

impl Serialize for CatalogPlatformPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        CatalogPlatformForWire {
            os: self.os.clone(),
            arch: self.arch.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CatalogPlatformPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CatalogPlatformForWire::deserialize(deserializer)?;
        Ok(Self {
            provider_os: wire.os.clone(),
            provider_arch: wire.arch.clone(),
            os: wire.os,
            arch: wire.arch,
        })
    }
}

impl From<CatalogPlatformPayload> for String {
    fn from(value: CatalogPlatformPayload) -> Self {
        format!("{}-{}", value.os, value.arch)
    }
}
