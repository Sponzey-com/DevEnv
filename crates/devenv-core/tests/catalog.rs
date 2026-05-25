use devenv_core::{
    CATALOG_MANIFEST_SCHEMA_VERSION, CatalogEntry, CatalogFetchRequest, CatalogFetchResponse,
    CatalogManifest, CatalogPayloadDescriptor, CatalogPayloadKind, CatalogSource,
    CatalogTrustFailure, CatalogTrustVerifier, CatalogVerificationResult, CoreError,
    FakeCatalogSource, FakeCatalogTrustVerifier, MetadataCacheKey, MetadataFetchMode, ProviderId,
    ProviderSourceKind, ToolName, TrustRoot,
};

fn catalog_key(tool: &str, provider: &str) -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new(tool).expect("tool should parse"),
        ProviderId::new(provider).expect("provider should parse"),
    )
}

fn catalog_descriptor(path: &str) -> CatalogPayloadDescriptor {
    CatalogPayloadDescriptor::new(
        path,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        CatalogPayloadKind::NormalizedReleaseIndex,
        86_400,
    )
}

fn catalog_entry(tool: &str, provider: &str, path: &str) -> CatalogEntry {
    CatalogEntry::new(catalog_key(tool, provider), catalog_descriptor(path))
}

#[test]
fn catalog_manifest_models_required_fields_and_entries() {
    let entry = catalog_entry("go", "official", "go/official/releases.json");
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2026-05-29T00:00:00Z",
        "2026.05.22.1",
        "0.1.0",
        7,
        [entry.clone()],
    );

    assert_eq!(manifest.schema_version(), CATALOG_MANIFEST_SCHEMA_VERSION);
    assert_eq!(manifest.catalog_id(), "dev.devenv.catalog");
    assert_eq!(manifest.generated_at(), "2026-05-22T00:00:00Z");
    assert_eq!(manifest.expires_at(), "2026-05-29T00:00:00Z");
    assert_eq!(manifest.catalog_version(), "2026.05.22.1");
    assert_eq!(manifest.min_devenv_version(), "0.1.0");
    assert_eq!(manifest.sequence(), 7);
    assert_eq!(manifest.entries(), &[entry]);
}

#[test]
fn catalog_manifest_finds_entry_by_tool_provider_key() {
    let go_entry = catalog_entry("go", "official", "go/official/releases.json");
    let node_entry = catalog_entry("node", "official", "node/official/releases.json");
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2026-05-29T00:00:00Z",
        "2026.05.22.1",
        "0.1.0",
        1,
        [go_entry, node_entry],
    );
    let key = catalog_key("node", "official");

    let entry = manifest.entry_for(&key).expect("node entry should exist");

    assert_eq!(entry.tool().as_str(), "node");
    assert_eq!(entry.provider().as_str(), "official");
    assert_eq!(entry.descriptor().path(), "node/official/releases.json");
    assert_eq!(
        entry.descriptor().payload_kind().as_str(),
        "normalized-release-index"
    );
    assert_eq!(entry.descriptor().ttl_seconds(), 86_400);
}

#[test]
fn catalog_manifest_detects_expiration_without_network_context() {
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2026-05-29T00:00:00Z",
        "2026.05.22.1",
        "0.1.0",
        1,
        [catalog_entry("go", "official", "go/official/releases.json")],
    );

    assert!(!manifest.is_expired_at("2026-05-28T23:59:59Z"));
    assert!(manifest.is_expired_at("2026-05-29T00:00:00Z"));
    assert!(manifest.is_expired_at("2026-05-30T00:00:00Z"));
}

#[test]
fn catalog_manifest_compares_unix_clock_with_utc_expiration() {
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2000-01-01T00:00:00Z",
        "2026.05.22.1",
        "0.1.0",
        1,
        [catalog_entry("go", "official", "go/official/releases.json")],
    );

    assert!(!manifest.is_expired_at("unix:946684799"));
    assert!(manifest.is_expired_at("unix:946684800"));
}

#[test]
fn catalog_manifest_detects_min_devenv_version_mismatch() {
    let manifest = CatalogManifest::new(
        "dev.devenv.catalog",
        "2026-05-22T00:00:00Z",
        "2026-05-29T00:00:00Z",
        "2026.05.22.1",
        "0.2.0",
        1,
        [catalog_entry("go", "official", "go/official/releases.json")],
    );

    assert!(manifest.requires_newer_devenv("0.1.9"));
    assert!(!manifest.requires_newer_devenv("0.2.0"));
    assert!(!manifest.requires_newer_devenv("0.3.0"));
}

