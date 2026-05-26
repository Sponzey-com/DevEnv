use std::fs;

use devenv_adapters::metadata_cache::FileMetadataCache;
use devenv_adapters::store::DevEnvHome;
use devenv_core::{
    MetadataCache, MetadataCacheEntry, MetadataCacheKey, MetadataCacheStatus, MetadataPayloadKind,
    ProviderId, StaticClock, ToolName,
};

const ABC_SHA256: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn metadata_cache_reports_missing_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let cache = FileMetadataCache::new(temp.path().join("cache/metadata"));

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:100"))
        .expect("missing cache should be readable");

    assert!(matches!(status, MetadataCacheStatus::Missing));
    assert_eq!(status.as_str(), "missing");
}

#[test]
fn metadata_cache_reports_fresh_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let mut cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    cache
        .write_metadata(raw_entry("unix:100", 60))
        .expect("cache entry should be written");

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:159"))
        .expect("cache status should be readable");

    let MetadataCacheStatus::Fresh(entry) = status else {
        panic!("cache should be fresh");
    };
    assert_eq!(entry.payload(), "abc");
    assert_eq!(entry.payload_sha256(), ABC_SHA256);
}

#[test]
fn metadata_cache_reports_stale_cache_after_ttl() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let mut cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    cache
        .write_metadata(raw_entry("unix:100", 60))
        .expect("cache entry should be written");

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:161"))
        .expect("cache status should be readable");

    assert!(matches!(status, MetadataCacheStatus::Stale(_)));
    assert_eq!(status.as_str(), "stale");
}

#[test]
fn metadata_cache_reports_corrupt_json_as_corrupt_status() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    let path = cache.cache_path(&go_key()).expect("path should resolve");
    fs::create_dir_all(path.parent().expect("path should have parent"))
        .expect("cache directory should be created");
    fs::write(&path, "{").expect("corrupt cache should be written");

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:100"))
        .expect("corrupt cache should be represented as status");

    let MetadataCacheStatus::Corrupt { reason } = status else {
        panic!("cache should be corrupt");
    };
    assert!(reason.contains("failed to parse metadata cache"));
}

#[test]
fn metadata_cache_reports_unsupported_schema_as_corrupt_status() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let mut cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    cache
        .write_metadata(raw_entry("unix:100", 60))
        .expect("cache entry should be written");
    let path = cache.cache_path(&go_key()).expect("path should resolve");
    let contents = fs::read_to_string(&path)
        .expect("cache entry should be readable")
        .replace("\"schema_version\": 1", "\"schema_version\": 999");
    fs::write(&path, contents).expect("cache entry should be corrupted");

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:100"))
        .expect("invalid schema should be represented as status");

    let MetadataCacheStatus::Corrupt { reason } = status else {
        panic!("cache should be corrupt");
    };
    assert!(reason.contains("unsupported schema version"));
}

#[test]
fn metadata_cache_detects_payload_hash_mismatch() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let mut cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    cache
        .write_metadata(raw_entry("unix:100", 60))
        .expect("cache entry should be written");
    let path = cache.cache_path(&go_key()).expect("path should resolve");
    let contents = fs::read_to_string(&path)
        .expect("cache entry should be readable")
        .replace(ABC_SHA256, "sha256:000000");
    fs::write(&path, contents).expect("cache entry should be corrupted");

    let status = cache
        .metadata_status(&go_key(), &StaticClock::new("unix:100"))
        .expect("hash mismatch should be represented as status");

    let MetadataCacheStatus::Corrupt { reason } = status else {
        panic!("cache should be corrupt");
    };
    assert!(reason.contains("payload sha256 mismatch"));
}

#[test]
fn metadata_cache_writes_and_reads_roundtrip_in_temp_directory() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let home = DevEnvHome::new(temp.path().join("devenv"));
    home.create_layout().expect("layout should be created");
    let mut cache = FileMetadataCache::at_home(&home);
    let entry = MetadataCacheEntry::new(
        go_key().with_selector("stable"),
        "https://go.dev/dl/?mode=json&include=all",
        "unix:100",
        3600,
        ABC_SHA256,
        MetadataPayloadKind::Normalized,
        "abc",
    )
    .with_validator_metadata("etag", "\"abc\"");

    cache
        .write_metadata(entry.clone())
        .expect("cache entry should be written");

    assert_eq!(
        cache
            .read_metadata(entry.cache_key())
            .expect("cache entry should be readable"),
        Some(entry)
    );
}

#[test]
fn metadata_cache_rejects_path_traversal_segments() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let cache = FileMetadataCache::new(temp.path().join("cache/metadata"));
    let key = go_key().with_selector("../stable");

    let error = cache
        .cache_path(&key)
        .expect_err("path traversal should be rejected");

    assert!(
        error
            .to_string()
            .contains("invalid metadata cache selector")
    );
}

#[cfg(unix)]
#[test]
fn metadata_cache_remove_rejects_symlink_escape_from_owned_root() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let cache_root = temp.path().join("devenv/cache/metadata");
    let outside = temp.path().join("outside-cache-target");
    let mut cache = FileMetadataCache::new(&cache_root);
    fs::create_dir_all(cache_root.join("go")).expect("cache tool directory should be created");
    fs::create_dir_all(&outside).expect("outside directory should be created");
    std::os::unix::fs::symlink(&outside, cache_root.join("go/official"))
        .expect("symlink should be created");
    fs::write(outside.join("metadata.json"), "outside")
        .expect("outside cache target should be written");

    let error = cache
        .remove_metadata(&go_key())
        .expect_err("symlink escape should be rejected");

    assert!(error.to_string().contains("outside owned metadata cache"));
    assert!(outside.join("metadata.json").exists());
}

fn go_key() -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new("go").expect("tool should be valid"),
        ProviderId::new("official").expect("provider should be valid"),
    )
}

fn raw_entry(fetched_at: &str, ttl_seconds: u64) -> MetadataCacheEntry {
    MetadataCacheEntry::new(
        go_key(),
        "https://go.dev/dl/?mode=json&include=all",
        fetched_at,
        ttl_seconds,
        ABC_SHA256,
        MetadataPayloadKind::Raw,
        "abc",
    )
}
