use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use devenv_core::{
    Clock, CoreError, CoreResult, METADATA_CACHE_SCHEMA_VERSION, MetadataCache, MetadataCacheEntry,
    MetadataCacheKey, MetadataCacheStatus, MetadataFreshness, MetadataPayloadKind, ProviderId,
    ToolName,
};
use serde::{Deserialize, Serialize};

use crate::checksum::hex_sha256;
use crate::store::DevEnvHome;

#[derive(Debug, Clone)]
pub struct FileMetadataCache {
    metadata_cache_dir: PathBuf,
}

impl FileMetadataCache {
    pub fn new(metadata_cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            metadata_cache_dir: metadata_cache_dir.into(),
        }
    }

    pub fn at_home(home: &DevEnvHome) -> Self {
        Self::new(home.metadata_cache_dir())
    }

    pub fn metadata_cache_dir(&self) -> &Path {
        &self.metadata_cache_dir
    }

    pub fn cache_path(&self, key: &MetadataCacheKey) -> CoreResult<PathBuf> {
        let mut path = self
            .metadata_cache_dir
            .join(cache_segment(key.tool().as_str(), "tool")?)
            .join(cache_segment(key.provider().as_str(), "provider")?);

        if let Some(selector) = key.selector() {
            path = path.join(cache_segment(selector, "selector")?);
        }

        Ok(path.join("metadata.json"))
    }

    fn read_status_entry(&self, key: &MetadataCacheKey) -> CoreResult<ReadStatus> {
        let path = self.cache_path(key)?;
        if !path.exists() {
            return Ok(ReadStatus::Missing);
        }

        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                return Ok(ReadStatus::Corrupt(format!(
                    "failed to read metadata cache `{}`: {error}",
                    path.display()
                )));
            }
        };

        let envelope = match serde_json::from_str::<MetadataCacheEnvelope>(&contents) {
            Ok(envelope) => envelope,
            Err(error) => {
                return Ok(ReadStatus::Corrupt(format!(
                    "failed to parse metadata cache `{}`: {error}",
                    path.display()
                )));
            }
        };

        match envelope.try_into_entry(key) {
            Ok(entry) => Ok(ReadStatus::Entry(entry)),
            Err(error) => Ok(ReadStatus::Corrupt(format!(
                "invalid metadata cache `{}`: {error}",
                path.display()
            ))),
        }
    }

    fn ensure_owned_cache_path(&self, path: &Path) -> CoreResult<()> {
        let boundary = canonical_owned_boundary(&self.metadata_cache_dir)?;
        let candidate = canonical_for_safety(path);
        if candidate.starts_with(&boundary) && candidate != boundary {
            Ok(())
        } else {
            Err(CoreError::message(format!(
                "metadata cache path `{}` is outside owned metadata cache `{}`",
                path.display(),
                boundary.display()
            )))
        }
    }
}

impl MetadataCache for FileMetadataCache {
    fn write_metadata(&mut self, entry: MetadataCacheEntry) -> CoreResult<()> {
        let path = self.cache_path(entry.cache_key())?;
        self.ensure_owned_cache_path(&path)?;
        let parent = path.parent().ok_or_else(|| {
            CoreError::message(format!(
                "invalid metadata cache path `{}`: missing parent",
                path.display()
            ))
        })?;
        std::fs::create_dir_all(parent).map_err(|error| {
            CoreError::message(format!(
                "failed to create metadata cache directory `{}`: {error}",
                parent.display()
            ))
        })?;

        let envelope = MetadataCacheEnvelope::from_entry(&entry);
        let contents = serde_json::to_string_pretty(&envelope).map_err(|error| {
            CoreError::message(format!("failed to encode metadata cache envelope: {error}"))
        })?;
        std::fs::write(&path, contents).map_err(|error| {
            CoreError::message(format!(
                "failed to write metadata cache `{}`: {error}",
                path.display()
            ))
        })
    }

    fn read_metadata(&self, key: &MetadataCacheKey) -> CoreResult<Option<MetadataCacheEntry>> {
        match self.read_status_entry(key)? {
            ReadStatus::Missing => Ok(None),
            ReadStatus::Entry(entry) => Ok(Some(entry)),
            ReadStatus::Corrupt(reason) => Err(CoreError::message(reason)),
        }
    }

    fn remove_metadata(&mut self, key: &MetadataCacheKey) -> CoreResult<bool> {
        let path = self.cache_path(key)?;
        self.ensure_owned_cache_path(&path)?;
        if !path.exists() {
            return Ok(false);
        }

        std::fs::remove_file(&path).map_err(|error| {
            CoreError::message(format!(
                "failed to remove metadata cache `{}`: {error}",
                path.display()
            ))
        })?;
        Ok(true)
    }

