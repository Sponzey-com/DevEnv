use devenv_core::{
    FakeMetadataHttpClient, MetadataFetchOutcome, MetadataHttpResponse,
    MetadataPayloadFetchRequest, fetch_metadata_payload,
};

#[test]
fn fake_http_200_response_returns_body_and_headers() {
    let response =
        MetadataHttpResponse::new(200, br#"{"ok":true}"#.to_vec()).with_header("etag", "\"abc\"");
    let mut client = FakeMetadataHttpClient::new(response);

    let outcome = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json"),
        &mut client,
    )
    .expect("metadata fetch should succeed");

    let MetadataFetchOutcome::Fetched(response) = outcome else {
        panic!("expected fetched outcome");
    };
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("etag"), Some("\"abc\""));
    assert_eq!(response.body(), br#"{"ok":true}"#);
    assert_eq!(client.calls().len(), 1);
}

#[test]
fn fake_http_304_response_is_cache_reuse_signal() {
    let response = MetadataHttpResponse::new(304, Vec::new()).with_header("etag", "\"abc\"");
    let mut client = FakeMetadataHttpClient::new(response);

    let outcome = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json"),
        &mut client,
    )
    .expect("metadata fetch should succeed");

    let MetadataFetchOutcome::NotModified { headers } = outcome else {
        panic!("expected not modified outcome");
    };
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"abc\""));
}

#[test]
fn fake_http_404_maps_to_actionable_provider_error() {
    let mut client = FakeMetadataHttpClient::new(MetadataHttpResponse::new(404, "missing"));

    let error = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json"),
        &mut client,
    )
    .expect_err("404 should fail");

    let message = error.to_string();
    assert!(message.contains("status 404"));
    assert!(message.contains("provider metadata endpoint"));
    assert!(message.contains("fixture override"));
}

#[test]
fn fake_http_500_marks_error_retryable() {
    let mut client = FakeMetadataHttpClient::new(MetadataHttpResponse::new(500, "failed"));

    let error = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json"),
        &mut client,
    )
    .expect_err("500 should fail");

    let message = error.to_string();
    assert!(message.contains("status 500"));
    assert!(message.contains("retryable=true"));
}

#[test]
fn timeout_error_is_mapped_by_http_client() {
    let mut client = FakeMetadataHttpClient::failing(
        "metadata HTTP request timed out for `https://example.invalid/releases.json`",
    );

    let error = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json")
            .with_timeout_seconds(1),
        &mut client,
    )
    .expect_err("timeout should fail");

    assert!(error.to_string().contains("timed out"));
}

#[test]
fn body_size_limit_is_enforced_after_fetch() {
    let mut client = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, "too large"));

    let error = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json")
            .with_max_body_bytes(3),
        &mut client,
    )
    .expect_err("oversized body should fail");

    assert!(error.to_string().contains("exceeded max body size"));
}

#[test]
fn offline_mode_does_not_call_http_client() {
    let mut client = FakeMetadataHttpClient::new(MetadataHttpResponse::new(200, "unused"));

    let outcome = fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json").offline(),
        &mut client,
    )
    .expect("offline fetch should return outcome");

    assert!(matches!(outcome, MetadataFetchOutcome::Offline { .. }));
    assert!(client.calls().is_empty());
}

#[test]
fn conditional_headers_are_included_in_request() {
    let mut client = FakeMetadataHttpClient::new(MetadataHttpResponse::new(304, Vec::new()));

    fetch_metadata_payload(
        MetadataPayloadFetchRequest::new("https://example.invalid/releases.json")
            .with_etag("\"abc\"")
            .with_last_modified("Fri, 22 May 2026 00:00:00 GMT"),
        &mut client,
    )
    .expect("conditional fetch should succeed");

    let request = client
        .calls()
        .first()
        .expect("request should have been recorded");
    assert_eq!(request.header("If-None-Match"), Some("\"abc\""));
    assert_eq!(
        request.header("If-Modified-Since"),
        Some("Fri, 22 May 2026 00:00:00 GMT")
    );
}
