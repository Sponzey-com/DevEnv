use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use devenv_core::{
    CoreError, CoreResult, MetadataHttpClient, MetadataHttpRequest, MetadataHttpResponse,
};
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;

#[derive(Debug, Clone)]
pub struct ReqwestMetadataHttpClient {
    client: Client,
}

impl ReqwestMetadataHttpClient {
    pub fn new() -> CoreResult<Self> {
        let client = Client::builder()
            .redirect(Policy::limited(10))
            .build()
            .map_err(|error| {
                CoreError::message(format!("failed to build metadata HTTP client: {error}"))
            })?;
        Ok(Self { client })
    }

    fn validate_url(url: &str) -> CoreResult<Url> {
        let parsed = Url::parse(url).map_err(|error| {
            CoreError::message(format!("invalid metadata URL `{url}`: {error}"))
        })?;
        match parsed.scheme() {
            "http" | "https" => Ok(parsed),
            scheme => Err(CoreError::message(format!(
                "invalid metadata URL `{url}`: unsupported scheme `{scheme}`"
            ))),
        }
    }

    fn request_headers(request: &MetadataHttpRequest) -> CoreResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        for (name, value) in request.headers() {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                CoreError::message(format!(
                    "invalid metadata HTTP header name `{name}`: {error}"
                ))
            })?;
            let value = HeaderValue::from_str(value).map_err(|error| {
                CoreError::message(format!(
                    "invalid metadata HTTP header value for `{name}`: {error}"
                ))
            })?;
            headers.insert(name, value);
        }
        Ok(headers)
    }
}

impl Default for ReqwestMetadataHttpClient {
    fn default() -> Self {
        Self::new().expect("default reqwest metadata HTTP client should build")
    }
}

impl MetadataHttpClient for ReqwestMetadataHttpClient {
    fn fetch_metadata(
        &mut self,
        request: &MetadataHttpRequest,
    ) -> CoreResult<MetadataHttpResponse> {
        let url = Self::validate_url(request.url())?;
        let headers = Self::request_headers(request)?;
        let timeout = Duration::from_secs(request.timeout_seconds());

        let mut response = self
            .client
            .get(url.clone())
            .headers(headers)
            .timeout(timeout)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    CoreError::message(format!(
                        "metadata HTTP request timed out for `{}` after {}s",
                        request.url(),
                        request.timeout_seconds()
                    ))
                } else {
                    CoreError::message(format!(
                        "metadata HTTP request failed for `{}`: {error}",
                        request.url()
                    ))
                }
            })?;

        let status = response.status().as_u16();
        let headers = response_headers(response.headers());
        let mut body = Vec::new();
        let limit = u64::try_from(request.max_body_bytes())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        response
            .by_ref()
            .take(limit)
            .read_to_end(&mut body)
            .map_err(|error| {
                CoreError::message(format!(
                    "failed to read metadata HTTP response from `{}`: {error}",
                    request.url()
                ))
            })?;
        if body.len() > request.max_body_bytes() {
            return Err(CoreError::message(format!(
                "metadata HTTP response from `{}` exceeded max body size of {} bytes",
                request.url(),
                request.max_body_bytes()
            )));
        }

        Ok(MetadataHttpResponse::new(status, body).with_headers(headers))
    }
}

fn response_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_owned()))
        })
        .collect()
}

trait MetadataHttpResponseExt {
    fn with_headers(self, headers: BTreeMap<String, String>) -> Self;
}

impl MetadataHttpResponseExt for MetadataHttpResponse {
    fn with_headers(mut self, headers: BTreeMap<String, String>) -> Self {
        for (name, value) in headers {
            self = self.with_header(name, value);
        }
        self
    }
}
