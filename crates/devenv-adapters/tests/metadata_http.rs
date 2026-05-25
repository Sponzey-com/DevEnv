use devenv_adapters::metadata_http::ReqwestMetadataHttpClient;
use devenv_core::{MetadataHttpClient, MetadataHttpRequest};

#[test]
fn reqwest_metadata_http_client_rejects_invalid_url() {
    let mut client = ReqwestMetadataHttpClient::new().expect("client should build");

    let error = client
        .fetch_metadata(&MetadataHttpRequest::new("not a url"))
        .expect_err("invalid URL should fail before network is used");

    assert!(error.to_string().contains("invalid metadata URL"));
}

#[test]
fn reqwest_metadata_http_client_rejects_unsupported_scheme() {
    let mut client = ReqwestMetadataHttpClient::new().expect("client should build");

    let error = client
        .fetch_metadata(&MetadataHttpRequest::new("file:///tmp/releases.json"))
        .expect_err("unsupported scheme should fail before network is used");

    assert!(error.to_string().contains("unsupported scheme"));
}