    fn metadata_status(
        &self,
        key: &MetadataCacheKey,
        clock: &dyn Clock,
    ) -> CoreResult<MetadataCacheStatus> {
        match self.read_status_entry(key)? {
            ReadStatus::Missing => Ok(MetadataCacheStatus::Missing),
            ReadStatus::Corrupt(reason) => Ok(MetadataCacheStatus::Corrupt { reason }),
            ReadStatus::Entry(entry) => match entry.freshness_at(&clock.now_utc()?) {
                MetadataFreshness::Fresh => Ok(MetadataCacheStatus::Fresh(entry)),
                MetadataFreshness::Stale => Ok(MetadataCacheStatus::Stale(entry)),
                MetadataFreshness::Corrupt => Ok(MetadataCacheStatus::Corrupt {
                    reason: "metadata cache entry has an invalid timestamp".to_owned(),
                }),
            },
        }
    }
}

enum ReadStatus {
    Missing,
    Entry(MetadataCacheEntry),
    Corrupt(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct MetadataCacheEnvelope {
    schema_version: u32,
    tool: String,
    provider: String,
    selector: Option<String>,
    source_url: String,
    fetched_at: String,
    ttl_seconds: u64,
    validator_metadata: BTreeMap<String, String>,
    payload_sha256: String,
    payload_kind: String,
    payload: String,
}

impl MetadataCacheEnvelope {
    fn from_entry(entry: &MetadataCacheEntry) -> Self {
        Self {
            schema_version: METADATA_CACHE_SCHEMA_VERSION,
            tool: entry.cache_key().tool().as_str().to_owned(),
            provider: entry.cache_key().provider().as_str().to_owned(),
            selector: entry.cache_key().selector().map(str::to_owned),
            source_url: entry.source_url().to_owned(),
            fetched_at: entry.fetched_at().to_owned(),
            ttl_seconds: entry.ttl_seconds(),
            validator_metadata: entry.validator_metadata().clone(),
            payload_sha256: entry.payload_sha256().to_owned(),
            payload_kind: entry.payload_kind().as_str().to_owned(),
            payload: entry.payload().to_owned(),
        }
    }

    fn try_into_entry(self, expected_key: &MetadataCacheKey) -> CoreResult<MetadataCacheEntry> {
        if self.schema_version != METADATA_CACHE_SCHEMA_VERSION {
            return Err(CoreError::message(format!(
                "unsupported schema version `{}`",
                self.schema_version
            )));
        }

        let tool = ToolName::new(&self.tool).map_err(CoreError::from)?;
        let provider = ProviderId::new(&self.provider).map_err(CoreError::from)?;
        let mut key = MetadataCacheKey::new(tool, provider);
        if let Some(selector) = self.selector {
            key = key.with_selector(selector);
        }
        if &key != expected_key {
            return Err(CoreError::message("cache key does not match requested key"));
        }

        let actual_sha256 = format!("sha256:{}", hex_sha256(self.payload.as_bytes()));
        if self.payload_sha256 != actual_sha256 {
            return Err(CoreError::message(format!(
                "payload sha256 mismatch: expected {}, got {}",
                self.payload_sha256, actual_sha256
            )));
        }

        let payload_kind = match self.payload_kind.as_str() {
            "raw" => MetadataPayloadKind::Raw,
            "normalized" => MetadataPayloadKind::Normalized,
            other => {
                return Err(CoreError::message(format!(
                    "unsupported payload kind `{other}`"
                )));
            }
        };

        let mut entry = MetadataCacheEntry::new(
            key,
            self.source_url,
            self.fetched_at,
            self.ttl_seconds,
            self.payload_sha256,
            payload_kind,
            self.payload,
        );
        for (key, value) in self.validator_metadata {
            entry = entry.with_validator_metadata(key, value);
        }
        Ok(entry)
    }
}

fn cache_segment(value: &str, label: &str) -> CoreResult<String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CoreError::message(format!(
            "invalid metadata cache {label} segment `{value}`"
        )));
    }

    Ok(value.to_owned())
}

fn canonical_owned_boundary(path: &Path) -> CoreResult<PathBuf> {
    if path.exists() {
        path.canonicalize().map_err(|error| {
            CoreError::message(format!(
                "failed to canonicalize metadata cache root `{}`: {error}",
                path.display()
            ))
        })
    } else {
        Ok(canonical_for_safety(path))
    }
}

fn canonical_for_safety(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let mut missing_segments = Vec::new();
    let mut cursor = path;
    while !cursor.exists() {
        let Some(name) = cursor.file_name() else {
            return path.to_path_buf();
        };
        missing_segments.push(name.to_owned());
        let Some(parent) = cursor.parent() else {
            return path.to_path_buf();
        };
        cursor = parent;
    }

    let mut canonical = cursor
        .canonicalize()
        .unwrap_or_else(|_| cursor.to_path_buf());
    for segment in missing_segments.iter().rev() {
        canonical.push(segment);
    }
    canonical
}