#[test]
fn catalog_trust_result_models_unknown_trust_root() {
    let failure = CatalogTrustFailure::UnknownTrustRoot {
        trust_root_id: "mirror-key".to_owned(),
    };
    let result = CatalogVerificationResult::rejected(failure.clone());

    assert!(!result.is_trusted());
    assert!(matches!(
        result.failure(),
        Some(CatalogTrustFailure::UnknownTrustRoot { trust_root_id })
            if trust_root_id == "mirror-key"
    ));
    assert!(failure.to_string().contains("unknown catalog trust root"));
}

#[test]
fn catalog_trust_failure_models_expiration_and_version_mismatch() {
    let expired = CatalogTrustFailure::ExpiredCatalog {
        catalog_id: "dev.devenv.catalog".to_owned(),
        expires_at: "2026-05-22T00:00:00Z".to_owned(),
        now: "2026-05-23T00:00:00Z".to_owned(),
    };
    let version_mismatch = CatalogTrustFailure::MinDevenvVersionMismatch {
        required: "0.2.0".to_owned(),
        current: "0.1.0".to_owned(),
    };

    assert!(expired.to_string().contains("expired at"));
    assert!(
        version_mismatch
            .to_string()
            .contains("requires DevEnv 0.2.0")
    );
}

#[test]
fn catalog_fake_trust_verifier_returns_success_and_failure() {
    let root = TrustRoot::new("builtin", "sha256:root");
    let mut verifier = FakeCatalogTrustVerifier::passing("builtin");

    let result = verifier
        .verify_manifest(b"{\"schema_version\":1}", b"signature", &root)
        .expect("fake verifier should return success");

    assert!(result.is_trusted());
    assert_eq!(result.trust_root_id(), Some("builtin"));
    assert_eq!(verifier.calls().len(), 1);
    assert_eq!(verifier.calls()[0].manifest_len(), 20);
    assert_eq!(verifier.calls()[0].signature_len(), 9);
    assert_eq!(verifier.calls()[0].trust_root(), &root);

    let mut verifier = FakeCatalogTrustVerifier::failing(CatalogTrustFailure::SignatureMismatch {
        reason: "fixture mismatch".to_owned(),
    });
    let result = verifier
        .verify_manifest(b"manifest", b"bad-signature", &root)
        .expect("fake verifier should return a trust result");

    assert!(!result.is_trusted());
    assert!(matches!(
        result.failure(),
        Some(CatalogTrustFailure::SignatureMismatch { reason })
            if reason == "fixture mismatch"
    ));
}

#[test]
fn catalog_fake_source_records_manifest_and_payload_requests() {
    let manifest_response =
        CatalogFetchResponse::new("fixture:manifest", "2026-05-22T00:00:00Z", b"manifest");
    let payload_response =
        CatalogFetchResponse::new("fixture:payload", "2026-05-22T00:00:01Z", b"payload");
    let mut source = FakeCatalogSource::new(manifest_response.clone());
    source.push_payload_response(payload_response.clone());
    let request = CatalogFetchRequest::new("fixture:catalog")
        .with_fetch_mode(MetadataFetchMode::Offline)
        .with_max_manifest_bytes(1024)
        .with_max_payload_bytes(2048);
    let descriptor = catalog_descriptor("go/official/releases.json");

    let fetched_manifest = source
        .fetch_manifest(&request)
        .expect("manifest should be returned");
    let fetched_payload = source
        .fetch_payload(&descriptor)
        .expect("payload should be returned");

    assert_eq!(fetched_manifest, manifest_response);
    assert_eq!(fetched_payload, payload_response);
    assert_eq!(source.manifest_calls(), &[request]);
    assert_eq!(source.payload_calls(), &[descriptor]);
}

#[test]
fn catalog_trust_failure_is_distinct_from_catalog_network_failure() {
    let trust_error = CoreError::catalog_trust(CatalogTrustFailure::SignatureMismatch {
        reason: "invalid signature".to_owned(),
    });
    let network_error = CoreError::catalog_network("timeout fetching manifest");

    assert!(matches!(trust_error, CoreError::CatalogTrust(_)));
    assert!(matches!(network_error, CoreError::CatalogNetwork(_)));
    assert!(trust_error.to_string().contains("catalog trust failure"));
    assert!(
        network_error
            .to_string()
            .contains("catalog network failure")
    );
}

#[test]
fn catalog_provider_source_kind_is_visible_to_capability_metadata() {
    assert_eq!(ProviderSourceKind::Catalog.as_str(), "catalog");
}
