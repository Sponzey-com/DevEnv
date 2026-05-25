use std::fs;

use devenv_adapters::catalog::CatalogFetchAdapter;
use devenv_adapters::checksum::hex_sha256;
use devenv_core::{
    CatalogEntry, CatalogFetchRequest, CatalogManifest, CatalogPayloadDescriptor,
    CatalogPayloadKind, CatalogSource, CatalogTrustFailure, CoreError, FakeCatalogTrustVerifier,
    FakeMetadataHttpClient, MetadataCacheKey, MetadataHttpResponse, ProviderId, ToolName,
    TrustRoot,
};
use reqwest::Url;

fn catalog_key(tool: &str, provider: &str) -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new(tool).expect("tool should parse"),
        ProviderId::new(provider).expect("provider should parse"),
    )
}

fn descriptor(path: &str, bytes: &[u8]) -> CatalogPayloadDescriptor {
    CatalogPayloadDescriptor::new(
        path,
        format!("sha256:{}", hex_sha256(bytes)),
        CatalogPayloadKind::NormalizedReleaseIndex,
        86_400,
    )
}

fn catalog_root_url(path: &std::path::Path) -> String {
    Url::from_directory_path(path)
        .expect("temp catalog path should become file URL")
        .to_string()
}

#[test]
fn catalog_fetches_manifest_over_fake_http() {
    let root = "https://example.invalid/catalog/v1";
    let http = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, b"manifest"));
    let mut adapter = CatalogFetchAdapter::new(root, http)
        .expect("adapter should build")
        .with_fetched_at("2026-05-22T00:00:00Z");

    let response = adapter
        .fetch_manifest(&CatalogFetchRequest::new(root))
        .expect("manifest should be fetched");

    assert_eq!(response.bytes(), b"manifest");
    assert_eq!(response.fetched_at(), "2026-05-22T00:00:00Z");
    assert_eq!(
        response.source_reference(),
        "https://example.invalid/catalog/v1/manifest.json"
    );
}

#[test]
fn catalog_fetches_payload_over_fake_http_and_verifies_checksum() {
    let bytes = br#"{"schema_version":1}"#;
    let http = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, bytes.as_slice()));
    let mut adapter = CatalogFetchAdapter::new("https://example.invalid/catalog/v1/", http)
        .expect("adapter should build");
    let descriptor = descriptor("go/official/releases.json", bytes);

    let response = adapter
        .fetch_payload(&descriptor)
        .expect("payload should be fetched");

    assert_eq!(response.bytes(), bytes);
    assert_eq!(
        response.source_reference(),
        "https://example.invalid/catalog/v1/go/official/releases.json"
    );
}

#[test]
fn catalog_fetches_manifest_and_payload_from_file_url() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let payload_dir = temp.path().join("go/official");
    fs::create_dir_all(&payload_dir).expect("payload directory should be created");
    fs::write(temp.path().join("manifest.json"), b"manifest").expect("manifest should be written");
    let payload = br#"{"schema_version":1}"#;
    fs::write(payload_dir.join("releases.json"), payload).expect("payload should be written");
    let root = catalog_root_url(temp.path());
    let http = FakeMetadataHttpClient::default();
    let mut adapter = CatalogFetchAdapter::new(&root, http).expect("adapter should build");

    let manifest = adapter
        .fetch_manifest(&CatalogFetchRequest::new(&root))
        .expect("manifest should be fetched");
    let payload = adapter
        .fetch_payload(&descriptor("go/official/releases.json", payload))
        .expect("payload should be fetched");

    assert_eq!(manifest.bytes(), b"manifest");
    assert_eq!(payload.bytes(), br#"{"schema_version":1}"#);
    assert!(payload.source_reference().starts_with("file://"));
}

#[test]
fn catalog_payload_checksum_mismatch_is_trust_failure() {
    let bytes = b"payload";
    let http = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, bytes.as_slice()));
    let mut adapter = CatalogFetchAdapter::new("https://example.invalid/catalog/v1/", http)
        .expect("adapter should build");
    let descriptor = CatalogPayloadDescriptor::new(
        "go/official/releases.json",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        CatalogPayloadKind::NormalizedReleaseIndex,
        86_400,
    );

    let error = adapter
        .fetch_payload(&descriptor)
        .expect_err("checksum mismatch should fail");

    assert!(matches!(
        error,
        CoreError::CatalogTrust(CatalogTrustFailure::ChecksumMismatch { .. })
    ));
    assert!(error.to_string().contains("checksum mismatch"));
    assert!(error.to_string().contains("go/official/releases.json"));
}

#[test]
fn catalog_signature_failure_is_trust_failure() {
    let root = "https://example.invalid/catalog/v1/";
    let mut http = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, b"manifest"));
    http.push_response(MetadataHttpResponse::new(200, b"bad-signature"));
    let mut adapter = CatalogFetchAdapter::new(root, http).expect("adapter should build");
    let mut verifier = FakeCatalogTrustVerifier::failing(CatalogTrustFailure::SignatureMismatch {
        reason: "fixture signature mismatch".to_owned(),
    });
    let trust_root = TrustRoot::new("builtin", "sha256:root");

    let error = adapter
        .fetch_and_verify_manifest(&CatalogFetchRequest::new(root), &mut verifier, &trust_root)
        .expect_err("signature failure should reject manifest");

    assert!(matches!(
        error,
        CoreError::CatalogTrust(CatalogTrustFailure::SignatureMismatch { .. })
    ));
    assert!(error.to_string().contains("fixture signature mismatch"));
}

#[test]
fn catalog_rejects_manifest_entry_path_traversal() {
    let http = FakeMetadataHttpClient::default();
    let mut adapter = CatalogFetchAdapter::new("https://example.invalid/catalog/v1/", http)
        .expect("adapter should build");
    let descriptor = CatalogPayloadDescriptor::new(
        "../outside.json",
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        CatalogPayloadKind::NormalizedReleaseIndex,
        86_400,
    );

    let error = adapter
        .fetch_payload(&descriptor)
        .expect_err("path traversal should be rejected");

    assert!(error.to_string().contains("path traversal"));
}

#[test]
fn catalog_rejects_missing_manifest_entry_before_fetching_payload() {
    let http = FakeMetadataHttpClient::default();
    let mut adapter = CatalogFetchAdapter::new("https://example.invalid/catalog/v1/", http)
        .expect("adapter should build");
    let go_entry = CatalogEntry::new(
        catalog_key("go", "official"),
        descriptor("go/official/releases.json", b"payload"),
    );
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2026-05-29T00:00:00Z",
        "2026.05.22.1",
        "0.1.0",
        1,
        [go_entry],
    );

    let error = adapter
        .fetch_manifest_entry_payload(&manifest, &catalog_key("node", "official"))
        .expect_err("missing manifest entry should fail");

    assert!(error.to_string().contains("does not contain an entry"));
    assert!(error.to_string().contains("node/official"));
}

#[test]
fn catalog_rejects_payload_larger_than_configured_limit() {
    let bytes = b"payload";
    let http = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, bytes.as_slice()));
    let mut adapter = CatalogFetchAdapter::new("https://example.invalid/catalog/v1/", http)
        .expect("adapter should build")
        .with_max_payload_bytes(3);
    let descriptor = descriptor("go/official/releases.json", bytes);

    let error = adapter
        .fetch_payload(&descriptor)
        .expect_err("oversized payload should fail");

    assert!(error.to_string().contains("exceeded max size"));
    assert!(error.to_string().contains("got 7 bytes"));
}

#[test]
fn catalog_rejects_invalid_root_url() {
    let http = FakeMetadataHttpClient::default();

    let error = CatalogFetchAdapter::new("ftp://example.invalid/catalog/v1", http)
        .expect_err("unsupported catalog root should fail");

    assert!(error.to_string().contains("supported schemes"));
}
